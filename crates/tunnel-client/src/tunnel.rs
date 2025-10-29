//! Tunnel protocol implementation for client

use crate::config::{ProtocolConfig, TunnelConfig};
use crate::metrics::MetricsStore;
use crate::TunnelError;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error, info, warn};
use tunnel_proto::{Endpoint, Protocol, TunnelMessage};
use tunnel_transport::{
    TransportConnection, TransportConnector as TransportConnectorTrait, TransportStream,
};
use tunnel_transport_quic::{QuicConfig, QuicConnector};

/// HTTP request data for processing
struct HttpRequestData {
    method: String,
    uri: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

/// Generate a short unique ID from stream_id (8 characters)
fn generate_short_id(stream_id: u32) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    stream_id.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:08x}", (hash as u32))
}

/// Generate a deterministic tunnel_id from auth token
/// This ensures the same token always gets the same tunnel_id (and thus same port/subdomain)
fn generate_tunnel_id_from_token(token: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    token.hash(&mut hasher);
    let hash = hasher.finish();

    // Format as UUID-like string for compatibility
    // Uses hash to generate deterministic values
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (hash >> 32) as u32,
        ((hash >> 16) & 0xFFFF) as u16,
        (hash & 0xFFFF) as u16,
        ((hash >> 48) & 0xFFFF) as u16,
        hash & 0xFFFFFFFFFFFF
    )
}

/// Tunnel connector - handles the tunnel protocol with the exit node
pub struct TunnelConnector {
    config: TunnelConfig,
}

impl TunnelConnector {
    pub fn new(config: TunnelConfig) -> Self {
        Self { config }
    }

    /// Parse relay address from various formats
    ///
    /// Supports:
    /// - `127.0.0.1:4443` (IP:port)
    /// - `localhost:4443` (hostname:port)
    /// - `relay.example.com:4443` (hostname:port)
    /// - `https://relay.example.com:4443` (URL with port)
    /// - `https://relay.example.com` (URL, defaults to port 4443)
    ///
    /// Returns: (hostname, SocketAddr)
    async fn parse_relay_address(
        addr_str: &str,
    ) -> Result<(String, std::net::SocketAddr), TunnelError> {
        // Remove protocol prefix if present (https://, http://, quic://)
        let addr_without_protocol = addr_str
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("quic://");

        // Try to parse as SocketAddr first (IP:port format like 127.0.0.1:4443)
        if let Ok(socket_addr) = addr_without_protocol.parse::<std::net::SocketAddr>() {
            // Extract hostname from IP for TLS SNI
            let hostname = socket_addr.ip().to_string();
            return Ok((hostname, socket_addr));
        }

        // Not a direct IP:port, must be hostname:port or just hostname
        let (hostname, port) = if let Some(colon_pos) = addr_without_protocol.rfind(':') {
            // Has port specified
            let host = &addr_without_protocol[..colon_pos];
            let port_str = &addr_without_protocol[colon_pos + 1..];

            let port: u16 = port_str.parse().map_err(|_| {
                TunnelError::ConnectionError(format!(
                    "Invalid port '{}' in relay address '{}'",
                    port_str, addr_str
                ))
            })?;

            (host.to_string(), port)
        } else {
            // No port specified, use default QUIC tunnel port
            (addr_without_protocol.to_string(), 4443)
        };

        // Resolve hostname to IP address
        let addr_with_port = format!("{}:{}", hostname, port);
        let socket_addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host(&addr_with_port)
            .await
            .map_err(|e| {
                TunnelError::ConnectionError(format!(
                    "Failed to resolve hostname '{}': {}",
                    hostname, e
                ))
            })?
            .collect();

        // Prefer IPv4 addresses (QUIC libraries often have better IPv4 support)
        let socket_addr = socket_addrs
            .iter()
            .find(|addr| addr.is_ipv4())
            .or_else(|| socket_addrs.first())
            .copied()
            .ok_or_else(|| {
                TunnelError::ConnectionError(format!(
                    "No addresses found for hostname '{}'",
                    hostname
                ))
            })?;

        Ok((hostname, socket_addr))
    }

    /// Connect to the exit node and establish tunnel
    pub async fn connect(self) -> Result<TunnelConnection, TunnelError> {
        // Parse relay address from config
        let relay_addr_str = match &self.config.exit_node {
            tunnel_proto::ExitNodeConfig::Custom(addr) => addr.clone(),
            _ => {
                return Err(TunnelError::ConnectionError(
                    "No custom relay specified. Use --relay flag.".to_string(),
                ))
            }
        };

        info!("Connecting to tunnel relay at {} (QUIC)", relay_addr_str);

        // Parse and resolve address (supports IP:port, hostname:port, or https://hostname:port)
        let (hostname, relay_addr) = Self::parse_relay_address(&relay_addr_str).await?;

        // Create QUIC connector with insecure mode (skip cert verification for localhost/dev)
        let quic_config = Arc::new(QuicConfig::client_insecure());
        let quic_connector = QuicConnector::new(quic_config).map_err(|e| {
            TunnelError::ConnectionError(format!("Failed to create QUIC connector: {}", e))
        })?;

        // Connect to tunnel control port using QUIC
        let connection = quic_connector
            .connect(relay_addr, &hostname)
            .await
            .map_err(|e| {
                TunnelError::ConnectionError(format!(
                    "Failed to connect to {}: {}",
                    relay_addr_str, e
                ))
            })?;

        let connection = Arc::new(connection);
        info!("✅ Connected to relay via QUIC");

        // Generate deterministic tunnel ID from auth token
        // This ensures the same token always gets the same tunnel_id (and thus same port/subdomain)
        let tunnel_id = generate_tunnel_id_from_token(&self.config.auth_token);
        info!("🎯 Using deterministic tunnel_id: {}", tunnel_id);

        // Convert ProtocolConfig to Protocol
        let protocols: Vec<Protocol> = self
            .config
            .protocols
            .iter()
            .map(|pc| match pc {
                ProtocolConfig::Http { subdomain, .. } => Protocol::Http {
                    // Send None if no subdomain - server will auto-generate one
                    subdomain: subdomain.clone(),
                },
                ProtocolConfig::Https { subdomain, .. } => Protocol::Https {
                    // Send None if no subdomain - server will auto-generate one
                    subdomain: subdomain.clone(),
                },
                ProtocolConfig::Tcp { remote_port, .. } => Protocol::Tcp {
                    // 0 means auto-allocate, specific port means request that port
                    port: remote_port.unwrap_or(0),
                },
                ProtocolConfig::Tls {
                    subdomain,
                    remote_port,
                    ..
                } => Protocol::Tls {
                    port: remote_port.unwrap_or(8443),
                    sni_pattern: subdomain.clone().unwrap_or_else(|| "*".to_string()),
                },
            })
            .collect();

        // Send Connect message
        let connect_msg = TunnelMessage::Connect {
            tunnel_id: tunnel_id.clone(),
            auth_token: self.config.auth_token.clone(),
            protocols: protocols.clone(),
            config: tunnel_proto::TunnelConfig {
                local_host: self.config.local_host.clone(),
                local_port: None,
                local_https: false,
                exit_node: self.config.exit_node.clone(),
                failover: self.config.failover,
                ip_allowlist: Vec::new(),
                enable_compression: false,
                enable_multiplexing: true,
            },
        };

        // Open control stream (first stream for control messages)
        let mut control_stream = connection.open_stream().await.map_err(|e| {
            TunnelError::ConnectionError(format!("Failed to open control stream: {}", e))
        })?;

        control_stream
            .send_message(&connect_msg)
            .await
            .map_err(|e| TunnelError::ConnectionError(format!("Failed to send Connect: {}", e)))?;

        debug!("Sent Connect message");

        // Wait for Connected response
        match control_stream.recv_message().await {
            Ok(Some(TunnelMessage::Connected {
                tunnel_id: tid,
                endpoints,
            })) => {
                info!("✅ Tunnel registered: {}", tid);
                for endpoint in &endpoints {
                    info!("🌍 Public URL: {}", endpoint.public_url);
                }

                Ok(TunnelConnection {
                    _connection: connection,
                    control_stream: Arc::new(tokio::sync::Mutex::new(control_stream)),
                    shutdown_tx: Arc::new(tokio::sync::Mutex::new(None)),
                    tunnel_id: tid,
                    endpoints,
                    config: self.config,
                    metrics: MetricsStore::default(),
                })
            }
            Ok(Some(TunnelMessage::Disconnect { reason })) => {
                // Check for specific error types and provide user-friendly messages
                if reason.contains("Authentication failed")
                    || reason.contains("JWT")
                    || reason.contains("InvalidToken")
                    || reason.contains("authentication")
                {
                    error!("❌ Authentication failed: {}", reason);
                    Err(TunnelError::AuthenticationFailed(reason))
                } else if reason.contains("Subdomain is already in use")
                    || reason.contains("Route already exists")
                {
                    error!("❌ {}", reason);
                    error!("💡 Tip: Try specifying a different subdomain with --subdomain or wait a moment and retry");
                    Err(TunnelError::ConnectionError(reason))
                } else {
                    error!("❌ Tunnel rejected: {}", reason);
                    Err(TunnelError::ConnectionError(reason))
                }
            }
            Ok(Some(other)) => {
                error!("Unexpected message: {:?}", other);
                Err(TunnelError::ConnectionError(
                    "Unexpected response".to_string(),
                ))
            }
            Ok(None) => {
                error!("Connection closed before receiving Connected message");
                Err(TunnelError::ConnectionError(
                    "Connection closed".to_string(),
                ))
            }
            Err(e) => {
                error!("Failed to read Connected message: {}", e);
                Err(TunnelError::ConnectionError(format!("{}", e)))
            }
        }
    }
}

use tunnel_transport_quic::{QuicConnection, QuicStream};

/// TCP stream manager to route data to active streams
type TcpStreamManager =
    Arc<tokio::sync::Mutex<std::collections::HashMap<u32, tokio::sync::mpsc::Sender<Vec<u8>>>>>;

/// Active tunnel connection
#[derive(Clone)]
pub struct TunnelConnection {
    _connection: Arc<QuicConnection>, // Kept alive to maintain QUIC connection
    control_stream: Arc<tokio::sync::Mutex<QuicStream>>,
    shutdown_tx: Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::Sender<()>>>>,
    tunnel_id: String,
    endpoints: Vec<Endpoint>,
    config: TunnelConfig,
    metrics: MetricsStore,
}

impl TunnelConnection {
    pub fn tunnel_id(&self) -> &str {
        &self.tunnel_id
    }

    pub fn endpoints(&self) -> &[Endpoint] {
        &self.endpoints
    }

    pub fn public_url(&self) -> Option<&str> {
        self.endpoints.first().map(|e| e.public_url.as_str())
    }

    /// Get access to the metrics store
    pub fn metrics(&self) -> &MetricsStore {
        &self.metrics
    }

    /// Send a graceful disconnect message to the exit node
    pub async fn disconnect(&self) -> Result<(), TunnelError> {
        info!("Triggering graceful disconnect");

        let mut shutdown_tx_guard = self.shutdown_tx.lock().await;
        if let Some(tx) = shutdown_tx_guard.take() {
            // Send shutdown signal to control stream task
            let _ = tx.send(()).await;
            info!("Disconnect signal sent");
        } else {
            warn!("Disconnect already triggered or run() not called");
        }

        Ok(())
    }

    /// Run the tunnel - handle incoming requests via multi-stream QUIC
    pub async fn run(self) -> Result<(), TunnelError> {
        info!("Tunnel running, waiting for requests...");

        let config = self.config.clone();
        let metrics = self.metrics.clone();
        let connection = self._connection.clone();

        // Create shutdown channel for graceful disconnect
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Store shutdown sender so disconnect() can trigger it
        {
            let mut guard = self.shutdown_tx.lock().await;
            *guard = Some(shutdown_tx);
        }

        // Keep control stream for ping/pong heartbeat only
        let control_stream_arc = self.control_stream.clone();
        let control_stream_task = tokio::spawn(async move {
            let mut control_stream = control_stream_arc.lock().await;
            loop {
                tokio::select! {
                    // Check for shutdown signal
                    _ = shutdown_rx.recv() => {
                        info!("Shutdown signal received, sending disconnect");
                        if let Err(e) = control_stream.send_message(&TunnelMessage::Disconnect {
                            reason: "Client shutdown".to_string(),
                        }).await {
                            warn!("Failed to send disconnect: {}", e);
                            break;
                        }

                        info!("Disconnect message sent, waiting for acknowledgment...");

                        // Wait for disconnect acknowledgment with 3-second timeout
                        let ack_result = tokio::time::timeout(
                            std::time::Duration::from_secs(3),
                            control_stream.recv_message()
                        ).await;

                        match ack_result {
                            Ok(Ok(Some(TunnelMessage::DisconnectAck { .. }))) => {
                                info!("✅ Disconnect acknowledged by server");
                            }
                            Ok(Ok(None)) => {
                                info!("Control stream closed before ack");
                            }
                            Ok(Err(e)) => {
                                warn!("Error waiting for disconnect ack: {}", e);
                            }
                            Err(_) => {
                                warn!("Disconnect ack timeout (server may be slow or disconnected)");
                            }
                            Ok(Ok(Some(msg))) => {
                                warn!("Unexpected message while waiting for ack: {:?}", msg);
                            }
                        }

                        break;
                    }
                    // Handle incoming messages
                    result = control_stream.recv_message() => {
                        match result {
                            Ok(Some(TunnelMessage::Ping { timestamp })) => {
                                debug!("Received ping on control stream");
                                if let Err(e) = control_stream.send_message(&TunnelMessage::Pong { timestamp }).await {
                                    error!("Failed to send pong: {}", e);
                                    break;
                                }
                            }
                            Ok(Some(TunnelMessage::Disconnect { reason })) => {
                                info!("Tunnel disconnected: {}", reason);
                                break;
                            }
                            Ok(None) => {
                                info!("Control stream closed");
                                break;
                            }
                            Err(e) => {
                                error!("Error on control stream: {}", e);
                                break;
                            }
                            Ok(Some(msg)) => {
                                warn!("Unexpected message on control stream: {:?}", msg);
                            }
                        }
                    }
                }
            }
            debug!("Control stream task exiting");
        });

        // Main loop: accept new QUIC streams from exit node
        loop {
            match connection.accept_stream().await {
                Ok(Some(mut stream)) => {
                    debug!("Accepted new QUIC stream: {}", stream.stream_id());

                    let config_clone = config.clone();
                    let metrics_clone = metrics.clone();

                    // Spawn handler for this stream
                    tokio::spawn(async move {
                        // Read first message to determine stream type
                        match stream.recv_message().await {
                            Ok(Some(TunnelMessage::HttpRequest {
                                stream_id,
                                method,
                                uri,
                                headers,
                                body,
                            })) => {
                                debug!(
                                    "HTTP request on stream {}: {} {}",
                                    stream.stream_id(),
                                    method,
                                    uri
                                );
                                Self::handle_http_stream(
                                    stream,
                                    &config_clone,
                                    &metrics_clone,
                                    stream_id,
                                    HttpRequestData {
                                        method,
                                        uri,
                                        headers,
                                        body,
                                    },
                                )
                                .await;
                            }
                            Ok(Some(TunnelMessage::HttpStreamConnect {
                                stream_id,
                                host,
                                initial_data,
                            })) => {
                                debug!(
                                    "HTTP transparent stream on stream {}: {} ({} bytes initial data)",
                                    stream.stream_id(),
                                    host,
                                    initial_data.len()
                                );
                                Self::handle_http_transparent_stream(
                                    stream,
                                    &config_clone,
                                    &metrics_clone,
                                    stream_id,
                                    initial_data,
                                )
                                .await;
                            }
                            Ok(Some(TunnelMessage::TcpConnect {
                                stream_id,
                                remote_addr,
                                remote_port,
                            })) => {
                                debug!(
                                    "TCP connect on stream {}: {}:{}",
                                    stream.stream_id(),
                                    remote_addr,
                                    remote_port
                                );
                                Self::handle_tcp_stream(
                                    stream,
                                    &config_clone,
                                    &metrics_clone,
                                    stream_id,
                                )
                                .await;
                            }
                            Ok(None) => {
                                debug!("Stream {} closed before first message", stream.stream_id());
                            }
                            Err(e) => {
                                error!(
                                    "Error reading first message from stream {}: {}",
                                    stream.stream_id(),
                                    e
                                );
                            }
                            Ok(Some(msg)) => {
                                warn!(
                                    "Unexpected first message on stream {}: {:?}",
                                    stream.stream_id(),
                                    msg
                                );
                            }
                        }
                    });
                }
                Ok(None) => {
                    info!("Connection closed, no more streams");
                    break;
                }
                Err(e) => {
                    error!("Error accepting stream: {}", e);
                    break;
                }
            }
        }

        // Wait for control stream task to finish
        let _ = control_stream_task.await;

        info!("Tunnel connection closed");
        Ok(())
    }

    /// Handle an HTTP request on a dedicated QUIC stream
    async fn handle_http_stream<S: TransportStream>(
        mut stream: S,
        config: &TunnelConfig,
        metrics: &MetricsStore,
        stream_id: u32,
        request: HttpRequestData,
    ) {
        // Process HTTP request using existing logic
        let response = Self::handle_http_request_static(
            config,
            metrics,
            stream_id,
            request.method,
            request.uri,
            request.headers,
            request.body,
        )
        .await;

        // Send response on THIS stream
        if let Err(e) = stream.send_message(&response).await {
            error!(
                "Failed to send HTTP response on stream {}: {}",
                stream.stream_id(),
                e
            );
        }

        // Close stream
        let _ = stream.finish().await;
    }

    /// Handle a TCP connection on a dedicated QUIC stream
    /// Handle transparent HTTP stream (for WebSocket, HTTP/2, SSE, etc.)
    async fn handle_http_transparent_stream(
        mut stream: tunnel_transport_quic::QuicStream,
        config: &TunnelConfig,
        _metrics: &MetricsStore,
        stream_id: u32,
        initial_data: Vec<u8>,
    ) {
        // Get local HTTP/HTTPS port from protocols
        let local_port = config.protocols.iter().find_map(|p| match p {
            ProtocolConfig::Http { local_port, .. } => Some(*local_port),
            ProtocolConfig::Https { local_port, .. } => Some(*local_port),
            _ => None,
        });

        let local_port = match local_port {
            Some(port) => port,
            None => {
                error!("No HTTP/HTTPS protocol configured for transparent streaming");
                let _ = stream
                    .send_message(&TunnelMessage::HttpStreamClose { stream_id })
                    .await;
                return;
            }
        };

        // Connect to local HTTP service
        let local_addr = format!("{}:{}", config.local_host, local_port);
        let mut local_socket = match TcpStream::connect(&local_addr).await {
            Ok(sock) => sock,
            Err(e) => {
                error!(
                    "Failed to connect to local HTTP service at {}: {}",
                    local_addr, e
                );
                let _ = stream
                    .send_message(&TunnelMessage::HttpStreamClose { stream_id })
                    .await;
                return;
            }
        };

        debug!(
            "Connected to local HTTP service at {} for transparent stream {}",
            local_addr, stream_id
        );

        // Write initial HTTP request data to local server
        if let Err(e) = local_socket.write_all(&initial_data).await {
            error!(
                "Failed to write initial data to local server (stream {}): {}",
                stream_id, e
            );
            let _ = stream
                .send_message(&TunnelMessage::HttpStreamClose { stream_id })
                .await;
            return;
        }

        debug!(
            "Wrote {} bytes initial data to local server (stream {})",
            initial_data.len(),
            stream_id
        );

        // Split streams for bidirectional communication
        let (mut local_read, mut local_write) = local_socket.into_split();
        let (mut quic_send, mut quic_recv) = stream.split();

        // Task: Local → Tunnel (read from local server, send to tunnel)
        let local_to_tunnel = tokio::spawn(async move {
            let mut buffer = vec![0u8; 16384];
            loop {
                match local_read.read(&mut buffer).await {
                    Ok(0) => {
                        debug!("Local HTTP server closed connection (stream {})", stream_id);
                        let _ = quic_send
                            .send_message(&TunnelMessage::HttpStreamClose { stream_id })
                            .await;
                        break;
                    }
                    Ok(n) => {
                        debug!(
                            "Read {} bytes from local HTTP server (stream {})",
                            n, stream_id
                        );
                        let data_msg = TunnelMessage::HttpStreamData {
                            stream_id,
                            data: buffer[..n].to_vec(),
                        };
                        if let Err(e) = quic_send.send_message(&data_msg).await {
                            error!("Failed to send HttpStreamData to tunnel: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!(
                            "Error reading from local HTTP server (stream {}): {}",
                            stream_id, e
                        );
                        break;
                    }
                }
            }
        });

        // Task: Tunnel → Local (read from tunnel, send to local server)
        let tunnel_to_local = tokio::spawn(async move {
            loop {
                match quic_recv.recv_message().await {
                    Ok(Some(TunnelMessage::HttpStreamData { data, .. })) => {
                        debug!(
                            "Received {} bytes from tunnel (stream {})",
                            data.len(),
                            stream_id
                        );
                        if let Err(e) = local_write.write_all(&data).await {
                            error!(
                                "Failed to write to local HTTP server (stream {}): {}",
                                stream_id, e
                            );
                            break;
                        }
                    }
                    Ok(Some(TunnelMessage::HttpStreamClose { .. })) => {
                        debug!("Tunnel closed HTTP stream {}", stream_id);
                        break;
                    }
                    Ok(None) => {
                        debug!("QUIC stream ended (stream {})", stream_id);
                        break;
                    }
                    Err(e) => {
                        error!("Error reading from tunnel (stream {}): {}", stream_id, e);
                        break;
                    }
                    _ => {
                        warn!(
                            "Unexpected message type on HTTP transparent stream {}",
                            stream_id
                        );
                    }
                }
            }
        });

        // Wait for both tasks to complete
        let _ = tokio::join!(local_to_tunnel, tunnel_to_local);
        debug!("Transparent HTTP stream {} ended", stream_id);
    }

    async fn handle_tcp_stream(
        mut stream: tunnel_transport_quic::stream::QuicStream,
        config: &TunnelConfig,
        _metrics: &MetricsStore,
        stream_id: u32,
    ) {
        // Get local TCP port from first TCP protocol
        let local_port = config.protocols.first().and_then(|p| match p {
            ProtocolConfig::Tcp { local_port, .. } => Some(*local_port),
            _ => None,
        });

        let local_port = match local_port {
            Some(port) => port,
            None => {
                error!("No TCP protocol configured");
                return;
            }
        };

        // Connect to local service
        let local_addr = format!("{}:{}", config.local_host, local_port);
        let local_socket = match TcpStream::connect(&local_addr).await {
            Ok(sock) => sock,
            Err(e) => {
                error!(
                    "Failed to connect to local TCP service at {}: {}",
                    local_addr, e
                );
                let _ = stream
                    .send_message(&TunnelMessage::TcpClose { stream_id })
                    .await;
                return;
            }
        };

        debug!("Connected to local TCP service at {}", local_addr);

        // Split BOTH streams for true bidirectional communication WITHOUT MUTEXES!
        let (mut local_read, mut local_write) = local_socket.into_split();
        let (mut quic_send, mut quic_recv) = stream.split();

        // Task to read from local TCP and send to QUIC stream
        // Now owns quic_send exclusively - no mutex needed!

        let local_to_quic = tokio::spawn(async move {
            let mut buffer = vec![0u8; 8192];
            loop {
                match local_read.read(&mut buffer).await {
                    Ok(0) => {
                        // Local socket closed
                        debug!("Local TCP socket closed (stream {})", stream_id);
                        let _ = quic_send
                            .send_message(&TunnelMessage::TcpClose { stream_id })
                            .await;
                        let _ = quic_send.finish().await;
                        break;
                    }
                    Ok(n) => {
                        debug!("Read {} bytes from local TCP (stream {})", n, stream_id);
                        let data_msg = TunnelMessage::TcpData {
                            stream_id,
                            data: buffer[..n].to_vec(),
                        };
                        if let Err(e) = quic_send.send_message(&data_msg).await {
                            error!("Failed to send TcpData on QUIC stream: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error reading from local TCP: {}", e);
                        break;
                    }
                }
            }
        });

        // Task to read from QUIC stream and send to local TCP
        // Now owns quic_recv exclusively - no mutex needed!
        let quic_to_local = tokio::spawn(async move {
            loop {
                // NO MUTEX - direct access to quic_recv!
                let msg = quic_recv.recv_message().await;

                match msg {
                    Ok(Some(TunnelMessage::TcpData { stream_id: _, data })) => {
                        if data.is_empty() {
                            debug!(
                                "Received close signal from QUIC stream (stream {})",
                                stream_id
                            );
                            break;
                        }
                        debug!(
                            "Received {} bytes from QUIC stream (stream {})",
                            data.len(),
                            stream_id
                        );
                        if let Err(e) = local_write.write_all(&data).await {
                            error!("Failed to write to local TCP: {}", e);
                            break;
                        }
                        if let Err(e) = local_write.flush().await {
                            error!("Failed to flush local TCP: {}", e);
                            break;
                        }
                    }
                    Ok(Some(TunnelMessage::TcpClose { stream_id: _ })) => {
                        debug!("Received TcpClose from QUIC stream (stream {})", stream_id);
                        break;
                    }
                    Ok(None) => {
                        debug!("QUIC stream closed (stream {})", stream_id);
                        break;
                    }
                    Err(e) => {
                        error!("Error reading from QUIC stream: {}", e);
                        break;
                    }
                    Ok(Some(msg)) => {
                        warn!("Unexpected message on TCP stream: {:?}", msg);
                    }
                }
            }
        });

        // Wait for both tasks
        let _ = tokio::join!(local_to_quic, quic_to_local);
        debug!("TCP stream handler finished (stream {})", stream_id);
    }

    async fn handle_http_request_static(
        config: &TunnelConfig,
        metrics: &MetricsStore,
        stream_id: u32,
        method: String,
        uri: String,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
    ) -> TunnelMessage {
        let start_time = Instant::now();

        // Generate short stream ID for metrics
        let short_stream_id = generate_short_id(stream_id);

        // Record request in metrics
        let metric_id = metrics
            .record_request(
                short_stream_id,
                method.clone(),
                uri.clone(),
                headers.clone(),
                body.clone(),
            )
            .await;
        // Get local port from first protocol
        let local_port = config.protocols.first().and_then(|p| match p {
            ProtocolConfig::Http { local_port, .. } => Some(*local_port),
            ProtocolConfig::Https { local_port, .. } => Some(*local_port),
            _ => None,
        });

        let local_port = match local_port {
            Some(port) => port,
            None => {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                error!("No HTTP/HTTPS protocol configured");
                metrics
                    .record_error(
                        &metric_id,
                        "No HTTP protocol configured".to_string(),
                        duration_ms,
                    )
                    .await;
                return TunnelMessage::HttpResponse {
                    stream_id,
                    status: 500,
                    headers: vec![],
                    body: Some(b"No HTTP protocol configured".to_vec()),
                };
            }
        };

        // Connect to local service
        let local_addr = format!("{}:{}", config.local_host, local_port);
        let mut local_socket = match TcpStream::connect(&local_addr).await {
            Ok(sock) => sock,
            Err(e) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                error!(
                    "Failed to connect to local service at {}: {}",
                    local_addr, e
                );
                metrics
                    .record_error(
                        &metric_id,
                        format!("Failed to connect to local service: {}", e),
                        duration_ms,
                    )
                    .await;
                return TunnelMessage::HttpResponse {
                    stream_id,
                    status: 502,
                    headers: vec![],
                    body: Some(format!("Failed to connect to local service: {}", e).into_bytes()),
                };
            }
        };

        // Build HTTP request
        let mut request = format!("{} {} HTTP/1.1\r\n", method, uri);
        for (name, value) in headers {
            request.push_str(&format!("{}: {}\r\n", name, value));
        }
        request.push_str("\r\n");

        // Send request
        if let Err(e) = local_socket.write_all(request.as_bytes()).await {
            let duration_ms = start_time.elapsed().as_millis() as u64;
            error!("Failed to write request: {}", e);
            metrics
                .record_error(&metric_id, format!("Write error: {}", e), duration_ms)
                .await;
            return TunnelMessage::HttpResponse {
                stream_id,
                status: 502,
                headers: vec![],
                body: Some(format!("Write error: {}", e).into_bytes()),
            };
        }

        if let Some(ref body_data) = body {
            if let Err(e) = local_socket.write_all(body_data).await {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                error!("Failed to write body: {}", e);
                metrics
                    .record_error(&metric_id, format!("Write error: {}", e), duration_ms)
                    .await;
                return TunnelMessage::HttpResponse {
                    stream_id,
                    status: 502,
                    headers: vec![],
                    body: Some(format!("Write error: {}", e).into_bytes()),
                };
            }
        }

        // Read response - first read to get headers
        let mut response_buf = Vec::new();
        let mut temp_buf = vec![0u8; 8192];

        // Read until we have headers (looking for \r\n\r\n or \n\n)
        let mut headers_complete = false;
        let mut header_end_pos = 0;

        while !headers_complete {
            let n = match local_socket.read(&mut temp_buf).await {
                Ok(0) => break, // Connection closed
                Ok(n) => n,
                Err(e) => {
                    let duration_ms = start_time.elapsed().as_millis() as u64;
                    error!("Failed to read response: {}", e);
                    metrics
                        .record_error(&metric_id, format!("Read error: {}", e), duration_ms)
                        .await;
                    return TunnelMessage::HttpResponse {
                        stream_id,
                        status: 502,
                        headers: vec![],
                        body: Some(format!("Read error: {}", e).into_bytes()),
                    };
                }
            };

            response_buf.extend_from_slice(&temp_buf[..n]);

            // Check if we have complete headers
            if let Some(pos) = response_buf.windows(4).position(|w| w == b"\r\n\r\n") {
                headers_complete = true;
                header_end_pos = pos + 4;
            } else if let Some(pos) = response_buf.windows(2).position(|w| w == b"\n\n") {
                headers_complete = true;
                header_end_pos = pos + 2;
            }
        }

        // Parse HTTP response headers
        let response_str = String::from_utf8_lossy(&response_buf[..header_end_pos]);

        // Extract status code from first line (e.g., "HTTP/1.1 200 OK")
        let status = response_str
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(200);

        // Parse headers
        let mut resp_headers = Vec::new();
        let mut content_length: Option<usize> = None;
        let mut is_chunked = false;

        for (i, line) in response_str.lines().enumerate() {
            if i == 0 {
                // Skip status line
                continue;
            }

            if line.is_empty() {
                break;
            }

            if let Some(colon_pos) = line.find(':') {
                let name = line[..colon_pos].trim().to_string();
                let value = line[colon_pos + 1..].trim().to_string();

                // Check for Content-Length
                if name.to_lowercase() == "content-length" {
                    content_length = value.parse::<usize>().ok();
                }

                // Check for chunked transfer encoding
                if name.to_lowercase() == "transfer-encoding"
                    && value.to_lowercase().contains("chunked")
                {
                    is_chunked = true;
                }

                resp_headers.push((name, value));
            }
        }

        // Read body based on Content-Length or chunked encoding
        let body = if let Some(expected_len) = content_length {
            // Content-Length present - read exact number of bytes
            let mut body_data = response_buf[header_end_pos..].to_vec();

            // Keep reading until we have all the body data
            while body_data.len() < expected_len {
                let n = match local_socket.read(&mut temp_buf).await {
                    Ok(0) => break, // Connection closed
                    Ok(n) => n,
                    Err(e) => {
                        warn!("Error reading body: {}", e);
                        break;
                    }
                };
                body_data.extend_from_slice(&temp_buf[..n]);
            }

            // Truncate to exact content length
            body_data.truncate(expected_len);

            if body_data.is_empty() {
                None
            } else {
                Some(body_data)
            }
        } else if is_chunked {
            // Chunked transfer encoding - read until connection closes or we see end marker
            // For simplicity, we'll read the entire chunked response and pass it as-is
            // The exit node will forward it with the same encoding
            let mut body_data = response_buf[header_end_pos..].to_vec();

            // Keep reading until connection closes or end marker
            // Use a short timeout per read to avoid waiting unnecessarily after last chunk
            loop {
                let read_result = tokio::time::timeout(
                    std::time::Duration::from_millis(100), // Short timeout - 100ms
                    local_socket.read(&mut temp_buf),
                )
                .await;

                match read_result {
                    Ok(Ok(0)) => {
                        // Connection closed
                        debug!(
                            "Chunked response: connection closed after {} bytes",
                            body_data.len()
                        );
                        break;
                    }
                    Ok(Ok(n)) => {
                        body_data.extend_from_slice(&temp_buf[..n]);

                        // Check for chunked encoding end marker
                        // Look for "\r\n0\r\n\r\n" or just "0\r\n\r\n" at the end
                        if body_data.len() >= 5
                            && (body_data.ends_with(b"0\r\n\r\n")
                                || body_data.ends_with(b"\r\n0\r\n\r\n"))
                        {
                            debug!(
                                "Chunked response: found end marker after {} bytes",
                                body_data.len()
                            );
                            break;
                        }
                    }
                    Ok(Err(e)) => {
                        warn!("Error reading chunked body: {}", e);
                        break;
                    }
                    Err(_) => {
                        // Timeout - assume response is complete (after 100ms of no data)
                        debug!(
                            "Chunked response: read timeout, assuming complete ({} bytes)",
                            body_data.len()
                        );
                        break;
                    }
                }
            }

            if body_data.is_empty() {
                None
            } else {
                Some(body_data)
            }
        } else {
            // No Content-Length and not chunked - read until connection closes
            let mut body_data = response_buf[header_end_pos..].to_vec();

            // Use short timeout to avoid unnecessary waiting
            loop {
                let read_result = tokio::time::timeout(
                    std::time::Duration::from_millis(100),
                    local_socket.read(&mut temp_buf),
                )
                .await;

                match read_result {
                    Ok(Ok(0)) => break, // Connection closed
                    Ok(Ok(n)) => {
                        body_data.extend_from_slice(&temp_buf[..n]);
                    }
                    Ok(Err(e)) => {
                        warn!("Error reading body: {}", e);
                        break;
                    }
                    Err(_) => {
                        // Timeout - assume response is complete
                        debug!(
                            "Response read timeout, assuming complete ({} bytes)",
                            body_data.len()
                        );
                        break;
                    }
                }
            }

            if body_data.is_empty() {
                None
            } else {
                Some(body_data)
            }
        };

        debug!(
            "Local service responded with status {} and {} headers, body size: {}",
            status,
            resp_headers.len(),
            body.as_ref().map(|b| b.len()).unwrap_or(0)
        );

        // Record successful response in metrics
        let duration_ms = start_time.elapsed().as_millis() as u64;
        metrics
            .record_response(
                &metric_id,
                status,
                resp_headers.clone(),
                body.clone(),
                duration_ms,
            )
            .await;

        TunnelMessage::HttpResponse {
            stream_id,
            status,
            headers: resp_headers,
            body,
        }
    }

    #[allow(dead_code)] // Legacy function - now using handle_tcp_stream
    async fn handle_tcp_connection_static(
        config: &TunnelConfig,
        stream_id: u32,
        remote_addr: String,
        _remote_port: u16,
        response_tx: tokio::sync::mpsc::UnboundedSender<TunnelMessage>,
        tcp_streams: TcpStreamManager,
        metrics: MetricsStore,
    ) {
        // Get local port from first TCP protocol
        let local_port = config.protocols.first().and_then(|p| match p {
            ProtocolConfig::Tcp { local_port, .. } => Some(*local_port),
            _ => None,
        });

        let local_port = match local_port {
            Some(port) => port,
            None => {
                error!("No TCP protocol configured");
                let _ = response_tx.send(TunnelMessage::TcpClose { stream_id });
                return;
            }
        };

        // Connect to local service
        let local_addr = format!("{}:{}", config.local_host, local_port);
        let local_socket = match TcpStream::connect(&local_addr).await {
            Ok(sock) => sock,
            Err(e) => {
                error!(
                    "Failed to connect to local service at {}: {}",
                    local_addr, e
                );
                let _ = response_tx.send(TunnelMessage::TcpClose { stream_id });
                return;
            }
        };

        debug!(
            "Connected to local TCP service at {} (stream {})",
            local_addr, stream_id
        );

        // Record TCP connection in metrics
        let stream_id_str = generate_short_id(stream_id);
        let connection_id = metrics
            .record_tcp_connection(stream_id_str, remote_addr, local_addr.clone())
            .await;

        // Split socket for bidirectional communication (into owned halves)
        let (mut local_read, mut local_write) = local_socket.into_split();

        // Create channel for receiving data from exit node
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(100);

        // Register this stream in the manager to receive TcpData messages
        {
            let mut tcp_streams_lock = tcp_streams.lock().await;
            tcp_streams_lock.insert(stream_id, tx);
        }
        debug!("Registered stream {} in TCP stream manager", stream_id);

        // Shared byte counters for metrics
        let bytes_sent = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let bytes_received = Arc::new(std::sync::atomic::AtomicU64::new(0));

        // Spawn task to read from local service and send to tunnel
        let response_tx_clone = response_tx.clone();
        let bytes_sent_clone = bytes_sent.clone();
        let read_task = tokio::spawn(async move {
            let mut buffer = vec![0u8; 8192];
            loop {
                match local_read.read(&mut buffer).await {
                    Ok(0) => {
                        // Local service closed connection
                        debug!("Local service closed connection (stream {})", stream_id);
                        let _ = response_tx_clone.send(TunnelMessage::TcpClose { stream_id });
                        break;
                    }
                    Ok(n) => {
                        debug!("Read {} bytes from local service (stream {})", n, stream_id);

                        // Update byte counter
                        bytes_sent_clone.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);

                        // Send data to tunnel
                        let data_msg = TunnelMessage::TcpData {
                            stream_id,
                            data: buffer[..n].to_vec(),
                        };

                        if let Err(e) = response_tx_clone.send(data_msg) {
                            error!("Failed to send TcpData: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error reading from local service: {}", e);
                        let _ = response_tx_clone.send(TunnelMessage::TcpClose { stream_id });
                        break;
                    }
                }
            }
        });

        // Spawn task to receive from tunnel and write to local service
        let bytes_received_clone = bytes_received.clone();
        let write_task = tokio::spawn(async move {
            while let Some(data) = rx.recv().await {
                if data.is_empty() {
                    // Empty data means close signal
                    debug!("Received close signal (stream {})", stream_id);
                    break;
                }

                debug!(
                    "Writing {} bytes to local service (stream {})",
                    data.len(),
                    stream_id
                );

                // Update byte counter
                bytes_received_clone
                    .fetch_add(data.len() as u64, std::sync::atomic::Ordering::Relaxed);

                if let Err(e) = local_write.write_all(&data).await {
                    error!("Failed to write to local service: {}", e);
                    break;
                }

                if let Err(e) = local_write.flush().await {
                    error!("Failed to flush local write: {}", e);
                    break;
                }
            }
        });

        // Wait for both tasks to complete
        let _ = tokio::join!(read_task, write_task);

        // Close connection in metrics with final byte counts
        let final_bytes_sent = bytes_sent.load(std::sync::atomic::Ordering::Relaxed);
        let final_bytes_received = bytes_received.load(std::sync::atomic::Ordering::Relaxed);

        // Update metrics one last time before closing
        metrics
            .update_tcp_connection(&connection_id, final_bytes_received, final_bytes_sent)
            .await;
        metrics.close_tcp_connection(&connection_id, None).await;

        // Unregister stream from manager
        {
            let mut tcp_streams_lock = tcp_streams.lock().await;
            tcp_streams_lock.remove(&stream_id);
        }

        debug!(
            "TCP connection handler exiting (stream {}) - {} bytes sent, {} bytes received",
            stream_id, final_bytes_sent, final_bytes_received
        );
    }
}
