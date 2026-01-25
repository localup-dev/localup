//! HTTP passthrough server with Host-based routing
//!
//! This server accepts incoming HTTP connections, extracts the Host header
//! from the first request, and routes the connection to the appropriate backend service.
//!
//! No HTTP processing is performed beyond initial Host extraction - the stream is
//! forwarded as-is to preserve the original request/response flow.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};

use localup_control::TunnelConnectionManager;
use localup_proto::TunnelMessage;
use localup_router::{RouteRegistry, SniRouter};
use localup_transport::{TransportConnection, TransportStream};
use sea_orm::DatabaseConnection;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Error)]
pub enum HttpPassthroughError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Host header extraction failed")]
    HostExtractionFailed,

    #[error("No route found for host: {0}")]
    NoRoute(String),

    #[error("Access denied for IP: {0}")]
    AccessDenied(String),

    #[error("Transport error: {0}")]
    TransportError(String),

    #[error("Failed to bind to {address}: {reason}\n\nTroubleshooting:\n  • Check if another process is using this port: lsof -i :{port}\n  • Try using a different address or port")]
    BindError {
        address: String,
        port: u16,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct HttpPassthroughConfig {
    pub bind_addr: SocketAddr,
}

impl Default for HttpPassthroughConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:80".parse().unwrap(),
        }
    }
}

/// Tracks metrics for an individual HTTP connection
struct HttpConnectionMetrics {
    bytes_received: Arc<AtomicU64>,
    bytes_sent: Arc<AtomicU64>,
    connected_at: chrono::DateTime<chrono::Utc>,
}

pub struct HttpPassthroughServer {
    config: HttpPassthroughConfig,
    sni_router: Arc<SniRouter>,
    tunnel_manager: Option<Arc<TunnelConnectionManager>>,
    db: Option<DatabaseConnection>,
}

impl HttpPassthroughServer {
    /// Create a new HTTP passthrough server with Host-based routing
    pub fn new(config: HttpPassthroughConfig, route_registry: Arc<RouteRegistry>) -> Self {
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

    /// Start the HTTP passthrough server
    pub async fn start(&self) -> Result<(), HttpPassthroughError> {
        info!(
            "HTTP passthrough server starting on {}",
            self.config.bind_addr
        );

        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| {
                let port = self.config.bind_addr.port();
                let address = self.config.bind_addr.ip().to_string();
                let reason = e.to_string();
                HttpPassthroughError::BindError {
                    address,
                    port,
                    reason,
                }
            })?;
        info!(
            "✅ HTTP passthrough server listening on {} (Host header routing)",
            self.config.bind_addr
        );

        loop {
            match listener.accept().await {
                Ok((socket, peer_addr)) => {
                    debug!("New HTTP connection from {}", peer_addr);

                    let sni_router = self.sni_router.clone();
                    let tunnel_manager = self.tunnel_manager.clone();
                    let db = self.db.clone();
                    let http_port = self.config.bind_addr.port();

                    tokio::spawn(async move {
                        if let Err(e) = Self::forward_http_stream(
                            socket,
                            &sni_router,
                            tunnel_manager,
                            peer_addr,
                            db,
                            http_port,
                        )
                        .await
                        {
                            debug!("Error forwarding HTTP stream from {}: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("HTTP listener accept error: {}", e);
                }
            }
        }
    }

    /// Extract Host header from HTTP request
    fn extract_host(data: &[u8]) -> Option<String> {
        // Convert to string for parsing
        let request_str = std::str::from_utf8(data).ok()?;

        // Look for Host header (case-insensitive)
        for line in request_str.lines() {
            if line.is_empty() {
                // End of headers
                break;
            }
            let line_lower = line.to_lowercase();
            if line_lower.starts_with("host:") {
                let host_value = &line[5..]; // Skip "Host:" or "host:"
                let host = host_value.trim();
                // Remove port if present
                let hostname = host.split(':').next().unwrap_or(host);
                return Some(hostname.to_string());
            }
        }
        None
    }

    /// Forward HTTP stream to backend based on Host header extraction
    async fn forward_http_stream(
        mut client_socket: tokio::net::TcpStream,
        sni_router: &Arc<SniRouter>,
        tunnel_manager: Option<Arc<TunnelConnectionManager>>,
        peer_addr: SocketAddr,
        db: Option<DatabaseConnection>,
        http_port: u16,
    ) -> Result<(), HttpPassthroughError> {
        // Read the initial HTTP request
        let mut request_buf = [0u8; 16384];
        let n = client_socket
            .read(&mut request_buf)
            .await
            .map_err(|e| HttpPassthroughError::TransportError(e.to_string()))?;

        if n == 0 {
            debug!("Client closed connection before sending request");
            return Ok(());
        }

        debug!("Received {} bytes from HTTP client", n);

        // Extract Host header from the request
        let hostname = Self::extract_host(&request_buf[..n]).ok_or_else(|| {
            debug!("Host header extraction failed from {}", peer_addr);
            HttpPassthroughError::HostExtractionFailed
        })?;

        info!(
            "📥 HTTP connection from {} for Host: {}",
            peer_addr, hostname
        );

        // Look up the route for this hostname (using SNI router which works for any hostname)
        let route = sni_router.lookup(&hostname).map_err(|e| {
            debug!(
                "No route found for Host {} from {}: {}",
                hostname, peer_addr, e
            );
            HttpPassthroughError::NoRoute(hostname.clone())
        })?;

        // Check IP filtering
        if !route.is_ip_allowed(&peer_addr) {
            warn!(
                "🚫 Connection from {} denied by IP filter for Host: {}",
                peer_addr, hostname
            );
            return Err(HttpPassthroughError::AccessDenied(peer_addr.to_string()));
        }

        info!(
            "🔀 Routing Host {} to backend: {} (localup: {})",
            hostname, route.target_addr, route.localup_id
        );

        // Create connection metrics tracker
        let connection_id = uuid::Uuid::new_v4().to_string();
        let metrics = HttpConnectionMetrics {
            bytes_received: Arc::new(AtomicU64::new(n as u64)),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            connected_at: chrono::Utc::now(),
        };

        // Save active connection to database
        if let Some(ref db_conn) = db {
            let active_connection =
                localup_relay_db::entities::captured_tcp_connection::ActiveModel {
                    id: sea_orm::Set(connection_id.clone()),
                    localup_id: sea_orm::Set(route.localup_id.clone()),
                    client_addr: sea_orm::Set(format!("{}|host:{}", peer_addr, hostname)),
                    target_port: sea_orm::Set(http_port as i32),
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
                    "Failed to save active HTTP connection {}: {}",
                    connection_id, e
                );
            } else {
                debug!("Saved active HTTP connection {} to database", connection_id);
            }
        }

        // Check if this is a tunnel target (format: tunnel:localup_id)
        let result = if route.target_addr.starts_with("tunnel:") {
            let localup_id = route.target_addr.strip_prefix("tunnel:").unwrap();

            let manager = tunnel_manager.ok_or_else(|| {
                HttpPassthroughError::TransportError(
                    "Tunnel target requested but tunnel manager not configured".to_string(),
                )
            })?;

            let connection = manager.get(localup_id).await.ok_or_else(|| {
                HttpPassthroughError::TransportError(format!("Tunnel not found: {}", localup_id))
            })?;

            let backend_stream = connection.open_stream().await.map_err(|e| {
                HttpPassthroughError::TransportError(format!(
                    "Failed to open stream to tunnel {}: {}",
                    localup_id, e
                ))
            })?;

            Self::forward_via_tunnel(
                client_socket,
                backend_stream,
                &request_buf[..n],
                peer_addr,
                &hostname,
                metrics.bytes_received.clone(),
                metrics.bytes_sent.clone(),
            )
            .await
        } else {
            // Direct backend connection (legacy support)
            let backend_addr: SocketAddr = route.target_addr.parse().map_err(|e| {
                HttpPassthroughError::TransportError(format!(
                    "Invalid backend address {}: {}",
                    route.target_addr, e
                ))
            })?;

            Self::forward_to_backend(
                client_socket,
                backend_addr,
                &request_buf[..n],
                metrics.bytes_received.clone(),
                metrics.bytes_sent.clone(),
            )
            .await
        };

        // Update database with final metrics
        let disconnected_at = chrono::Utc::now();
        let duration_ms = (disconnected_at - metrics.connected_at).num_milliseconds();

        if let Some(ref db_conn) = db {
            use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

            let update = localup_relay_db::entities::captured_tcp_connection::ActiveModel {
                id: sea_orm::Set(connection_id.clone()),
                bytes_received: sea_orm::Set(metrics.bytes_received.load(Ordering::Relaxed) as i64),
                bytes_sent: sea_orm::Set(metrics.bytes_sent.load(Ordering::Relaxed) as i64),
                disconnected_at: sea_orm::Set(Some(disconnected_at.into())),
                duration_ms: sea_orm::Set(Some(duration_ms as i32)),
                disconnect_reason: sea_orm::Set(result.as_ref().err().map(|e| e.to_string())),
                ..Default::default()
            };

            if let Err(e) =
                localup_relay_db::entities::prelude::CapturedTcpConnection::update(update)
                    .filter(
                        localup_relay_db::entities::captured_tcp_connection::Column::Id
                            .eq(&connection_id),
                    )
                    .exec(db_conn)
                    .await
            {
                warn!(
                    "Failed to update HTTP connection metrics {}: {}",
                    connection_id, e
                );
            }
        }

        info!(
            "📤 HTTP connection closed: {} (duration: {}ms, received: {} bytes, sent: {} bytes)",
            hostname,
            duration_ms,
            metrics.bytes_received.load(Ordering::Relaxed),
            metrics.bytes_sent.load(Ordering::Relaxed)
        );

        result
    }

    /// Forward stream via QUIC tunnel using TlsConnect/TlsData protocol
    /// (Uses TLS message types so TLS tunnel clients can handle HTTP passthrough)
    async fn forward_via_tunnel(
        client_socket: tokio::net::TcpStream,
        mut tunnel_stream: localup_transport_quic::QuicStream,
        initial_data: &[u8],
        peer_addr: SocketAddr,
        hostname: &str,
        bytes_received: Arc<AtomicU64>,
        bytes_sent: Arc<AtomicU64>,
    ) -> Result<(), HttpPassthroughError> {
        // Generate stream ID for this connection
        static STREAM_COUNTER: AtomicU32 = AtomicU32::new(1);
        let stream_id = STREAM_COUNTER.fetch_add(1, Ordering::SeqCst);

        debug!(
            "Opening HTTP tunnel stream {} for peer {} (Host: {})",
            stream_id, peer_addr, hostname
        );

        // Send TlsConnect message with the HTTP request as "client_hello"
        // This allows TLS tunnel clients to handle HTTP passthrough traffic
        // The SNI field contains the Host header value for routing info
        let connect_msg = TunnelMessage::TlsConnect {
            stream_id,
            sni: hostname.to_string(),
            client_hello: initial_data.to_vec(),
        };

        tunnel_stream
            .send_message(&connect_msg)
            .await
            .map_err(|e| {
                HttpPassthroughError::TransportError(format!("Failed to send TlsConnect: {}", e))
            })?;

        debug!(
            "Sent TlsConnect with {} bytes on stream {} (Host: {})",
            initial_data.len(),
            stream_id,
            hostname
        );

        // Split both streams for bidirectional forwarding
        let (mut client_read, mut client_write) = client_socket.into_split();
        let (mut tunnel_send, mut tunnel_recv) = tunnel_stream.split();

        // Client to tunnel
        let bytes_received_clone = bytes_received.clone();
        let client_to_tunnel = async move {
            let mut buf = [0u8; 8192];
            loop {
                match client_read.read(&mut buf).await {
                    Ok(0) => {
                        debug!("Client closed HTTP connection (stream {})", stream_id);
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
                            debug!("Error sending HTTP data to tunnel: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("Error reading from HTTP client: {}", e);
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
                match tunnel_recv.recv_message().await {
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
                            debug!("Error writing HTTP data to client: {}", e);
                            break;
                        }
                    }
                    Ok(Some(TunnelMessage::TlsClose {
                        stream_id: msg_stream_id,
                    })) => {
                        if msg_stream_id == stream_id {
                            debug!("Tunnel closed HTTP stream {}", stream_id);
                            let _ = client_write.shutdown().await;
                            break;
                        }
                    }
                    Ok(Some(_msg)) => {
                        debug!(
                            "Received unexpected message type for HTTP stream {}",
                            stream_id
                        );
                    }
                    Ok(None) => {
                        debug!(
                            "Tunnel stream closed for HTTP connection (stream {})",
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
            _ = client_to_tunnel => {}
            _ = tunnel_to_client => {}
        }

        debug!("HTTP tunnel connection closed for peer: {}", peer_addr);
        Ok(())
    }

    /// Forward to direct backend (legacy support)
    async fn forward_to_backend(
        mut client_socket: tokio::net::TcpStream,
        backend_addr: SocketAddr,
        initial_data: &[u8],
        bytes_received: Arc<AtomicU64>,
        bytes_sent: Arc<AtomicU64>,
    ) -> Result<(), HttpPassthroughError> {
        let mut backend_socket =
            tokio::net::TcpStream::connect(backend_addr)
                .await
                .map_err(|e| {
                    HttpPassthroughError::TransportError(format!(
                        "Failed to connect to backend {}: {}",
                        backend_addr, e
                    ))
                })?;

        // Send initial data
        backend_socket
            .write_all(initial_data)
            .await
            .map_err(|e| HttpPassthroughError::TransportError(e.to_string()))?;

        // Bidirectional forwarding
        let (mut client_read, mut client_write) = client_socket.split();
        let (mut backend_read, mut backend_write) = backend_socket.split();

        let bytes_received_clone = bytes_received.clone();
        let bytes_sent_clone = bytes_sent.clone();

        let client_to_backend = async {
            let mut buf = vec![0u8; 65536];
            loop {
                match client_read.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        bytes_received_clone.fetch_add(n as u64, Ordering::Relaxed);
                        if backend_write.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        };

        let backend_to_client = async {
            let mut buf = vec![0u8; 65536];
            loop {
                match backend_read.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        bytes_sent_clone.fetch_add(n as u64, Ordering::Relaxed);
                        if client_write.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        };

        tokio::select! {
            _ = client_to_backend => {}
            _ = backend_to_client => {}
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_host_basic() {
        let request = b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
        assert_eq!(
            HttpPassthroughServer::extract_host(request),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn test_extract_host_with_port() {
        let request = b"GET / HTTP/1.1\r\nHost: example.com:8080\r\n\r\n";
        assert_eq!(
            HttpPassthroughServer::extract_host(request),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn test_extract_host_lowercase() {
        let request = b"GET / HTTP/1.1\r\nhost: example.com\r\n\r\n";
        assert_eq!(
            HttpPassthroughServer::extract_host(request),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn test_extract_host_mixed_case() {
        let request = b"GET / HTTP/1.1\r\nHoSt: Example.COM\r\n\r\n";
        assert_eq!(
            HttpPassthroughServer::extract_host(request),
            Some("Example.COM".to_string())
        );
    }

    #[test]
    fn test_extract_host_with_path() {
        let request = b"GET /api/v1/users HTTP/1.1\r\nHost: api.example.com\r\nContent-Type: application/json\r\n\r\n";
        assert_eq!(
            HttpPassthroughServer::extract_host(request),
            Some("api.example.com".to_string())
        );
    }

    #[test]
    fn test_extract_host_subdomain() {
        let request = b"GET / HTTP/1.1\r\nHost: sub.domain.example.com\r\n\r\n";
        assert_eq!(
            HttpPassthroughServer::extract_host(request),
            Some("sub.domain.example.com".to_string())
        );
    }

    #[test]
    fn test_extract_host_missing() {
        let request = b"GET / HTTP/1.1\r\nContent-Type: text/html\r\n\r\n";
        assert_eq!(HttpPassthroughServer::extract_host(request), None);
    }

    #[test]
    fn test_extract_host_empty_request() {
        let request = b"";
        assert_eq!(HttpPassthroughServer::extract_host(request), None);
    }

    #[test]
    fn test_extract_host_post_request() {
        let request = b"POST /submit HTTP/1.1\r\nHost: forms.example.com\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: 13\r\n\r\ndata=example";
        assert_eq!(
            HttpPassthroughServer::extract_host(request),
            Some("forms.example.com".to_string())
        );
    }

    #[test]
    fn test_extract_host_with_whitespace() {
        let request = b"GET / HTTP/1.1\r\nHost:   example.com   \r\n\r\n";
        assert_eq!(
            HttpPassthroughServer::extract_host(request),
            Some("example.com".to_string())
        );
    }

    #[test]
    fn test_extract_host_ipv4() {
        let request = b"GET / HTTP/1.1\r\nHost: 192.168.1.1\r\n\r\n";
        assert_eq!(
            HttpPassthroughServer::extract_host(request),
            Some("192.168.1.1".to_string())
        );
    }

    #[test]
    fn test_extract_host_ipv4_with_port() {
        let request = b"GET / HTTP/1.1\r\nHost: 192.168.1.1:8080\r\n\r\n";
        assert_eq!(
            HttpPassthroughServer::extract_host(request),
            Some("192.168.1.1".to_string())
        );
    }

    #[test]
    fn test_extract_host_localhost() {
        let request = b"GET / HTTP/1.1\r\nHost: localhost:3000\r\n\r\n";
        assert_eq!(
            HttpPassthroughServer::extract_host(request),
            Some("localhost".to_string())
        );
    }

    #[test]
    fn test_config_default() {
        let config = HttpPassthroughConfig::default();
        assert_eq!(config.bind_addr.port(), 80);
    }
}
