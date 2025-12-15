//! HTTPS server implementation with TLS termination
use localup_control::{PendingRequests, TunnelConnectionManager};
use localup_proto::TunnelMessage;
use localup_relay_db::entities::custom_domain;
use localup_router::{RouteKey, RouteRegistry};
use localup_transport::TransportConnection;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::server::{ClientHello, ResolvesServerCert};
use tokio_rustls::rustls::{sign::CertifiedKey, ServerConfig};
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};

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
    db: Option<DatabaseConnection>,
}

/// Captured response data from transparent proxy
struct ResponseCapture {
    status: Option<u16>,
    headers: Option<Vec<(String, String)>>,
    body: Option<Vec<u8>>,
}

/// SNI-based certificate resolver that supports custom domain certificates
/// This resolver can be shared and updated at runtime for hot-reload support.
#[derive(Debug)]
pub struct CustomCertResolver {
    default_cert: Arc<CertifiedKey>,
    custom_certs: Arc<RwLock<HashMap<String, Arc<CertifiedKey>>>>,
}

impl CustomCertResolver {
    /// Create a new certificate resolver with a default certificate
    pub fn new(default_cert: Arc<CertifiedKey>) -> Self {
        Self {
            default_cert,
            custom_certs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add or update a custom certificate for a domain (hot-reload support)
    pub async fn add_custom_cert(&self, domain: String, cert: Arc<CertifiedKey>) {
        let mut certs = self.custom_certs.write().await;
        info!("Adding/updating custom certificate for domain: {}", domain);
        certs.insert(domain, cert);
    }

    /// Remove a custom certificate for a domain
    pub async fn remove_custom_cert(&self, domain: &str) -> bool {
        let mut certs = self.custom_certs.write().await;
        let removed = certs.remove(domain).is_some();
        if removed {
            info!("Removed custom certificate for domain: {}", domain);
        }
        removed
    }

    /// Check if a custom certificate exists for a domain
    pub async fn has_custom_cert(&self, domain: &str) -> bool {
        let certs = self.custom_certs.read().await;
        certs.contains_key(domain)
    }

    /// List all domains with custom certificates
    pub async fn list_domains(&self) -> Vec<String> {
        let certs = self.custom_certs.read().await;
        certs.keys().cloned().collect()
    }

    /// Get the number of custom certificates loaded
    pub async fn custom_cert_count(&self) -> usize {
        let certs = self.custom_certs.read().await;
        certs.len()
    }
}

impl ResolvesServerCert for CustomCertResolver {
    fn resolve(&self, client_hello: ClientHello) -> Option<Arc<CertifiedKey>> {
        // Get SNI hostname from client hello
        let sni_hostname = client_hello.server_name()?;
        let domain = sni_hostname;

        debug!("SNI hostname: {}", domain);

        // Try to find custom cert for this domain
        // Note: We can't use async here, so we use try_read() which is non-blocking
        if let Ok(certs) = self.custom_certs.try_read() {
            if let Some(cert) = certs.get(domain) {
                info!("Using custom certificate for domain: {}", domain);
                return Some(cert.clone());
            }
        }

        // Fall back to default certificate
        debug!("Using default certificate for domain: {}", domain);
        Some(self.default_cert.clone())
    }
}

impl HttpsServer {
    pub fn new(config: HttpsServerConfig, route_registry: Arc<RouteRegistry>) -> Self {
        Self {
            config,
            route_registry,
            localup_manager: None,
            pending_requests: None,
            db: None,
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

    pub fn with_database(mut self, db: DatabaseConnection) -> Self {
        self.db = Some(db);
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

    /// Load custom domain certificates from database
    async fn load_custom_domain_certs(
        db: &DatabaseConnection,
        resolver: &Arc<CustomCertResolver>,
    ) -> Result<usize, HttpsServerError> {
        use localup_relay_db::entities::custom_domain::DomainStatus;

        // Query all active custom domains
        let domains = custom_domain::Entity::find()
            .filter(custom_domain::Column::Status.eq(DomainStatus::Active))
            .all(db)
            .await
            .map_err(|e| {
                HttpsServerError::TlsError(format!("Database error loading custom domains: {}", e))
            })?;

        let mut loaded_count = 0;

        for domain in domains {
            // Skip if cert or key path is missing
            let cert_path = match &domain.cert_path {
                Some(path) => path,
                None => {
                    warn!("Domain {} has no cert_path, skipping", domain.domain);
                    continue;
                }
            };
            let key_path = match &domain.key_path {
                Some(path) => path,
                None => {
                    warn!("Domain {} has no key_path, skipping", domain.domain);
                    continue;
                }
            };

            // Load certificate and key
            match Self::load_domain_cert(cert_path, key_path) {
                Ok(cert_key) => {
                    resolver
                        .add_custom_cert(domain.domain.clone(), Arc::new(cert_key))
                        .await;
                    loaded_count += 1;
                }
                Err(e) => {
                    warn!(
                        "Failed to load certificate for domain {}: {}",
                        domain.domain, e
                    );
                }
            }
        }

        Ok(loaded_count)
    }

    /// Load a single domain's certificate and key into a CertifiedKey
    /// This can be used for hot-reload of certificates.
    pub fn load_domain_cert(
        cert_path: &str,
        key_path: &str,
    ) -> Result<CertifiedKey, HttpsServerError> {
        let certs = Self::load_certs(Path::new(cert_path))?;
        let key = Self::load_private_key(Path::new(key_path))?;

        let signing_key = rustls::crypto::ring::sign::any_supported_type(&key)
            .map_err(|e| HttpsServerError::TlsError(format!("Invalid key: {}", e)))?;

        Ok(CertifiedKey::new(certs, signing_key))
    }

    /// Start the HTTPS server
    pub async fn start(self) -> Result<(), HttpsServerError> {
        let local_addr = self.config.bind_addr;

        // Load default TLS certificate
        info!(
            "Loading default TLS certificate from: {}",
            self.config.cert_path
        );
        let certs = Self::load_certs(Path::new(&self.config.cert_path))?;

        info!(
            "Loading default TLS private key from: {}",
            self.config.key_path
        );
        let key = Self::load_private_key(Path::new(&self.config.key_path))?;

        // Create CertifiedKey for default certificate
        let signing_key = rustls::crypto::ring::sign::any_supported_type(&key)
            .map_err(|e| HttpsServerError::TlsError(format!("Invalid key: {}", e)))?;

        let default_cert = Arc::new(CertifiedKey::new(certs, signing_key));

        // Create custom cert resolver with default certificate
        let cert_resolver = Arc::new(CustomCertResolver::new(default_cert));

        // Load custom domain certificates from database if available
        if let Some(ref db) = self.db {
            info!("Loading custom domain certificates from database");
            match Self::load_custom_domain_certs(db, &cert_resolver).await {
                Ok(count) => info!("Loaded {} custom domain certificate(s)", count),
                Err(e) => warn!("Failed to load custom domain certificates: {}", e),
            }
        }

        // Build TLS config with custom resolver
        let tls_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(cert_resolver);

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
        let db = self.db.clone();

        // Accept connections
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let acceptor = acceptor.clone();
                    let registry = route_registry.clone();
                    let manager = localup_manager.clone();
                    let pending = pending_requests.clone();
                    let db = db.clone();

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(
                            stream, peer_addr, acceptor, registry, manager, pending, db,
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
        db: Option<DatabaseConnection>,
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
                tls_stream, manager, pending, localup_id, &request, &buffer, db,
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
        request: &str,
        request_bytes: &[u8],
        db: Option<DatabaseConnection>,
    ) -> Result<(), HttpsServerError> {
        debug!("Transparent HTTPS streaming for tunnel: {}", localup_id);

        // Record start time and generate request ID for database capture
        let request_start = chrono::Utc::now();
        let request_id = uuid::Uuid::new_v4().to_string();

        // Parse request for database capture
        let (method, uri, headers) = Self::parse_http_request(request);
        let host = Self::extract_host_from_request(request);

        // Extract body from request bytes (after \r\n\r\n)
        let body = if let Some(pos) = request.find("\r\n\r\n") {
            let body_offset = pos + 4;
            if body_offset < request_bytes.len() {
                Some(request_bytes[body_offset..].to_vec())
            } else {
                None
            }
        } else {
            None
        };

        // Check HTTP authentication if configured for this tunnel
        if let Some(authenticator) = localup_manager.get_http_authenticator(localup_id).await {
            if authenticator.requires_auth() {
                // Parse headers from request
                let headers = localup_http_auth::parse_headers_from_request(request_bytes);

                // Authenticate
                match authenticator.authenticate(&headers) {
                    localup_http_auth::AuthResult::Authenticated => {
                        debug!("HTTP auth successful for tunnel: {}", localup_id);
                    }
                    localup_http_auth::AuthResult::Unauthorized(response) => {
                        debug!(
                            "HTTP auth failed for tunnel: {} (type: {})",
                            localup_id,
                            authenticator.auth_type()
                        );
                        tls_stream.write_all(&response).await?;
                        return Ok(());
                    }
                }
            }
        }

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

        // Now enter bidirectional streaming loop and capture response
        let response_capture =
            Self::proxy_transparent_stream(tls_stream, quic_send, quic_recv, stream_id).await?;

        // Save captured request/response to database
        if let Some(ref db_conn) = db {
            use base64::prelude::{Engine as _, BASE64_STANDARD as BASE64};

            let response_end = chrono::Utc::now();
            let latency_ms = (response_end - request_start).num_milliseconds() as i32;

            let captured_request = localup_relay_db::entities::captured_request::ActiveModel {
                id: Set(request_id.clone()),
                localup_id: Set(localup_id.to_string()),
                method: Set(method.clone()),
                path: Set(uri.clone()),
                host: Set(host),
                headers: Set(serde_json::to_string(&headers).unwrap_or_default()),
                body: Set(body.as_ref().map(|b| BASE64.encode(b))),
                status: Set(response_capture.status.map(|s| s as i32)),
                response_headers: Set(response_capture
                    .headers
                    .as_ref()
                    .map(|h| serde_json::to_string(h).unwrap_or_default())),
                response_body: Set(response_capture.body.as_ref().map(|b| BASE64.encode(b))),
                created_at: Set(request_start),
                responded_at: Set(Some(response_end)),
                latency_ms: Set(Some(latency_ms)),
            };

            use sea_orm::EntityTrait;
            if let Err(e) =
                localup_relay_db::entities::prelude::CapturedRequest::insert(captured_request)
                    .exec(db_conn)
                    .await
            {
                warn!(
                    "Failed to save captured HTTPS request {}: {}",
                    request_id, e
                );
            } else {
                debug!("Captured HTTPS request {} to database", request_id);
            }
        }

        Ok(())
    }

    /// Parse HTTP request into components
    fn parse_http_request(request: &str) -> (String, String, Vec<(String, String)>) {
        let mut lines = request.lines();

        // Parse request line
        let (method, uri) = if let Some(line) = lines.next() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                (parts[0].to_string(), parts[1].to_string())
            } else {
                ("GET".to_string(), "/".to_string())
            }
        } else {
            ("GET".to_string(), "/".to_string())
        };

        // Parse headers
        let mut headers = Vec::new();
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some(colon_pos) = line.find(':') {
                let name = line[..colon_pos].trim().to_string();
                let value = line[colon_pos + 1..].trim().to_string();
                headers.push((name, value));
            }
        }

        (method, uri, headers)
    }

    /// Extract Host header from HTTP request
    fn extract_host_from_request(request: &str) -> Option<String> {
        for line in request.lines() {
            if line.to_lowercase().starts_with("host:") {
                let host = line.split(':').nth(1)?.trim();
                // Remove port if present
                let host = host.split(':').next().unwrap_or(host);
                return Some(host.to_string());
            }
        }
        None
    }

    /// Bidirectional transparent streaming proxy with response capture
    async fn proxy_transparent_stream(
        mut tls_stream: tokio_rustls::server::TlsStream<TcpStream>,
        mut quic_send: localup_transport_quic::QuicSendHalf,
        mut quic_recv: localup_transport_quic::QuicRecvHalf,
        stream_id: u32,
    ) -> Result<ResponseCapture, HttpsServerError> {
        let mut client_buffer = vec![0u8; 16384];
        let mut response_buffer = Vec::new();
        let mut headers_parsed = false;
        let mut status: Option<u16> = None;
        let mut response_headers: Option<Vec<(String, String)>> = None;

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

                            // Capture response data for database (limit to first 64KB)
                            if response_buffer.len() < 65536 {
                                let remaining = 65536 - response_buffer.len();
                                let to_capture = data.len().min(remaining);
                                response_buffer.extend_from_slice(&data[..to_capture]);
                            }

                            // Parse headers from first chunk if not already done
                            if !headers_parsed {
                                if let Ok(response_str) = std::str::from_utf8(&response_buffer) {
                                    if let Some(header_end) = response_str.find("\r\n\r\n") {
                                        let header_section = &response_str[..header_end];
                                        let mut lines = header_section.lines();

                                        // Parse status line
                                        if let Some(status_line) = lines.next() {
                                            let parts: Vec<&str> = status_line.split_whitespace().collect();
                                            if parts.len() >= 2 {
                                                status = parts[1].parse().ok();
                                            }
                                        }

                                        // Parse headers
                                        let mut hdrs = Vec::new();
                                        for line in lines {
                                            if let Some(colon_pos) = line.find(':') {
                                                let name = line[..colon_pos].trim().to_string();
                                                let value = line[colon_pos + 1..].trim().to_string();
                                                hdrs.push((name, value));
                                            }
                                        }
                                        response_headers = Some(hdrs);
                                        headers_parsed = true;
                                    }
                                }
                            }

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

        // Extract body from response buffer
        let body = if let Ok(response_str) = std::str::from_utf8(&response_buffer) {
            if let Some(header_end) = response_str.find("\r\n\r\n") {
                let body_start = header_end + 4;
                if body_start < response_buffer.len() {
                    Some(response_buffer[body_start..].to_vec())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        Ok(ResponseCapture {
            status,
            headers: response_headers,
            body,
        })
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
