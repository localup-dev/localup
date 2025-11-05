//! Reverse tunnel client implementation
//!
//! Allows clients to connect to reverse tunnels exposed by agents through the relay.
//! The client binds a local TCP server and proxies connections through the relay to remote services.

use crate::TunnelError;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};
use tunnel_proto::TunnelMessage;
use tunnel_transport::{
    TransportConnection, TransportConnector as TransportConnectorTrait, TransportStream,
};
use tunnel_transport_quic::{QuicConfig, QuicConnector};

/// Reverse tunnel client errors
#[derive(Debug, Error)]
pub enum ReverseTunnelError {
    #[error("Connection to relay failed: {0}")]
    ConnectionFailed(String),

    #[error("Reverse tunnel rejected: {0}")]
    Rejected(String),

    #[error("Agent not available: {0}")]
    AgentNotAvailable(String),

    #[error("Operation timed out: {0}")]
    Timeout(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Transport error: {0}")]
    TransportError(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),
}

impl From<ReverseTunnelError> for TunnelError {
    fn from(err: ReverseTunnelError) -> Self {
        match err {
            ReverseTunnelError::ConnectionFailed(msg) => TunnelError::ConnectionError(msg),
            ReverseTunnelError::Rejected(msg) => TunnelError::ConnectionError(msg),
            ReverseTunnelError::AgentNotAvailable(msg) => TunnelError::ConnectionError(msg),
            ReverseTunnelError::Timeout(msg) => TunnelError::NetworkError(msg),
            ReverseTunnelError::IoError(e) => TunnelError::NetworkError(e.to_string()),
            ReverseTunnelError::TransportError(msg) => TunnelError::NetworkError(msg),
            ReverseTunnelError::ProtocolError(msg) => TunnelError::ProtocolError(msg),
        }
    }
}

/// Configuration for reverse tunnel client
#[derive(Debug, Clone)]
pub struct ReverseTunnelConfig {
    /// Relay server address (e.g., "relay.example.com:4443" or "127.0.0.1:4443")
    pub relay_addr: String,

    /// Optional JWT authentication token
    pub auth_token: Option<String>,

    /// Target address to connect to through the agent (e.g., "192.168.1.100:8080")
    pub remote_address: String,

    /// Specific agent ID to route through
    pub agent_id: String,

    /// Local bind address (defaults to "127.0.0.1:0" for automatic port allocation)
    pub local_bind_address: Option<String>,

    /// Skip TLS verification (development only, insecure)
    pub insecure: bool,
}

impl ReverseTunnelConfig {
    /// Create a new reverse tunnel configuration
    pub fn new(relay_addr: String, remote_address: String, agent_id: String) -> Self {
        Self {
            relay_addr,
            auth_token: None,
            remote_address,
            agent_id,
            local_bind_address: None,
            insecure: false,
        }
    }

    /// Set authentication token
    pub fn with_auth_token(mut self, token: String) -> Self {
        self.auth_token = Some(token);
        self
    }

    /// Set local bind address
    pub fn with_local_bind_address(mut self, addr: String) -> Self {
        self.local_bind_address = Some(addr);
        self
    }

    /// Enable insecure mode (skip TLS verification)
    pub fn with_insecure(mut self, insecure: bool) -> Self {
        self.insecure = insecure;
        self
    }
}

/// Reverse tunnel client
pub struct ReverseTunnelClient {
    config: ReverseTunnelConfig,
    #[allow(dead_code)] // Keep connection alive to maintain QUIC connection
    connection: Arc<tunnel_transport_quic::QuicConnection>,
    tunnel_id: String,
    local_addr: SocketAddr,
    shutdown_tx: Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::Sender<()>>>>,
}

impl ReverseTunnelClient {
    /// Connect to relay and establish reverse tunnel
    pub async fn connect(config: ReverseTunnelConfig) -> Result<Self, ReverseTunnelError> {
        info!(
            "Connecting to relay at {} for reverse tunnel to {} via agent {}",
            config.relay_addr, config.remote_address, config.agent_id
        );

        // Parse relay address
        let (hostname, relay_addr) = Self::parse_relay_address(&config.relay_addr).await?;

        // Create QUIC connector
        // TODO: Implement proper certificate validation when insecure is false
        let quic_config = Arc::new(QuicConfig::client_insecure());

        let quic_connector = QuicConnector::new(quic_config).map_err(|e| {
            ReverseTunnelError::ConnectionFailed(format!("Failed to create QUIC connector: {}", e))
        })?;

        // Connect to relay via QUIC
        let connection = quic_connector
            .connect(relay_addr, &hostname)
            .await
            .map_err(|e| {
                ReverseTunnelError::ConnectionFailed(format!(
                    "Failed to connect to relay {}: {}",
                    config.relay_addr, e
                ))
            })?;

        let connection = Arc::new(connection);
        info!("✅ Connected to relay via QUIC");

        // Generate tunnel ID
        let tunnel_id = uuid::Uuid::new_v4().to_string();

        // Open control stream
        let mut control_stream = connection.open_stream().await.map_err(|e| {
            ReverseTunnelError::ConnectionFailed(format!("Failed to open control stream: {}", e))
        })?;

        // Send ReverseTunnelRequest
        let request_msg = TunnelMessage::ReverseTunnelRequest {
            tunnel_id: tunnel_id.clone(),
            remote_address: config.remote_address.clone(),
            agent_id: config.agent_id.clone(),
        };

        control_stream
            .send_message(&request_msg)
            .await
            .map_err(|e| {
                ReverseTunnelError::TransportError(format!(
                    "Failed to send ReverseTunnelRequest: {}",
                    e
                ))
            })?;

        debug!("Sent ReverseTunnelRequest");

        // Wait for response (ReverseTunnelAccept or ReverseTunnelReject)
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            control_stream.recv_message(),
        )
        .await
        .map_err(|_| ReverseTunnelError::Timeout("Waiting for reverse tunnel response".into()))?
        .map_err(|e| {
            ReverseTunnelError::TransportError(format!("Failed to receive response: {}", e))
        })?
        .ok_or_else(|| ReverseTunnelError::ConnectionFailed("Connection closed by relay".into()))?;

        match response {
            TunnelMessage::ReverseTunnelAccept {
                tunnel_id: tid,
                local_address,
            } => {
                info!("✅ Reverse tunnel accepted: {}", tid);
                info!("📍 Local address suggestion: {}", local_address);

                // Bind local TCP server
                let bind_addr = config
                    .local_bind_address
                    .clone()
                    .unwrap_or_else(|| "127.0.0.1:0".to_string());

                let listener = TcpListener::bind(&bind_addr).await?;
                let local_addr = listener.local_addr()?;

                info!("🌐 Listening on: {}", local_addr);

                // Spawn local server task - pass control_stream to keep it alive
                let tunnel_id_clone = tunnel_id.clone();
                let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

                tokio::spawn(async move {
                    Self::run_local_server(listener, control_stream, tunnel_id_clone, shutdown_rx)
                        .await;
                });

                Ok(Self {
                    config,
                    connection,
                    tunnel_id: tid,
                    local_addr,
                    shutdown_tx: Arc::new(tokio::sync::Mutex::new(Some(shutdown_tx))),
                })
            }
            TunnelMessage::ReverseTunnelReject { reason, .. } => {
                error!("❌ Reverse tunnel rejected: {}", reason);

                if reason.contains("not available") || reason.contains("not connected") {
                    Err(ReverseTunnelError::AgentNotAvailable(reason))
                } else {
                    Err(ReverseTunnelError::Rejected(reason))
                }
            }
            other => {
                error!("Unexpected response: {:?}", other);
                Err(ReverseTunnelError::ProtocolError(format!(
                    "Unexpected message: {:?}",
                    other
                )))
            }
        }
    }

    /// Get the local bind address
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Get the tunnel ID
    pub fn tunnel_id(&self) -> &str {
        &self.tunnel_id
    }

    /// Get the remote address
    pub fn remote_address(&self) -> &str {
        &self.config.remote_address
    }

    /// Get the agent ID
    pub fn agent_id(&self) -> &str {
        &self.config.agent_id
    }

    /// Wait for the reverse tunnel to close
    pub async fn wait(self) -> Result<(), ReverseTunnelError> {
        // The server task runs until shutdown is triggered
        // We just keep the connection alive
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            // Check if shutdown was triggered
            let shutdown_tx_guard = self.shutdown_tx.lock().await;
            if shutdown_tx_guard.is_none() {
                break;
            }
        }

        info!("Reverse tunnel closed");
        Ok(())
    }

    /// Close the reverse tunnel gracefully
    pub async fn close(self) -> Result<(), ReverseTunnelError> {
        info!("Closing reverse tunnel");

        let mut shutdown_tx_guard = self.shutdown_tx.lock().await;
        if let Some(tx) = shutdown_tx_guard.take() {
            let _ = tx.send(()).await;
        }

        // Give time for cleanup
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Ok(())
    }

    /// Parse relay address from various formats
    async fn parse_relay_address(
        addr_str: &str,
    ) -> Result<(String, SocketAddr), ReverseTunnelError> {
        // Remove protocol prefix if present
        let addr_without_protocol = addr_str
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("quic://");

        // Try to parse as SocketAddr first (IP:port format)
        if let Ok(socket_addr) = addr_without_protocol.parse::<SocketAddr>() {
            let hostname = socket_addr.ip().to_string();
            return Ok((hostname, socket_addr));
        }

        // Not a direct IP:port, must be hostname:port or just hostname
        let (hostname, port) = if let Some(colon_pos) = addr_without_protocol.rfind(':') {
            let host = &addr_without_protocol[..colon_pos];
            let port_str = &addr_without_protocol[colon_pos + 1..];

            let port: u16 = port_str.parse().map_err(|_| {
                ReverseTunnelError::ConnectionFailed(format!(
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
        let socket_addrs: Vec<SocketAddr> = tokio::net::lookup_host(&addr_with_port)
            .await
            .map_err(|e| {
                ReverseTunnelError::ConnectionFailed(format!(
                    "Failed to resolve hostname '{}': {}",
                    hostname, e
                ))
            })?
            .collect();

        // Prefer IPv4 addresses
        let socket_addr = socket_addrs
            .iter()
            .find(|addr| addr.is_ipv4())
            .or_else(|| socket_addrs.first())
            .copied()
            .ok_or_else(|| {
                ReverseTunnelError::ConnectionFailed(format!(
                    "No addresses found for hostname '{}'",
                    hostname
                ))
            })?;

        Ok((hostname, socket_addr))
    }

    /// Run local TCP server and handle incoming connections
    async fn run_local_server(
        listener: TcpListener,
        control_stream: tunnel_transport_quic::QuicStream,
        tunnel_id: String,
        mut shutdown_rx: tokio::sync::mpsc::Receiver<()>,
    ) {
        info!("Starting local TCP server for reverse tunnel");

        // Split control stream for bidirectional communication
        let (control_send, mut control_recv) = control_stream.split();
        let control_send = Arc::new(tokio::sync::Mutex::new(control_send));

        // Map of stream_id -> channel for routing incoming messages to TCP connections
        let stream_map: Arc<
            tokio::sync::RwLock<
                std::collections::HashMap<u32, tokio::sync::mpsc::Sender<TunnelMessage>>,
            >,
        > = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));

        let stream_map_clone = stream_map.clone();

        // Spawn task to read from control stream and route to TCP connections
        let tunnel_id_clone = tunnel_id.clone();
        tokio::spawn(async move {
            loop {
                match control_recv.recv_message().await {
                    Ok(Some(TunnelMessage::ReverseData {
                        stream_id, data, ..
                    })) => {
                        debug!(
                            "Received {} bytes from relay for stream {}",
                            data.len(),
                            stream_id
                        );
                        let map = stream_map_clone.read().await;
                        if let Some(tx) = map.get(&stream_id) {
                            let _ = tx
                                .send(TunnelMessage::ReverseData {
                                    tunnel_id: tunnel_id_clone.clone(),
                                    stream_id,
                                    data,
                                })
                                .await;
                        } else {
                            warn!("No handler for stream_id {}", stream_id);
                        }
                    }
                    Ok(Some(TunnelMessage::ReverseClose { stream_id, .. })) => {
                        debug!("Received ReverseClose from relay for stream {}", stream_id);
                        let map = stream_map_clone.read().await;
                        if let Some(tx) = map.get(&stream_id) {
                            let _ = tx
                                .send(TunnelMessage::ReverseClose {
                                    tunnel_id: tunnel_id_clone.clone(),
                                    stream_id,
                                })
                                .await;
                        }
                    }
                    Ok(None) => {
                        error!("Control stream closed by relay");
                        break;
                    }
                    Err(e) => {
                        error!("Error reading from control stream: {}", e);
                        break;
                    }
                    Ok(Some(msg)) => {
                        warn!("Unexpected message on control stream: {:?}", msg);
                    }
                }
            }
        });

        let mut stream_id_counter: u32 = 1;

        loop {
            tokio::select! {
                // Check for shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, stopping local server");
                    break;
                }

                // Accept incoming TCP connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((tcp_stream, peer_addr)) => {
                            debug!("Accepted TCP connection from {}", peer_addr);

                            let stream_id = stream_id_counter;
                            stream_id_counter += 1;

                            let control_send_clone = control_send.clone();
                            let stream_map_clone = stream_map.clone();
                            let tunnel_id_clone = tunnel_id.clone();

                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_reverse_connection(
                                    tcp_stream,
                                    control_send_clone,
                                    stream_map_clone,
                                    tunnel_id_clone,
                                    stream_id,
                                )
                                .await
                                {
                                    error!("Error handling reverse connection: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept TCP connection: {}", e);
                        }
                    }
                }
            }
        }

        info!("Local TCP server stopped");
    }

    /// Handle a single reverse connection
    async fn handle_reverse_connection(
        tcp_stream: TcpStream,
        control_send: Arc<tokio::sync::Mutex<tunnel_transport_quic::QuicSendHalf>>,
        stream_map: Arc<
            tokio::sync::RwLock<
                std::collections::HashMap<u32, tokio::sync::mpsc::Sender<TunnelMessage>>,
            >,
        >,
        tunnel_id: String,
        stream_id: u32,
    ) -> Result<(), ReverseTunnelError> {
        debug!(
            "Handling reverse connection for stream {} (tunnel {})",
            stream_id, tunnel_id
        );

        // Create channel for receiving messages from relay
        let (msg_tx, mut msg_rx) = tokio::sync::mpsc::channel::<TunnelMessage>(100);

        // Register this stream_id in the map
        {
            let mut map = stream_map.write().await;
            map.insert(stream_id, msg_tx);
        }

        // Ensure cleanup on exit
        let stream_map_cleanup = stream_map.clone();
        let _cleanup_guard = scopeguard::guard(stream_id, move |stream_id| {
            let map = stream_map_cleanup.clone();
            tokio::spawn(async move {
                let mut map = map.write().await;
                map.remove(&stream_id);
            });
        });

        // Split TCP stream for bidirectional communication
        let (mut tcp_read, mut tcp_write) = tcp_stream.into_split();

        // Task: TCP → Control Stream (read from local TCP, send to relay via ReverseData)
        let control_send_clone = control_send.clone();
        let tunnel_id_clone = tunnel_id.clone();
        let tcp_to_relay = tokio::spawn(async move {
            let mut buffer = vec![0u8; 8192];
            loop {
                match tcp_read.read(&mut buffer).await {
                    Ok(0) => {
                        debug!("Local TCP connection closed (stream {})", stream_id);
                        let close_msg = TunnelMessage::ReverseClose {
                            tunnel_id: tunnel_id_clone.clone(),
                            stream_id,
                        };
                        let mut send = control_send_clone.lock().await;
                        let _ = send.send_message(&close_msg).await;
                        break;
                    }
                    Ok(n) => {
                        debug!("Read {} bytes from local TCP (stream {})", n, stream_id);
                        let data_msg = TunnelMessage::ReverseData {
                            tunnel_id: tunnel_id_clone.clone(),
                            stream_id,
                            data: buffer[..n].to_vec(),
                        };
                        let mut send = control_send_clone.lock().await;
                        if let Err(e) = send.send_message(&data_msg).await {
                            error!("Failed to send ReverseData to relay: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error reading from local TCP (stream {}): {}", stream_id, e);
                        break;
                    }
                }
            }
        });

        // Task: Control Stream → TCP (receive from relay via channel, write to local TCP)
        let relay_to_tcp = tokio::spawn(async move {
            loop {
                match msg_rx.recv().await {
                    Some(TunnelMessage::ReverseData { data, .. }) => {
                        debug!(
                            "Received {} bytes from relay (stream {})",
                            data.len(),
                            stream_id
                        );
                        if let Err(e) = tcp_write.write_all(&data).await {
                            error!("Failed to write to local TCP (stream {}): {}", stream_id, e);
                            break;
                        }
                    }
                    Some(TunnelMessage::ReverseClose { .. }) => {
                        debug!("Received ReverseClose from relay (stream {})", stream_id);
                        break;
                    }
                    None => {
                        debug!("Message channel closed (stream {})", stream_id);
                        break;
                    }
                    Some(msg) => {
                        warn!("Unexpected message for stream {}: {:?}", stream_id, msg);
                    }
                }
            }
        });

        // Wait for both tasks to complete
        let _ = tokio::join!(tcp_to_relay, relay_to_tcp);
        debug!("Reverse connection handler finished (stream {})", stream_id);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverse_tunnel_config() {
        let config = ReverseTunnelConfig::new(
            "relay.example.com:4443".to_string(),
            "192.168.1.100:8080".to_string(),
            "agent-123".to_string(),
        )
        .with_auth_token("test-token".to_string())
        .with_local_bind_address("127.0.0.1:8888".to_string())
        .with_insecure(true);

        assert_eq!(config.relay_addr, "relay.example.com:4443");
        assert_eq!(config.remote_address, "192.168.1.100:8080");
        assert_eq!(config.agent_id, "agent-123");
        assert_eq!(config.auth_token, Some("test-token".to_string()));
        assert_eq!(
            config.local_bind_address,
            Some("127.0.0.1:8888".to_string())
        );
        assert!(config.insecure);
    }

    #[tokio::test]
    async fn test_parse_relay_address_ip_port() {
        let (hostname, addr) = ReverseTunnelClient::parse_relay_address("127.0.0.1:4443")
            .await
            .unwrap();
        assert_eq!(hostname, "127.0.0.1");
        assert_eq!(addr.port(), 4443);
    }

    #[tokio::test]
    async fn test_parse_relay_address_with_protocol() {
        let (hostname, addr) = ReverseTunnelClient::parse_relay_address("https://127.0.0.1:4443")
            .await
            .unwrap();
        assert_eq!(hostname, "127.0.0.1");
        assert_eq!(addr.port(), 4443);
    }

    #[tokio::test]
    #[ignore] // Requires DNS resolution
    async fn test_parse_relay_address_hostname() {
        let result = ReverseTunnelClient::parse_relay_address("localhost:4443").await;
        assert!(result.is_ok());
        let (hostname, addr) = result.unwrap();
        assert_eq!(hostname, "localhost");
        assert_eq!(addr.port(), 4443);
    }

    #[tokio::test]
    #[ignore] // Requires a running relay server
    async fn test_reverse_tunnel_connection() {
        let config = ReverseTunnelConfig::new(
            "127.0.0.1:4443".to_string(),
            "192.168.1.100:8080".to_string(),
            "test-agent".to_string(),
        )
        .with_auth_token("test-token".to_string())
        .with_insecure(true);

        let result = ReverseTunnelClient::connect(config).await;
        // This will fail without a real server, but verifies the code compiles
        assert!(result.is_err());
    }
}
