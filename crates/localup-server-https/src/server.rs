//! HTTPS server implementation with TLS termination
use localup_control::{PendingRequests, TunnelConnectionManager};
use localup_proto::TunnelMessage;
use localup_router::{RouteKey, RouteRegistry};
use localup_transport::TransportConnection;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn}; // For open_stream() method

#[derive(Debug, Error)]
pub enum HttpsServerError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("TLS error: {0}")]
    TlsError(String),

    #[error("Route error: {0}")]
    RouteError(String),

    #[error("Failed to bind to {address}: {reason}\n\nTroubleshooting:\n  • Check if another process is using this port: lsof -i :{port}\n  • Try using a different address or port")]
    BindError {
        address: String,
        port: u16,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct HttpsServerConfig {
    pub bind_addr: SocketAddr,
    pub cert_path: String,
    pub key_path: String,
}

impl Default for HttpsServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:443".parse().unwrap(),
            cert_path: "cert.pem".to_string(),
            key_path: "key.pem".to_string(),
        }
    }
}

pub struct HttpsServer {
    config: HttpsServerConfig,
    route_registry: Arc<RouteRegistry>,
    localup_manager: Option<Arc<TunnelConnectionManager>>,
    pending_requests: Option<Arc<PendingRequests>>,
}

impl HttpsServer {
    pub fn new(config: HttpsServerConfig, route_registry: Arc<RouteRegistry>) -> Self {
        Self {
            config,
            route_registry,
            localup_manager: None,
            pending_requests: None,
        }
    }

    pub fn with_localup_manager(mut self, manager: Arc<TunnelConnectionManager>) -> Self {
        self.localup_manager = Some(manager);
        self
    }

    pub fn with_pending_requests(mut self, pending: Arc<PendingRequests>) -> Self {
        self.pending_requests = Some(pending);
        self
    }

    /// Load TLS certificates from PEM files
    fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, HttpsServerError> {
        let file = File::open(path)
            .map_err(|e| HttpsServerError::TlsError(format!("Failed to open cert file: {}", e)))?;
        let mut reader = BufReader::new(file);

        rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| HttpsServerError::TlsError(format!("Failed to parse certs: {}", e)))
    }

    /// Load private key from PEM file
    fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, HttpsServerError> {
        let file = File::open(path)
            .map_err(|e| HttpsServerError::TlsError(format!("Failed to open key file: {}", e)))?;
        let mut reader = BufReader::new(file);

        rustls_pemfile::private_key(&mut reader)
            .map_err(|e| HttpsServerError::TlsError(format!("Failed to parse key: {}", e)))?
            .ok_or_else(|| HttpsServerError::TlsError("No private key found".to_string()))
    }

    /// Start the HTTPS server
    pub async fn start(self) -> Result<(), HttpsServerError> {
        let local_addr = self.config.bind_addr;

        // Load TLS certificates
        info!("Loading TLS certificate from: {}", self.config.cert_path);
        let certs = Self::load_certs(Path::new(&self.config.cert_path))?;

        info!("Loading TLS private key from: {}", self.config.key_path);
        let key = Self::load_private_key(Path::new(&self.config.key_path))?;

        // Build TLS config
        let tls_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| HttpsServerError::TlsError(format!("Invalid cert/key: {}", e)))?;

        let acceptor = TlsAcceptor::from(Arc::new(tls_config));

        // Bind TCP listener
        let listener = TcpListener::bind(local_addr).await.map_err(|e| {
            let port = local_addr.port();
            let address = local_addr.ip().to_string();
            let reason = e.to_string();
            HttpsServerError::BindError {
                address,
                port,
                reason,
            }
        })?;
        let bound_addr = listener.local_addr()?;

        info!("HTTPS server listening on {}", bound_addr);

        let route_registry = self.route_registry.clone();
        let localup_manager = self.localup_manager.clone();
        let pending_requests = self.pending_requests.clone();

        // Accept connections
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let acceptor = acceptor.clone();
                    let registry = route_registry.clone();
                    let manager = localup_manager.clone();
                    let pending = pending_requests.clone();

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(
                            stream, peer_addr, acceptor, registry, manager, pending,
                        )
                        .await
                        {
                            debug!("HTTPS connection error from {}: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept HTTPS connection: {}", e);
                }
            }
        }
    }

    async fn handle_connection(
        stream: TcpStream,
        peer_addr: SocketAddr,
        acceptor: TlsAcceptor,
        route_registry: Arc<RouteRegistry>,
        localup_manager: Option<Arc<TunnelConnectionManager>>,
        pending_requests: Option<Arc<PendingRequests>>,
    ) -> Result<(), HttpsServerError> {
        debug!("New HTTPS connection from {}", peer_addr);

        // TLS handshake
        let mut tls_stream = match acceptor.accept(stream).await {
            Ok(s) => s,
            Err(e) => {
                warn!("TLS handshake failed from {}: {}", peer_addr, e);
                return Err(HttpsServerError::TlsError(format!(
                    "Handshake failed: {}",
                    e
                )));
            }
        };

        debug!("TLS handshake completed for {}", peer_addr);

        // Read HTTP request
        let mut buffer = vec![0u8; 8192];
        let n = tls_stream.read(&mut buffer).await?;

        if n == 0 {
            return Ok(()); // Connection closed
        }

        buffer.truncate(n);
        let request = String::from_utf8_lossy(&buffer);

        // Parse HTTP request line and Host header
        let mut lines = request.lines();
        let _request_line = lines
            .next()
            .ok_or_else(|| HttpsServerError::RouteError("Empty request".to_string()))?;

        // Extract Host header
        let host = lines
            .find(|line| line.to_lowercase().starts_with("host:"))
            .and_then(|line| line.split(':').nth(1))
            .map(|h| h.trim())
            .ok_or_else(|| HttpsServerError::RouteError("No Host header".to_string()))?;

        debug!("HTTPS request for host: {}", host);

        // Lookup route
        let route_key = RouteKey::HttpHost(host.to_string());
        let target = match route_registry.lookup(&route_key) {
            Ok(t) => t,
            Err(_) => {
                warn!("No HTTPS route found for host: {}", host);
                let response = b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nNot Found";
                tls_stream.write_all(response).await?;
                return Ok(());
            }
        };

        // Check if this is a tunnel route
        if !target.target_addr.starts_with("tunnel:") {
            warn!("HTTPS route is not a tunnel: {}", target.target_addr);
            let response = b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 11\r\n\r\nBad Gateway";
            tls_stream.write_all(response).await?;
            return Ok(());
        }

        // Extract tunnel ID
        let localup_id = target.target_addr.strip_prefix("tunnel:").unwrap();

        // Forward through tunnel (same as HTTP server)
        if let (Some(manager), Some(pending)) = (localup_manager, pending_requests) {
            Self::handle_localup_request(
                tls_stream, manager, pending, localup_id, &request, &buffer,
            )
            .await?;
        } else {
            error!("Tunnel manager not configured for HTTPS");
            let response = b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 19\r\n\r\nService Unavailable";
            tls_stream.write_all(response.as_ref()).await?;
        }

        Ok(())
    }

    async fn handle_localup_request(
        mut tls_stream: tokio_rustls::server::TlsStream<TcpStream>,
        localup_manager: Arc<TunnelConnectionManager>,
        _pending_requests: Arc<PendingRequests>,
        localup_id: &str,
        _request: &str,
        request_bytes: &[u8],
    ) -> Result<(), HttpsServerError> {
        debug!("Transparent HTTPS streaming for tunnel: {}", localup_id);

        // Get tunnel connection
        let connection = match localup_manager.get(localup_id).await {
            Some(c) => c,
            None => {
                warn!("Tunnel not found: {}", localup_id);
                let response =
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 16\r\n\r\nTunnel not found\n";
                tls_stream.write_all(response).await?;
                return Ok(());
            }
        };

        // Generate stream ID
        let stream_id = rand::random::<u32>();

        // Open a new QUIC stream for transparent proxying
        let stream = match connection.open_stream().await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to open QUIC stream: {}", e);
                let response =
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 23\r\n\r\nTunnel stream error\n";
                tls_stream.write_all(response).await?;
                return Ok(());
            }
        };

        // Split stream for bidirectional communication
        let (mut quic_send, quic_recv) = stream.split();

        // Send initial HTTP request data through tunnel (transparent)
        // We already extracted the host for routing, now send ALL raw bytes
        let connect_msg = TunnelMessage::HttpStreamConnect {
            stream_id,
            host: localup_id.to_string(), // Use localup_id as host identifier
            initial_data: request_bytes.to_vec(),
        };

        if let Err(e) = quic_send.send_message(&connect_msg).await {
            error!("Failed to send HTTPS stream connect: {}", e);
            let response = b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 12\r\n\r\nTunnel error";
            tls_stream.write_all(response).await?;
            return Ok(());
        }

        debug!(
            "HTTPS transparent stream established (stream {})",
            stream_id
        );

        // Now enter bidirectional streaming loop
        Self::proxy_transparent_stream(tls_stream, quic_send, quic_recv, stream_id).await?;

        Ok(())
    }

    /// Bidirectional transparent streaming proxy
    async fn proxy_transparent_stream(
        mut tls_stream: tokio_rustls::server::TlsStream<TcpStream>,
        mut quic_send: localup_transport_quic::QuicSendHalf,
        mut quic_recv: localup_transport_quic::QuicRecvHalf,
        stream_id: u32,
    ) -> Result<(), HttpsServerError> {
        let mut client_buffer = vec![0u8; 16384];

        loop {
            tokio::select! {
                // Client → Tunnel
                result = tls_stream.read(&mut client_buffer) => {
                    match result {
                        Ok(0) => {
                            debug!("Client closed connection (stream {})", stream_id);
                            let _ = quic_send.send_message(&TunnelMessage::HttpStreamClose { stream_id }).await;
                            break;
                        }
                        Ok(n) => {
                            debug!("Forwarding {} bytes from client to tunnel (stream {})", n, stream_id);
                            let data_msg = TunnelMessage::HttpStreamData {
                                stream_id,
                                data: client_buffer[..n].to_vec(),
                            };
                            if let Err(e) = quic_send.send_message(&data_msg).await {
                                warn!("Failed to send data to tunnel: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Client read error (stream {}): {}", stream_id, e);
                            let _ = quic_send.send_message(&TunnelMessage::HttpStreamClose { stream_id }).await;
                            break;
                        }
                    }
                }

                // Tunnel → Client
                result = quic_recv.recv_message() => {
                    match result {
                        Ok(Some(TunnelMessage::HttpStreamData { data, .. })) => {
                            debug!("Forwarding {} bytes from tunnel to client (stream {})", data.len(), stream_id);
                            if let Err(e) = tls_stream.write_all(&data).await {
                                warn!("Failed to write to client: {}", e);
                                break;
                            }
                            if let Err(e) = tls_stream.flush().await {
                                warn!("Failed to flush to client: {}", e);
                                break;
                            }
                        }
                        Ok(Some(TunnelMessage::HttpStreamClose { .. })) => {
                            debug!("Tunnel closed stream {}", stream_id);
                            break;
                        }
                        Ok(None) => {
                            debug!("Tunnel stream ended (stream {})", stream_id);
                            break;
                        }
                        Err(e) => {
                            warn!("Tunnel read error (stream {}): {}", stream_id, e);
                            break;
                        }
                        _ => {
                            warn!("Unexpected message type from tunnel (stream {})", stream_id);
                        }
                    }
                }
            }
        }

        debug!("Transparent stream proxy ended (stream {})", stream_id);
        let _ = tls_stream.shutdown().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_https_server_config() {
        let config = HttpsServerConfig::default();
        assert_eq!(config.bind_addr.port(), 443);
    }
}
