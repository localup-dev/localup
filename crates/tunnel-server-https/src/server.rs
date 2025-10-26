//! HTTPS server implementation with TLS termination
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
use tracing::{debug, error, info, warn};
use tunnel_control::{PendingRequests, TunnelConnectionManager};
use tunnel_proto::TunnelMessage;
use tunnel_router::{RouteKey, RouteRegistry};
use tunnel_transport::TransportConnection; // For open_stream() method

#[derive(Debug, Error)]
pub enum HttpsServerError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("TLS error: {0}")]
    TlsError(String),

    #[error("Route error: {0}")]
    RouteError(String),
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
    tunnel_manager: Option<Arc<TunnelConnectionManager>>,
    pending_requests: Option<Arc<PendingRequests>>,
}

impl HttpsServer {
    pub fn new(config: HttpsServerConfig, route_registry: Arc<RouteRegistry>) -> Self {
        Self {
            config,
            route_registry,
            tunnel_manager: None,
            pending_requests: None,
        }
    }

    pub fn with_tunnel_manager(mut self, manager: Arc<TunnelConnectionManager>) -> Self {
        self.tunnel_manager = Some(manager);
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
        let listener = TcpListener::bind(local_addr).await?;
        let bound_addr = listener.local_addr()?;

        info!("HTTPS server listening on {}", bound_addr);

        let route_registry = self.route_registry.clone();
        let tunnel_manager = self.tunnel_manager.clone();
        let pending_requests = self.pending_requests.clone();

        // Accept connections
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let acceptor = acceptor.clone();
                    let registry = route_registry.clone();
                    let manager = tunnel_manager.clone();
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
        tunnel_manager: Option<Arc<TunnelConnectionManager>>,
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
        let tunnel_id = target.target_addr.strip_prefix("tunnel:").unwrap();

        // Forward through tunnel (same as HTTP server)
        if let (Some(manager), Some(pending)) = (tunnel_manager, pending_requests) {
            Self::handle_tunnel_request(tls_stream, manager, pending, tunnel_id, &request, &buffer)
                .await?;
        } else {
            error!("Tunnel manager not configured for HTTPS");
            let response = b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 19\r\n\r\nService Unavailable";
            tls_stream.write_all(response.as_ref()).await?;
        }

        Ok(())
    }

    async fn handle_tunnel_request(
        mut tls_stream: tokio_rustls::server::TlsStream<TcpStream>,
        tunnel_manager: Arc<TunnelConnectionManager>,
        _pending_requests: Arc<PendingRequests>, // Not needed with multi-stream
        tunnel_id: &str,
        request: &str,
        request_bytes: &[u8],
    ) -> Result<(), HttpsServerError> {
        debug!("Forwarding HTTPS request through tunnel: {}", tunnel_id);

        // Get tunnel connection
        let connection = match tunnel_manager.get(tunnel_id).await {
            Some(c) => c,
            None => {
                warn!("Tunnel not found: {}", tunnel_id);
                let response =
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 16\r\n\r\nTunnel not found\n";
                tls_stream.write_all(response).await?;
                return Ok(());
            }
        };

        // Get peer address for X-Forwarded-For
        let peer_addr = tls_stream.get_ref().0.peer_addr().ok();

        // Generate stream ID
        let stream_id = rand::random::<u32>();

        // Parse HTTP request (same as HTTP server)
        let mut lines = request.lines();
        let request_line = lines.next().unwrap_or("");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("GET").to_string();
        let uri = parts.next().unwrap_or("/").to_string();

        // Parse headers
        let mut headers = Vec::new();
        let mut body_start = 0;
        let mut original_host = String::new();

        for (i, line) in request.lines().enumerate() {
            if i == 0 {
                continue; // Skip request line
            }
            if line.is_empty() {
                // Calculate body start
                if let Some(pos) = request.find("\r\n\r\n") {
                    body_start = pos + 4;
                } else if let Some(pos) = request.find("\n\n") {
                    body_start = pos + 2;
                }
                break;
            }
            if let Some(colon_pos) = line.find(':') {
                let name = line[..colon_pos].trim().to_string();
                let value = line[colon_pos + 1..].trim().to_string();

                // Capture original Host header
                if name.to_lowercase() == "host" {
                    original_host = value.clone();
                }

                headers.push((name, value));
            }
        }

        // Add X-Forwarded-* headers
        if let Some(addr) = peer_addr {
            // X-Forwarded-For: client IP address
            headers.push(("X-Forwarded-For".to_string(), addr.ip().to_string()));
        }

        // X-Forwarded-Proto: always "https" for HTTPS server
        headers.push(("X-Forwarded-Proto".to_string(), "https".to_string()));

        // X-Forwarded-Host: original Host header
        if !original_host.is_empty() {
            headers.push(("X-Forwarded-Host".to_string(), original_host));
        }

        // Extract body
        let body = if body_start < request_bytes.len() {
            Some(request_bytes[body_start..].to_vec())
        } else {
            None
        };

        // Open a new QUIC stream for this HTTPS request
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

        // Split stream for bidirectional communication without mutexes
        let (mut quic_send, mut quic_recv) = stream.split();

        // Send HTTP request through tunnel
        let http_request = TunnelMessage::HttpRequest {
            stream_id,
            method,
            uri,
            headers,
            body,
        };

        if let Err(e) = quic_send.send_message(&http_request).await {
            error!("Failed to send HTTPS request to tunnel: {}", e);
            let response = b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 12\r\n\r\nTunnel error";
            tls_stream.write_all(response).await?;
            return Ok(());
        }

        debug!("HTTPS request sent to tunnel client (stream {})", stream_id);

        // Wait for response (with timeout)
        let response =
            tokio::time::timeout(std::time::Duration::from_secs(30), quic_recv.recv_message())
                .await;

        match response {
            Ok(Ok(Some(TunnelMessage::HttpResponse {
                stream_id: _,
                status,
                headers: resp_headers,
                body: resp_body,
            }))) => {
                // Build HTTP response
                let status_text = match status {
                    200 => "OK",
                    201 => "Created",
                    204 => "No Content",
                    301 => "Moved Permanently",
                    302 => "Found",
                    304 => "Not Modified",
                    400 => "Bad Request",
                    401 => "Unauthorized",
                    403 => "Forbidden",
                    404 => "Not Found",
                    500 => "Internal Server Error",
                    502 => "Bad Gateway",
                    503 => "Service Unavailable",
                    _ => "Unknown",
                };

                let response_line = format!("HTTP/1.1 {} {}\r\n", status, status_text);
                tls_stream.write_all(response_line.as_bytes()).await?;

                // Forward headers (skip Content-Length, we'll add our own)
                for (name, value) in resp_headers {
                    if name.to_lowercase() == "content-length" {
                        continue;
                    }
                    tls_stream
                        .write_all(format!("{}: {}\r\n", name, value).as_bytes())
                        .await?;
                }

                // Write body with correct Content-Length
                if let Some(body) = resp_body {
                    tls_stream
                        .write_all(format!("Content-Length: {}\r\n", body.len()).as_bytes())
                        .await?;
                    tls_stream.write_all(b"\r\n").await?;
                    tls_stream.write_all(&body).await?;
                } else {
                    tls_stream.write_all(b"Content-Length: 0\r\n\r\n").await?;
                }

                // Flush the TLS stream to ensure all data is sent
                tls_stream.flush().await?;

                debug!(
                    "HTTPS response forwarded to client: {} {}",
                    status, status_text
                );
            }
            Ok(Ok(Some(msg))) => {
                warn!("Unexpected message from tunnel: {:?}", msg);
                let response =
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 20\r\n\r\nUnexpected response\n";
                tls_stream.write_all(response).await?;
                tls_stream.flush().await?;
            }
            Ok(Ok(None)) => {
                warn!("Tunnel stream closed before sending response");
                let response =
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 14\r\n\r\nTunnel closed\n";
                tls_stream.write_all(response).await?;
                tls_stream.flush().await?;
            }
            Ok(Err(e)) => {
                warn!("Error reading from tunnel: {}", e);
                let response =
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 14\r\n\r\nTunnel error\n";
                tls_stream.write_all(response).await?;
                tls_stream.flush().await?;
            }
            Err(_) => {
                warn!("Timeout waiting for tunnel response (stream {})", stream_id);
                let response =
                    b"HTTP/1.1 504 Gateway Timeout\r\nContent-Length: 15\r\n\r\nTunnel timeout\n";
                tls_stream.write_all(response).await?;
                tls_stream.flush().await?;
            }
        }

        // Gracefully shutdown TLS connection
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
