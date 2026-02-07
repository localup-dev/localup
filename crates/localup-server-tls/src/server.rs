//! TLS server with SNI-based passthrough routing
//!
//! This server accepts incoming TLS connections, extracts the SNI (Server Name Indication)
//! from the ClientHello, and routes the connection to the appropriate backend service.
//!
//! No TLS termination is performed - the TLS stream is forwarded as-is to preserve
//! end-to-end encryption between the client and backend service.
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};

use localup_control::TunnelConnectionManager;
use localup_proto::TunnelMessage;
use localup_router::{RouteRegistry, SniRouter};
use localup_transport::{TransportConnection, TransportStream};
use localup_transport_quic::QuicStream;
use sea_orm::DatabaseConnection;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Error)]
pub enum TlsServerError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Certificate error: {0}")]
    CertificateError(String),

    #[error("SNI extraction failed")]
    SniExtractionFailed,

    #[error("No route found for SNI: {0}")]
    NoRoute(String),

    #[error("Access denied for IP: {0}")]
    AccessDenied(String),

    #[error("Transport error: {0}")]
    TransportError(String),

    #[error("TLS error: {0}")]
    TlsError(String),

    #[error("Failed to bind to {address}: {reason}\n\nTroubleshooting:\n  • Check if another process is using this port: lsof -i :{port}\n  • Try using a different address or port")]
    BindError {
        address: String,
        port: u16,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct TlsServerConfig {
    pub bind_addr: SocketAddr,
}

impl Default for TlsServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:443".parse().unwrap(),
        }
    }
}

/// Tracks metrics for an individual TLS connection
struct TlsConnectionMetrics {
    bytes_received: Arc<AtomicU64>,
    bytes_sent: Arc<AtomicU64>,
    connected_at: chrono::DateTime<chrono::Utc>,
}

pub struct TlsServer {
    config: TlsServerConfig,
    sni_router: Arc<SniRouter>,
    tunnel_manager: Option<Arc<TunnelConnectionManager>>,
    db: Option<DatabaseConnection>,
}

impl TlsServer {
    /// Create a new TLS server with SNI routing
    pub fn new(config: TlsServerConfig, route_registry: Arc<RouteRegistry>) -> Self {
        let sni_router = Arc::new(SniRouter::new(route_registry));
        Self {
            config,
            sni_router,
            tunnel_manager: None,
            db: None,
        }
    }

    /// Set the tunnel connection manager for forwarding to tunnels
    pub fn with_localup_manager(mut self, manager: Arc<TunnelConnectionManager>) -> Self {
        self.tunnel_manager = Some(manager);
        self
    }

    /// Set database connection for metrics tracking
    pub fn with_database(mut self, db: DatabaseConnection) -> Self {
        self.db = Some(db);
        self
    }

    /// Get reference to SNI router for registering routes
    pub fn sni_router(&self) -> Arc<SniRouter> {
        self.sni_router.clone()
    }

    /// Start the TLS server
    /// This server accepts incoming TLS connections and routes them based on SNI (passthrough mode)
    /// No certificate termination is performed - the TLS connection is forwarded as-is to the backend
    pub async fn start(&self) -> Result<(), TlsServerError> {
        info!("TLS server starting on {}", self.config.bind_addr);

        // Create TCP listener (no TLS termination - we do SNI passthrough)
        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| {
                let port = self.config.bind_addr.port();
                let address = self.config.bind_addr.ip().to_string();
                let reason = e.to_string();
                TlsServerError::BindError {
                    address,
                    port,
                    reason,
                }
            })?;
        info!(
            "✅ TLS server listening on {} (SNI passthrough routing, no certificate termination)",
            self.config.bind_addr
        );

        // Accept incoming connections
        loop {
            match listener.accept().await {
                Ok((socket, peer_addr)) => {
                    debug!("New TLS connection from {}", peer_addr);

                    let sni_router = self.sni_router.clone();
                    let tunnel_manager = self.tunnel_manager.clone();
                    let db = self.db.clone();
                    let tls_port = self.config.bind_addr.port();

                    tokio::spawn(async move {
                        // Forward the raw TLS stream based on SNI extraction
                        if let Err(e) = Self::forward_tls_stream(
                            socket,
                            &sni_router,
                            tunnel_manager,
                            peer_addr,
                            db,
                            tls_port,
                        )
                        .await
                        {
                            debug!("Error forwarding TLS stream from {}: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("TLS listener accept error: {}", e);
                }
            }
        }
    }

    /// Forward TLS stream to backend based on SNI extraction
    /// This implements SNI passthrough: no TLS termination, just routing based on SNI hostname
    async fn forward_tls_stream(
        mut client_socket: tokio::net::TcpStream,
        sni_router: &Arc<SniRouter>,
        tunnel_manager: Option<Arc<TunnelConnectionManager>>,
        peer_addr: SocketAddr,
        db: Option<DatabaseConnection>,
        tls_port: u16,
    ) -> Result<(), TlsServerError> {
        // Read the ClientHello from the incoming connection
        let mut client_hello_buf = [0u8; 16384];
        let n = client_socket
            .read(&mut client_hello_buf)
            .await
            .map_err(|e| TlsServerError::TransportError(e.to_string()))?;

        if n == 0 {
            debug!("Client closed connection before sending ClientHello");
            return Ok(());
        }

        debug!("Received {} bytes from TLS client", n);

        // Extract SNI from the ClientHello
        let sni_hostname = SniRouter::extract_sni(&client_hello_buf[..n]).map_err(|e| {
            debug!("SNI extraction failed from {}: {}", peer_addr, e);
            TlsServerError::SniExtractionFailed
        })?;

        info!(
            "📥 TLS connection from {} for SNI: {}",
            peer_addr, sni_hostname
        );

        // Look up the route for this SNI hostname
        let route = sni_router.lookup(&sni_hostname).map_err(|e| {
            debug!(
                "No route found for SNI {} from {}: {}",
                sni_hostname, peer_addr, e
            );
            TlsServerError::NoRoute(sni_hostname.clone())
        })?;

        // Check IP filtering
        if !route.is_ip_allowed(&peer_addr) {
            warn!(
                "🚫 Connection from {} denied by IP filter for SNI: {}",
                peer_addr, sni_hostname
            );
            return Err(TlsServerError::AccessDenied(peer_addr.to_string()));
        }

        info!(
            "🔀 Routing SNI {} to backend: {} (localup: {})",
            sni_hostname, route.target_addr, route.localup_id
        );

        // Create connection metrics tracker
        let connection_id = uuid::Uuid::new_v4().to_string();
        let metrics = TlsConnectionMetrics {
            bytes_received: Arc::new(AtomicU64::new(n as u64)), // Include ClientHello bytes
            bytes_sent: Arc::new(AtomicU64::new(0)),
            connected_at: chrono::Utc::now(),
        };

        // Save active connection to database
        if let Some(ref db_conn) = db {
            let active_connection =
                localup_relay_db::entities::captured_tcp_connection::ActiveModel {
                    id: sea_orm::Set(connection_id.clone()),
                    localup_id: sea_orm::Set(route.localup_id.clone()),
                    client_addr: sea_orm::Set(format!("{}|sni:{}", peer_addr, sni_hostname)),
                    target_port: sea_orm::Set(tls_port as i32),
                    bytes_received: sea_orm::Set(n as i64),
                    bytes_sent: sea_orm::Set(0),
                    connected_at: sea_orm::Set(metrics.connected_at.into()),
                    disconnected_at: sea_orm::NotSet,
                    duration_ms: sea_orm::NotSet,
                    disconnect_reason: sea_orm::NotSet,
                };

            use sea_orm::EntityTrait;
            if let Err(e) = localup_relay_db::entities::prelude::CapturedTcpConnection::insert(
                active_connection,
            )
            .exec(db_conn)
            .await
            {
                warn!(
                    "Failed to save active TLS connection {}: {}",
                    connection_id, e
                );
            } else {
                debug!("Saved active TLS connection {} to database", connection_id);
            }
        }

        // Check if this is a tunnel target (format: tunnel:localup_id)
        let result = if route.target_addr.starts_with("tunnel:") {
            // Extract localup_id from "tunnel:localup_id"
            let localup_id = route.target_addr.strip_prefix("tunnel:").unwrap();

            // Get tunnel manager
            let manager = tunnel_manager.ok_or_else(|| {
                TlsServerError::TransportError(
                    "Tunnel target requested but tunnel manager not configured".to_string(),
                )
            })?;

            // Get the tunnel connection
            let connection = manager.get(localup_id).await.ok_or_else(|| {
                TlsServerError::TransportError(format!("Tunnel not found: {}", localup_id))
            })?;

            // Open a new stream on the tunnel
            let backend_stream = connection.open_stream().await.map_err(|e| {
                TlsServerError::TransportError(format!(
                    "Failed to open stream to tunnel {}: {}",
                    localup_id, e
                ))
            })?;

            // Forward using TransportStream methods
            Self::forward_via_transport_stream(
                client_socket,
                backend_stream,
                &client_hello_buf[..n],
                peer_addr,
                metrics.bytes_received.clone(),
                metrics.bytes_sent.clone(),
            )
            .await
        } else {
            // Regular TCP backend connection
            let mut backend_socket = tokio::net::TcpStream::connect(&route.target_addr)
                .await
                .map_err(|e| {
                    TlsServerError::TransportError(format!(
                        "Failed to connect to backend {}: {}",
                        route.target_addr, e
                    ))
                })?;

            // Send the ClientHello to the backend
            backend_socket
                .write_all(&client_hello_buf[..n])
                .await
                .map_err(|e| TlsServerError::TransportError(e.to_string()))?;

            // Bidirectionally forward data between client and backend with metrics
            Self::forward_tcp_streams(
                client_socket,
                backend_socket,
                metrics.bytes_received.clone(),
                metrics.bytes_sent.clone(),
            )
            .await
        };

        // Update final metrics in database
        let disconnected_at = chrono::Utc::now();
        let duration_ms = (disconnected_at - metrics.connected_at).num_milliseconds() as i32;
        let final_bytes_received = metrics.bytes_received.load(Ordering::Relaxed);
        let final_bytes_sent = metrics.bytes_sent.load(Ordering::Relaxed);

        info!(
            "📤 TLS connection closed for SNI: {} ({}ms, ↓{}B ↑{}B)",
            sni_hostname, duration_ms, final_bytes_received, final_bytes_sent
        );

        if let Some(ref db_conn) = db {
            let disconnect_reason = match &result {
                Ok(()) => "completed".to_string(),
                Err(e) => format!("error: {}", e),
            };

            let update_connection =
                localup_relay_db::entities::captured_tcp_connection::ActiveModel {
                    id: sea_orm::Set(connection_id.clone()),
                    localup_id: sea_orm::Unchanged(route.localup_id),
                    client_addr: sea_orm::Unchanged(format!("{}|sni:{}", peer_addr, sni_hostname)),
                    target_port: sea_orm::Unchanged(tls_port as i32),
                    bytes_received: sea_orm::Set(final_bytes_received as i64),
                    bytes_sent: sea_orm::Set(final_bytes_sent as i64),
                    connected_at: sea_orm::Unchanged(metrics.connected_at.into()),
                    disconnected_at: sea_orm::Set(Some(disconnected_at.into())),
                    duration_ms: sea_orm::Set(Some(duration_ms)),
                    disconnect_reason: sea_orm::Set(Some(disconnect_reason)),
                };

            use sea_orm::EntityTrait;
            if let Err(e) = localup_relay_db::entities::prelude::CapturedTcpConnection::update(
                update_connection,
            )
            .exec(db_conn)
            .await
            {
                warn!(
                    "Failed to update TLS connection {} metrics: {}",
                    connection_id, e
                );
            } else {
                debug!("Updated TLS connection {} final metrics", connection_id);
            }
        }

        result
    }

    /// Forward bidirectional TCP streams with metrics tracking
    async fn forward_tcp_streams(
        client_socket: tokio::net::TcpStream,
        backend_socket: tokio::net::TcpStream,
        bytes_received: Arc<AtomicU64>,
        bytes_sent: Arc<AtomicU64>,
    ) -> Result<(), TlsServerError> {
        let (mut client_read, mut client_write) = client_socket.into_split();
        let (mut backend_read, mut backend_write) = backend_socket.into_split();

        let bytes_received_clone = bytes_received.clone();
        let client_to_backend = async move {
            let mut buf = [0u8; 8192];
            loop {
                match client_read.read(&mut buf).await {
                    Ok(0) => {
                        debug!("Client closed connection");
                        let _ = backend_write.shutdown().await;
                        break;
                    }
                    Ok(n) => {
                        bytes_received_clone.fetch_add(n as u64, Ordering::Relaxed);
                        if let Err(e) = backend_write.write_all(&buf[..n]).await {
                            debug!("Error writing to backend: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("Error reading from client: {}", e);
                        break;
                    }
                }
            }
        };

        let bytes_sent_clone = bytes_sent.clone();
        let backend_to_client = async move {
            let mut buf = [0u8; 8192];
            loop {
                match backend_read.read(&mut buf).await {
                    Ok(0) => {
                        debug!("Backend closed connection");
                        let _ = client_write.shutdown().await;
                        break;
                    }
                    Ok(n) => {
                        bytes_sent_clone.fetch_add(n as u64, Ordering::Relaxed);
                        if let Err(e) = client_write.write_all(&buf[..n]).await {
                            debug!("Error writing to client: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("Error reading from backend: {}", e);
                        break;
                    }
                }
            }
        };

        // Run both forwarding tasks concurrently
        tokio::select! {
            _ = client_to_backend => {},
            _ = backend_to_client => {},
        }

        Ok(())
    }

    /// Forward TLS stream through a QUIC tunnel using TransportStream trait
    async fn forward_via_transport_stream(
        client_socket: tokio::net::TcpStream,
        mut tunnel_stream: QuicStream,
        client_hello: &[u8],
        peer_addr: SocketAddr,
        bytes_received: Arc<AtomicU64>,
        bytes_sent: Arc<AtomicU64>,
    ) -> Result<(), TlsServerError> {
        // Generate stream ID for this tunnel connection
        static STREAM_COUNTER: AtomicU32 = AtomicU32::new(1);
        let stream_id = STREAM_COUNTER.fetch_add(1, Ordering::SeqCst);

        debug!(
            "Opening TLS tunnel stream {} for peer {}",
            stream_id, peer_addr
        );

        // Extract SNI from ClientHello for informational purposes
        let sni = "unknown".to_string(); // SNI was already extracted and routed; we know this is a valid tunnel

        // Send initial TlsConnect message with ClientHello
        let connect_msg = TunnelMessage::TlsConnect {
            stream_id,
            sni,
            client_hello: client_hello.to_vec(),
        };

        tunnel_stream
            .send_message(&connect_msg)
            .await
            .map_err(|e| {
                TlsServerError::TransportError(format!("Failed to send TlsConnect: {}", e))
            })?;

        debug!(
            "Sent TlsConnect message (stream {}) with {} bytes",
            stream_id,
            client_hello.len()
        );

        // Split the client socket for bidirectional forwarding
        let (mut client_read, mut client_write) = client_socket.into_split();

        // Split the QUIC stream for concurrent send/receive without mutexes
        let (mut tunnel_send, mut tunnel_recv) = tunnel_stream.split();

        // Bidirectional forwarding: client to tunnel
        let bytes_received_clone = bytes_received.clone();
        let client_to_tunnel = async move {
            let mut buf = [0u8; 8192];
            loop {
                match client_read.read(&mut buf).await {
                    Ok(0) => {
                        debug!("Client closed TLS connection (stream {})", stream_id);
                        let close_msg = TunnelMessage::TlsClose { stream_id };
                        let _ = tunnel_send.send_message(&close_msg).await;
                        break;
                    }
                    Ok(n) => {
                        bytes_received_clone.fetch_add(n as u64, Ordering::Relaxed);
                        let data_msg = TunnelMessage::TlsData {
                            stream_id,
                            data: buf[..n].to_vec(),
                        };
                        if let Err(e) = tunnel_send.send_message(&data_msg).await {
                            debug!("Error sending TLS data to tunnel: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("Error reading from TLS client: {}", e);
                        let close_msg = TunnelMessage::TlsClose { stream_id };
                        let _ = tunnel_send.send_message(&close_msg).await;
                        break;
                    }
                }
            }
        };

        // Tunnel to client
        let bytes_sent_clone = bytes_sent.clone();
        let tunnel_to_client = async move {
            loop {
                let msg = tunnel_recv.recv_message().await;

                match msg {
                    Ok(Some(TunnelMessage::TlsData {
                        stream_id: msg_stream_id,
                        data,
                    })) => {
                        if msg_stream_id != stream_id {
                            debug!(
                                "Received TLS data for wrong stream: expected {}, got {}",
                                stream_id, msg_stream_id
                            );
                            continue;
                        }
                        bytes_sent_clone.fetch_add(data.len() as u64, Ordering::Relaxed);
                        if let Err(e) = client_write.write_all(&data).await {
                            debug!("Error writing TLS data to client: {}", e);
                            break;
                        }
                    }
                    Ok(Some(TunnelMessage::TlsClose {
                        stream_id: msg_stream_id,
                    })) => {
                        if msg_stream_id == stream_id {
                            debug!("Tunnel closed TLS stream {}", stream_id);
                            let _ = client_write.shutdown().await;
                            break;
                        }
                    }
                    Ok(Some(_msg)) => {
                        debug!(
                            "Received unexpected message type for TLS stream {}",
                            stream_id
                        );
                    }
                    Ok(None) => {
                        debug!(
                            "Tunnel stream closed for TLS connection (stream {})",
                            stream_id
                        );
                        break;
                    }
                    Err(e) => {
                        debug!("Error receiving from tunnel: {}", e);
                        break;
                    }
                }
            }
        };

        // Run both tasks concurrently
        tokio::select! {
            _ = client_to_tunnel => {},
            _ = tunnel_to_client => {},
        }

        debug!("TLS tunnel connection closed for peer: {}", peer_addr);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_server_config() {
        let config = TlsServerConfig::default();
        assert_eq!(config.bind_addr.port(), 443);
    }

    #[test]
    fn test_tls_server_creation() {
        let config = TlsServerConfig::default();
        let route_registry = Arc::new(RouteRegistry::new());
        let server = TlsServer::new(config, route_registry);

        // Verify server was created
        assert_eq!(server.config.bind_addr.port(), 443);
    }

    #[tokio::test]
    async fn test_sni_routing() {
        use localup_proto::IpFilter;
        use localup_router::{RouteKey, RouteTarget};

        let route_registry = Arc::new(RouteRegistry::new());

        // Register a route for example.com
        let key = RouteKey::TlsSni("example.com".to_string());
        let target = RouteTarget {
            localup_id: "test-tunnel".to_string(),
            target_addr: "localhost:9443".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };
        route_registry.register(key, target).unwrap();

        let config = TlsServerConfig::default();
        let server = TlsServer::new(config, route_registry);

        // Verify SNI router has the route
        assert!(server.sni_router().has_route("example.com"));
        assert!(!server.sni_router().has_route("unknown.com"));
    }
}
