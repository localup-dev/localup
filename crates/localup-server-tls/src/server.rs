//! TLS server with SNI-based passthrough routing
//!
//! This server accepts incoming TLS connections, extracts the SNI (Server Name Indication)
//! from the ClientHello, and routes the connection to the appropriate backend service.
//!
//! No TLS termination is performed - the TLS stream is forwarded as-is to preserve
//! end-to-end encryption between the client and backend service.
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info};

use localup_control::TunnelConnectionManager;
use localup_proto::TunnelMessage;
use localup_router::{RouteRegistry, SniRouter};
use localup_transport::TransportConnection;
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

pub struct TlsServer {
    config: TlsServerConfig,
    sni_router: Arc<SniRouter>,
    tunnel_manager: Option<Arc<TunnelConnectionManager>>,
}

impl TlsServer {
    /// Create a new TLS server with SNI routing
    pub fn new(config: TlsServerConfig, route_registry: Arc<RouteRegistry>) -> Self {
        let sni_router = Arc::new(SniRouter::new(route_registry));
        Self {
            config,
            sni_router,
            tunnel_manager: None,
        }
    }

    /// Set the tunnel connection manager for forwarding to tunnels
    pub fn with_localup_manager(mut self, manager: Arc<TunnelConnectionManager>) -> Self {
        self.tunnel_manager = Some(manager);
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

                    tokio::spawn(async move {
                        // Forward the raw TLS stream based on SNI extraction
                        if let Err(e) =
                            Self::forward_tls_stream(socket, &sni_router, tunnel_manager, peer_addr)
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

        debug!("Extracted SNI: {} from client {}", sni_hostname, peer_addr);

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
            debug!(
                "Connection from {} denied by IP filter for SNI: {}",
                peer_addr, sni_hostname
            );
            return Err(TlsServerError::AccessDenied(peer_addr.to_string()));
        }

        debug!(
            "Routing SNI {} to backend: {}",
            sni_hostname, route.target_addr
        );

        // Check if this is a tunnel target (format: tunnel:localup_id)
        if route.target_addr.starts_with("tunnel:") {
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
            return Self::forward_via_transport_stream(
                client_socket,
                backend_stream,
                &client_hello_buf[..n],
                peer_addr,
            )
            .await;
        }

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

        // Bidirectionally forward data between client and backend
        let (mut client_read, mut client_write) = client_socket.into_split();
        let (mut backend_read, mut backend_write) = backend_socket.into_split();

        let client_to_backend = async {
            let mut buf = [0u8; 4096];
            loop {
                match client_read.read(&mut buf).await {
                    Ok(0) => {
                        debug!("Client closed connection");
                        let _ = backend_write.shutdown().await;
                        break;
                    }
                    Ok(n) => {
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

        let backend_to_client = async {
            let mut buf = [0u8; 4096];
            loop {
                match backend_read.read(&mut buf).await {
                    Ok(0) => {
                        debug!("Backend closed connection");
                        let _ = client_write.shutdown().await;
                        break;
                    }
                    Ok(n) => {
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

        debug!(
            "TLS passthrough connection closed for SNI: {}",
            sni_hostname
        );
        Ok(())
    }

    /// Forward TLS stream through a QUIC tunnel using TransportStream trait
    async fn forward_via_transport_stream<S: localup_transport::TransportStream>(
        client_socket: tokio::net::TcpStream,
        mut tunnel_stream: S,
        client_hello: &[u8],
        peer_addr: SocketAddr,
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

        // Wrap tunnel stream in Arc<Mutex<>> for shared access
        let tunnel_stream = Arc::new(tokio::sync::Mutex::new(tunnel_stream));
        let tunnel_send = tunnel_stream.clone();
        let tunnel_recv = tunnel_stream.clone();

        // Bidirectional forwarding: client to tunnel
        let client_to_tunnel = async {
            let mut buf = [0u8; 4096];
            loop {
                match client_read.read(&mut buf).await {
                    Ok(0) => {
                        debug!("Client closed TLS connection (stream {})", stream_id);
                        let close_msg = TunnelMessage::TlsClose { stream_id };
                        let mut tunnel = tunnel_send.lock().await;
                        let _ = tunnel.send_message(&close_msg).await;
                        break;
                    }
                    Ok(n) => {
                        let data_msg = TunnelMessage::TlsData {
                            stream_id,
                            data: buf[..n].to_vec(),
                        };
                        let mut tunnel = tunnel_send.lock().await;
                        if let Err(e) = tunnel.send_message(&data_msg).await {
                            debug!("Error sending TLS data to tunnel: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("Error reading from TLS client: {}", e);
                        let close_msg = TunnelMessage::TlsClose { stream_id };
                        let mut tunnel = tunnel_send.lock().await;
                        let _ = tunnel.send_message(&close_msg).await;
                        break;
                    }
                }
            }
        };

        // Tunnel to client
        let tunnel_to_client = async {
            loop {
                let msg = {
                    let mut tunnel = tunnel_recv.lock().await;
                    tunnel.recv_message().await
                };

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
