//! HTTPS server implementation with TLS termination
//!
//! Supports wildcard domain certificates (e.g., `*.example.com`) with fallback resolution.
//! Supports both HTTP/1.1 and HTTP/2 via ALPN negotiation.
use bytes::Bytes;
use h2::server::SendResponse;
use http::Request;
use localup_control::{PendingRequests, TunnelConnectionManager};
use localup_proto::TunnelMessage;
use localup_relay_db::entities::custom_domain;
use localup_router::{extract_parent_wildcard, RouteKey, RouteRegistry};
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

    #[error("HTTP/2 error: {0}")]
    H2Error(#[from] h2::Error),

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
            // 1. Try exact domain match first
            if let Some(cert) = certs.get(domain) {
                info!("Using custom certificate for domain: {}", domain);
                return Some(cert.clone());
            }

            // 2. Try wildcard fallback: api.example.com -> *.example.com
            if let Some(wildcard_pattern) = extract_parent_wildcard(domain) {
                if let Some(cert) = certs.get(&wildcard_pattern) {
                    info!(
                        "Using wildcard certificate {} for domain: {}",
                        wildcard_pattern, domain
                    );
                    return Some(cert.clone());
                }
            }
        }

        // 3. Fall back to default certificate
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

    /// Load TLS certificates from PEM string content
    fn load_certs_from_pem(
        pem_content: &str,
    ) -> Result<Vec<CertificateDer<'static>>, HttpsServerError> {
        let mut reader = BufReader::new(pem_content.as_bytes());

        rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                HttpsServerError::TlsError(format!("Failed to parse certs from PEM: {}", e))
            })
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

    /// Load private key from PEM string content
    fn load_private_key_from_pem(
        pem_content: &str,
    ) -> Result<PrivateKeyDer<'static>, HttpsServerError> {
        let mut reader = BufReader::new(pem_content.as_bytes());

        rustls_pemfile::private_key(&mut reader)
            .map_err(|e| {
                HttpsServerError::TlsError(format!("Failed to parse key from PEM: {}", e))
            })?
            .ok_or_else(|| {
                HttpsServerError::TlsError("No private key found in PEM content".to_string())
            })
    }

    /// Load custom domain certificates from database
    /// Prefers loading from cert_pem/key_pem content stored directly in database,
    /// falls back to cert_path/key_path filesystem loading if content not available
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
            // Try loading from database content first (preferred)
            if let (Some(cert_pem), Some(key_pem)) = (&domain.cert_pem, &domain.key_pem) {
                match Self::load_domain_cert_from_pem(cert_pem, key_pem) {
                    Ok(cert_key) => {
                        info!(
                            "Loaded certificate for domain {} from database content",
                            domain.domain
                        );
                        resolver
                            .add_custom_cert(domain.domain.clone(), Arc::new(cert_key))
                            .await;
                        loaded_count += 1;
                        continue;
                    }
                    Err(e) => {
                        warn!(
                            "Failed to load certificate for domain {} from database content: {}, trying file path",
                            domain.domain, e
                        );
                    }
                }
            }

            // Fall back to loading from file paths
            let cert_path = match &domain.cert_path {
                Some(path) => path,
                None => {
                    warn!(
                        "Domain {} has no cert_pem or cert_path, skipping",
                        domain.domain
                    );
                    continue;
                }
            };
            let key_path = match &domain.key_path {
                Some(path) => path,
                None => {
                    warn!(
                        "Domain {} has no key_pem or key_path, skipping",
                        domain.domain
                    );
                    continue;
                }
            };

            // Load certificate and key from filesystem
            match Self::load_domain_cert(cert_path, key_path) {
                Ok(cert_key) => {
                    info!(
                        "Loaded certificate for domain {} from filesystem",
                        domain.domain
                    );
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

    /// Load a single domain's certificate and key from PEM content strings
    /// This is used when loading certificates stored directly in the database.
    pub fn load_domain_cert_from_pem(
        cert_pem: &str,
        key_pem: &str,
    ) -> Result<CertifiedKey, HttpsServerError> {
        let certs = Self::load_certs_from_pem(cert_pem)?;
        let key = Self::load_private_key_from_pem(key_pem)?;

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

        // Build TLS config with custom resolver and ALPN for HTTP/2 support
        let mut tls_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(cert_resolver);

        // Configure ALPN protocols: prefer HTTP/2, fallback to HTTP/1.1
        // HTTP/2 is used for regular requests (multiplexing, header compression)
        // WebSocket works via:
        //   - HTTP/1.1: standard Upgrade mechanism (RFC 6455)
        //   - HTTP/2: Extended CONNECT protocol (RFC 8441) with :protocol pseudo-header
        tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

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

        // Check negotiated ALPN protocol
        let alpn_protocol = tls_stream.get_ref().1.alpn_protocol();
        let is_h2 = alpn_protocol == Some(b"h2".as_slice());

        if is_h2 {
            debug!("HTTP/2 connection from {} via ALPN", peer_addr);
            return Self::handle_h2_connection(
                tls_stream,
                peer_addr,
                route_registry,
                localup_manager,
                pending_requests,
                db,
            )
            .await;
        }

        debug!(
            "HTTP/1.1 connection from {} (ALPN: {:?})",
            peer_addr,
            alpn_protocol.map(|p| String::from_utf8_lossy(p))
        );

        // HTTP/1.1 path: Read HTTP request
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

        // Parse request path from request line
        let request_path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");

        // Handle ACME HTTP-01 challenges BEFORE route lookup
        // Note: ACME challenges typically come over HTTP (port 80), not HTTPS,
        // but we handle it here too for completeness
        if request_path.starts_with("/.well-known/acme-challenge/") {
            let token = request_path
                .strip_prefix("/.well-known/acme-challenge/")
                .unwrap_or("");

            if !token.is_empty() {
                if let Some(ref db_conn) = db {
                    match Self::lookup_acme_challenge(db_conn, host, token).await {
                        Ok(Some(key_auth)) => {
                            info!(
                                "ACME HTTP-01 challenge response for domain {} token {}",
                                host, token
                            );
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                                key_auth.len(),
                                key_auth
                            );
                            tls_stream.write_all(response.as_bytes()).await?;
                            return Ok(());
                        }
                        Ok(None) => {
                            debug!(
                                "ACME challenge not found for domain {} token {}, continuing to route lookup",
                                host, token
                            );
                            // Don't return - fall through to normal routing
                        }
                        Err(e) => {
                            error!("Database error looking up ACME challenge: {}", e);
                            // Don't return - fall through to normal routing
                        }
                    }
                }
                // If no database or challenge not found, continue to route lookup
            }
        }

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

        // Check IP filtering
        if !target.is_ip_allowed(&peer_addr) {
            warn!(
                "Connection f   rom IP {} denied by IP filter for host: {} (allowed: {:?})",
                peer_addr.ip(),
                host,
                target.ip_filter
            );
            let response = b"HTTP/1.1 403 Forbidden\r\nContent-Length: 13\r\n\r\nAccess denied";
            tls_stream.write_all(response).await?;
            return Ok(());
        }

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
                let auth_headers = localup_http_auth::parse_headers_from_request(request_bytes);

                // Authenticate
                match authenticator.authenticate(&auth_headers) {
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

        // Open a new QUIC stream
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

        // Use transparent streaming for ALL requests (data in, data out)
        // This handles WebSocket, SSE, chunked transfers, large files, and
        // regular HTTP correctly without buffering the entire response body.
        debug!(
            "HTTPS request for tunnel: {} {} {} (transparent streaming)",
            localup_id, method, uri
        );

        let (mut quic_send, quic_recv) = stream.split();

        let connect_msg = TunnelMessage::HttpStreamConnect {
            stream_id,
            host: localup_id.to_string(),
            initial_data: request_bytes.to_vec(),
        };

        if let Err(e) = quic_send.send_message(&connect_msg).await {
            error!("Failed to send stream connect: {}", e);
            let response = b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 12\r\n\r\nTunnel error";
            tls_stream.write_all(response).await?;
            return Ok(());
        }

        // Bidirectional transparent streaming
        let response_capture =
            Self::proxy_transparent_stream(tls_stream, quic_send, quic_recv, stream_id).await?;

        // Save to database
        if let Some(ref db_conn) = db {
            use base64::prelude::{Engine as _, BASE64_STANDARD as BASE64};

            let response_end = chrono::Utc::now();
            let latency_ms = (response_end - request_start).num_milliseconds() as i32;

            let captured_request = localup_relay_db::entities::captured_request::ActiveModel {
                id: Set(request_id.clone()),
                localup_id: Set(localup_id.to_string()),
                method: Set(method),
                path: Set(uri),
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
                warn!("Failed to save captured request {}: {}", request_id, e);
            } else {
                debug!("Captured request {} to database", request_id);
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

    /// Handle an HTTP/2 connection
    /// Accepts multiple streams and forwards each request through the tunnel
    async fn handle_h2_connection(
        tls_stream: tokio_rustls::server::TlsStream<TcpStream>,
        peer_addr: SocketAddr,
        route_registry: Arc<RouteRegistry>,
        localup_manager: Option<Arc<TunnelConnectionManager>>,
        pending_requests: Option<Arc<PendingRequests>>,
        db: Option<DatabaseConnection>,
    ) -> Result<(), HttpsServerError> {
        // Perform HTTP/2 handshake with Extended CONNECT support (RFC 8441)
        // This enables WebSocket-over-HTTP/2 via the :protocol pseudo-header
        let mut h2_conn = match h2::server::Builder::new()
            .enable_connect_protocol()
            .handshake(tls_stream)
            .await
        {
            Ok(conn) => conn,
            Err(e) => {
                warn!("HTTP/2 handshake failed from {}: {}", peer_addr, e);
                return Err(HttpsServerError::TlsError(format!(
                    "H2 handshake failed: {}",
                    e
                )));
            }
        };

        info!(
            "HTTP/2 connection established from {} (Extended CONNECT enabled)",
            peer_addr
        );

        // Accept streams in a loop
        while let Some(result) = h2_conn.accept().await {
            match result {
                Ok((request, send_response)) => {
                    let registry = route_registry.clone();
                    let manager = localup_manager.clone();
                    let pending = pending_requests.clone();
                    let db = db.clone();

                    // Handle each HTTP/2 stream concurrently
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_h2_stream(
                            request,
                            send_response,
                            peer_addr,
                            registry,
                            manager,
                            pending,
                            db,
                        )
                        .await
                        {
                            debug!("HTTP/2 stream error from {}: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    if e.is_go_away() || e.is_io() {
                        debug!("HTTP/2 connection closed from {}: {}", peer_addr, e);
                        break;
                    }
                    warn!("HTTP/2 accept error from {}: {}", peer_addr, e);
                }
            }
        }

        debug!("HTTP/2 connection ended from {}", peer_addr);
        Ok(())
    }

    /// Handle a single HTTP/2 stream (request/response pair, or WebSocket via Extended CONNECT)
    async fn handle_h2_stream(
        request: Request<h2::RecvStream>,
        mut send_response: SendResponse<Bytes>,
        peer_addr: SocketAddr,
        route_registry: Arc<RouteRegistry>,
        localup_manager: Option<Arc<TunnelConnectionManager>>,
        _pending_requests: Option<Arc<PendingRequests>>,
        db: Option<DatabaseConnection>,
    ) -> Result<(), HttpsServerError> {
        let request_start = chrono::Utc::now();
        let request_id = uuid::Uuid::new_v4().to_string();

        // Check if this is a WebSocket Extended CONNECT request (RFC 8441)
        // Browser sends: CONNECT with :protocol=websocket, :path=/ws, :scheme=https
        let is_websocket_connect = request.method() == http::Method::CONNECT
            && request
                .extensions()
                .get::<h2::ext::Protocol>()
                .map(|p| p.as_str().eq_ignore_ascii_case("websocket"))
                .unwrap_or(false);

        // Extract request info from HTTP/2 pseudo-headers
        let method = request.method().to_string();
        let uri = request.uri().to_string();
        let authority = request
            .uri()
            .authority()
            .map(|a| a.to_string())
            .or_else(|| {
                request
                    .headers()
                    .get("host")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();

        // Extract host without port
        let host = authority.split(':').next().unwrap_or(&authority);

        debug!(
            "HTTP/2 request from {}: {} {} (host: {}, websocket: {})",
            peer_addr, method, uri, host, is_websocket_connect
        );

        // Convert headers to Vec<(String, String)>
        let headers: Vec<(String, String)> = request
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        // Route lookup (common for both regular and WebSocket requests)
        let route_key = RouteKey::HttpHost(host.to_string());
        let target = match route_registry.lookup(&route_key) {
            Ok(t) => t,
            Err(_) => {
                warn!("No HTTPS route found for host: {}", host);
                let response = http::Response::builder().status(404).body(()).unwrap();
                let mut send = send_response.send_response(response, false)?;
                send.send_data(Bytes::from("Not Found"), true)?;
                return Ok(());
            }
        };

        // Check IP filtering
        if !target.is_ip_allowed(&peer_addr) {
            warn!(
                "HTTP/2 connection from IP {} denied for host: {}",
                peer_addr.ip(),
                host
            );
            let response = http::Response::builder().status(403).body(()).unwrap();
            let mut send = send_response.send_response(response, false)?;
            send.send_data(Bytes::from("Access denied"), true)?;
            return Ok(());
        }

        // Check if this is a tunnel route
        if !target.target_addr.starts_with("tunnel:") {
            warn!("HTTPS route is not a tunnel: {}", target.target_addr);
            let response = http::Response::builder().status(502).body(()).unwrap();
            let mut send = send_response.send_response(response, false)?;
            send.send_data(Bytes::from("Bad Gateway"), true)?;
            return Ok(());
        }

        // Extract tunnel ID
        let localup_id = target.target_addr.strip_prefix("tunnel:").unwrap();

        // Get tunnel manager
        let localup_manager = match localup_manager {
            Some(m) => m,
            None => {
                error!("Tunnel manager not configured for HTTPS");
                let response = http::Response::builder().status(503).body(()).unwrap();
                let mut send = send_response.send_response(response, false)?;
                send.send_data(Bytes::from("Service Unavailable"), true)?;
                return Ok(());
            }
        };

        // Check HTTP authentication if configured
        if let Some(authenticator) = localup_manager.get_http_authenticator(localup_id).await {
            if authenticator.requires_auth() {
                let auth_headers: Vec<(String, String)> = headers
                    .iter()
                    .map(|(k, v)| (k.to_lowercase(), v.clone()))
                    .collect();

                match authenticator.authenticate(&auth_headers) {
                    localup_http_auth::AuthResult::Authenticated => {
                        debug!("HTTP auth successful for tunnel: {}", localup_id);
                    }
                    localup_http_auth::AuthResult::Unauthorized(_) => {
                        debug!("HTTP auth failed for tunnel: {}", localup_id);
                        let www_auth = match authenticator.auth_type() {
                            "basic" => "Basic",
                            "bearer" => "Bearer",
                            other => other,
                        };
                        let response = http::Response::builder()
                            .status(401)
                            .header("WWW-Authenticate", www_auth)
                            .body(())
                            .unwrap();
                        let mut send = send_response.send_response(response, false)?;
                        send.send_data(Bytes::from("Unauthorized"), true)?;
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
                let response = http::Response::builder().status(502).body(()).unwrap();
                let mut send = send_response.send_response(response, false)?;
                send.send_data(Bytes::from("Tunnel not found"), true)?;
                return Ok(());
            }
        };

        // Open QUIC stream to tunnel
        let stream_id = rand::random::<u32>();
        let stream = match connection.open_stream().await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to open QUIC stream: {}", e);
                let response = http::Response::builder().status(502).body(()).unwrap();
                let mut send = send_response.send_response(response, false)?;
                send.send_data(Bytes::from("Tunnel stream error"), true)?;
                return Ok(());
            }
        };

        let (mut quic_send, mut quic_recv) = stream.split();

        // =====================================================================
        // WebSocket Extended CONNECT path (RFC 8441)
        // =====================================================================
        if is_websocket_connect {
            debug!(
                "WebSocket Extended CONNECT for tunnel: {} {} (stream {})",
                localup_id, uri, stream_id
            );

            // Build an HTTP/1.1 WebSocket upgrade request to send through the tunnel
            // The tunnel client expects raw HTTP/1.1 bytes for WebSocket handling
            let path = request
                .uri()
                .path_and_query()
                .map(|pq| pq.to_string())
                .unwrap_or_else(|| "/".to_string());

            let mut raw_request = format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n",
                path, host
            );

            // Forward relevant headers (sec-websocket-*, origin, etc.)
            for (name, value) in &headers {
                let name_lower = name.to_lowercase();
                // Skip h2-specific and already-added headers
                if name_lower == "host" || name_lower == "upgrade" || name_lower == "connection" {
                    continue;
                }
                raw_request.push_str(&format!("{}: {}\r\n", name, value));
            }
            raw_request.push_str("\r\n");

            // Send as HttpStreamConnect (same as HTTP/1.1 WebSocket path)
            let connect_msg = TunnelMessage::HttpStreamConnect {
                stream_id,
                host: localup_id.to_string(),
                initial_data: raw_request.into_bytes(),
            };

            if let Err(e) = quic_send.send_message(&connect_msg).await {
                error!("Failed to send WebSocket stream connect: {}", e);
                let response = http::Response::builder().status(502).body(()).unwrap();
                let mut send = send_response.send_response(response, false)?;
                send.send_data(Bytes::from("Tunnel error"), true)?;
                return Ok(());
            }

            // Wait for the 101 response from the tunnel (the local server's upgrade response)
            let first_response =
                tokio::time::timeout(std::time::Duration::from_secs(30), quic_recv.recv_message())
                    .await;

            match first_response {
                Ok(Ok(Some(TunnelMessage::HttpStreamData { data, .. }))) => {
                    // Parse the HTTP/1.1 response to check for 101
                    let response_str = String::from_utf8_lossy(&data);
                    let is_upgrade = response_str.contains("101");

                    if !is_upgrade {
                        // Not an upgrade response - forward as error
                        warn!(
                            "WebSocket upgrade failed, got: {}",
                            response_str.lines().next().unwrap_or("")
                        );
                        let response = http::Response::builder().status(502).body(()).unwrap();
                        let mut send = send_response.send_response(response, false)?;
                        send.send_data(Bytes::from("WebSocket upgrade failed"), true)?;
                        return Ok(());
                    }

                    debug!("WebSocket upgrade successful, starting bidirectional streaming");

                    // Send 200 OK to the h2 client (RFC 8441: use 200, not 101)
                    // The browser treats this as a successful WebSocket connection
                    let mut response_builder = http::Response::builder().status(200);

                    // Parse and forward response headers from the 101
                    if let Some(header_end) = response_str.find("\r\n\r\n") {
                        let header_section = &response_str[..header_end];
                        for line in header_section.lines().skip(1) {
                            if let Some(colon_pos) = line.find(':') {
                                let name = line[..colon_pos].trim().to_lowercase();
                                let value = line[colon_pos + 1..].trim();
                                // Skip hop-by-hop headers forbidden in h2
                                if name == "connection"
                                    || name == "upgrade"
                                    || name == "keep-alive"
                                    || name == "transfer-encoding"
                                {
                                    continue;
                                }
                                response_builder = response_builder.header(&name, value);
                            }
                        }
                    }

                    let response = response_builder.body(()).unwrap();
                    let mut h2_send = send_response.send_response(response, false)?;

                    // Get the h2 RecvStream for reading client data
                    let mut h2_recv = request.into_body();

                    // Bidirectional streaming: h2 stream <-> QUIC tunnel
                    loop {
                        tokio::select! {
                            // Browser → Tunnel (h2 RecvStream → QUIC)
                            chunk = h2_recv.data() => {
                                match chunk {
                                    Some(Ok(data)) => {
                                        let len = data.len();
                                        // Release h2 flow control
                                        let _ = h2_recv.flow_control().release_capacity(len);

                                        let data_msg = TunnelMessage::HttpStreamData {
                                            stream_id,
                                            data: data.to_vec(),
                                        };
                                        if let Err(e) = quic_send.send_message(&data_msg).await {
                                            debug!("QUIC send error in WebSocket h2 proxy: {}", e);
                                            break;
                                        }
                                    }
                                    Some(Err(e)) => {
                                        debug!("h2 recv error in WebSocket proxy: {}", e);
                                        let _ = quic_send.send_message(
                                            &TunnelMessage::HttpStreamClose { stream_id }
                                        ).await;
                                        break;
                                    }
                                    None => {
                                        // Client closed the stream (END_STREAM)
                                        debug!("h2 WebSocket stream closed by client");
                                        let _ = quic_send.send_message(
                                            &TunnelMessage::HttpStreamClose { stream_id }
                                        ).await;
                                        break;
                                    }
                                }
                            }

                            // Tunnel → Browser (QUIC → h2 SendStream)
                            result = quic_recv.recv_message() => {
                                match result {
                                    Ok(Some(TunnelMessage::HttpStreamData { data, .. })) => {
                                        if let Err(e) = h2_send.send_data(Bytes::from(data), false) {
                                            debug!("h2 send error in WebSocket proxy: {}", e);
                                            break;
                                        }
                                    }
                                    Ok(Some(TunnelMessage::HttpStreamClose { .. })) => {
                                        debug!("Tunnel closed WebSocket stream {}", stream_id);
                                        // Send END_STREAM to close the h2 stream
                                        let _ = h2_send.send_data(Bytes::new(), true);
                                        break;
                                    }
                                    Ok(None) => {
                                        debug!("QUIC stream ended for WebSocket {}", stream_id);
                                        let _ = h2_send.send_data(Bytes::new(), true);
                                        break;
                                    }
                                    Err(e) => {
                                        debug!("QUIC recv error in WebSocket proxy: {}", e);
                                        break;
                                    }
                                    _ => {
                                        warn!("Unexpected message on WebSocket h2 stream");
                                    }
                                }
                            }
                        }
                    }

                    debug!("WebSocket h2 proxy ended (stream {})", stream_id);

                    // Capture to database
                    if let Some(ref db_conn) = db {
                        let response_end = chrono::Utc::now();
                        let latency_ms = (response_end - request_start).num_milliseconds() as i32;

                        let captured_request =
                            localup_relay_db::entities::captured_request::ActiveModel {
                                id: Set(request_id.clone()),
                                localup_id: Set(localup_id.to_string()),
                                method: Set("WEBSOCKET".to_string()),
                                path: Set(uri),
                                host: Set(Some(host.to_string())),
                                headers: Set(serde_json::to_string(&headers).unwrap_or_default()),
                                body: Set(None),
                                status: Set(Some(101)),
                                response_headers: Set(None),
                                response_body: Set(None),
                                created_at: Set(request_start),
                                responded_at: Set(Some(response_end)),
                                latency_ms: Set(Some(latency_ms)),
                            };

                        use sea_orm::EntityTrait;
                        if let Err(e) =
                            localup_relay_db::entities::prelude::CapturedRequest::insert(
                                captured_request,
                            )
                            .exec(db_conn)
                            .await
                        {
                            warn!("Failed to save WebSocket request {}: {}", request_id, e);
                        }
                    }

                    return Ok(());
                }
                Ok(Ok(Some(TunnelMessage::HttpStreamClose { .. }))) | Ok(Ok(None)) => {
                    warn!("Tunnel closed before WebSocket upgrade completed");
                    let response = http::Response::builder().status(502).body(()).unwrap();
                    let mut send = send_response.send_response(response, false)?;
                    send.send_data(Bytes::from("Tunnel closed"), true)?;
                    return Ok(());
                }
                Ok(Err(e)) => {
                    error!("Tunnel error during WebSocket upgrade: {}", e);
                    let response = http::Response::builder().status(502).body(()).unwrap();
                    let mut send = send_response.send_response(response, false)?;
                    send.send_data(Bytes::from("Tunnel error"), true)?;
                    return Ok(());
                }
                Err(_) => {
                    error!("WebSocket upgrade timeout after 30s");
                    let response = http::Response::builder().status(504).body(()).unwrap();
                    let mut send = send_response.send_response(response, false)?;
                    send.send_data(Bytes::from("Gateway Timeout"), true)?;
                    return Ok(());
                }
                _ => {
                    warn!("Unexpected message during WebSocket upgrade");
                    let response = http::Response::builder().status(502).body(()).unwrap();
                    let mut send = send_response.send_response(response, false)?;
                    send.send_data(Bytes::from("Unexpected response"), true)?;
                    return Ok(());
                }
            }
        }

        // =====================================================================
        // Regular HTTP/2 request-response path
        // =====================================================================

        // Extract path+query BEFORE consuming the request with into_body().
        // HTTP/2 URIs include the full form (https://vt.tunnel.kfs.es/path) but the
        // tunnel client builds an HTTP/1.1 request for the local server which expects
        // origin-form (/path). Sending the full URI causes local servers to return 400.
        let request_path = request
            .uri()
            .path_and_query()
            .map(|pq| pq.to_string())
            .unwrap_or_else(|| "/".to_string());

        // Read request body
        let mut body_stream = request.into_body();
        let mut body_bytes = Vec::new();
        while let Some(chunk) = body_stream.data().await {
            match chunk {
                Ok(data) => {
                    body_bytes.extend_from_slice(&data);
                    let _ = body_stream.flow_control().release_capacity(data.len());
                }
                Err(e) => {
                    warn!("Error reading HTTP/2 request body: {}", e);
                    break;
                }
            }
        }
        let body = if body_bytes.is_empty() {
            None
        } else {
            Some(body_bytes)
        };

        // Send HTTP request through tunnel
        let http_request = TunnelMessage::HttpRequest {
            stream_id,
            method: method.clone(),
            uri: request_path,
            headers: headers.clone(),
            body: body.clone(),
        };

        if let Err(e) = quic_send.send_message(&http_request).await {
            error!("Failed to send HTTP/2 request to tunnel: {}", e);
            let response = http::Response::builder().status(502).body(()).unwrap();
            let mut send = send_response.send_response(response, false)?;
            send.send_data(Bytes::from("Tunnel send error"), true)?;
            return Ok(());
        }

        debug!(
            "HTTP/2 request sent to tunnel client (stream {})",
            stream_id
        );

        // Wait for response from tunnel
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
                // Build HTTP/2 response
                let mut response_builder = http::Response::builder().status(status);

                // Add response headers (skip pseudo-headers and connection-specific headers)
                for (name, value) in &resp_headers {
                    let name_lower = name.to_lowercase();
                    if name_lower == "connection"
                        || name_lower == "keep-alive"
                        || name_lower == "transfer-encoding"
                        || name_lower == "upgrade"
                    {
                        continue;
                    }
                    response_builder = response_builder.header(name, value);
                }

                let has_body = resp_body.is_some() && !resp_body.as_ref().unwrap().is_empty();
                let response = response_builder.body(()).unwrap();
                let mut send = send_response.send_response(response, !has_body)?;

                if let Some(body_data) = resp_body.as_ref() {
                    if !body_data.is_empty() {
                        send.send_data(Bytes::copy_from_slice(body_data), true)?;
                    }
                }

                debug!("HTTP/2 response forwarded to client: {}", status);

                // Capture to database
                if let Some(ref db_conn) = db {
                    use base64::prelude::{Engine as _, BASE64_STANDARD as BASE64};

                    let response_end = chrono::Utc::now();
                    let latency_ms = (response_end - request_start).num_milliseconds() as i32;

                    let captured_request =
                        localup_relay_db::entities::captured_request::ActiveModel {
                            id: Set(request_id.clone()),
                            localup_id: Set(localup_id.to_string()),
                            method: Set(method),
                            path: Set(uri),
                            host: Set(Some(host.to_string())),
                            headers: Set(serde_json::to_string(&headers).unwrap_or_default()),
                            body: Set(body.as_ref().map(|b| BASE64.encode(b))),
                            status: Set(Some(status as i32)),
                            response_headers: Set(Some(
                                serde_json::to_string(&resp_headers).unwrap_or_default(),
                            )),
                            response_body: Set(resp_body.as_ref().map(|b| BASE64.encode(b))),
                            created_at: Set(request_start),
                            responded_at: Set(Some(response_end)),
                            latency_ms: Set(Some(latency_ms)),
                        };

                    use sea_orm::EntityTrait;
                    if let Err(e) = localup_relay_db::entities::prelude::CapturedRequest::insert(
                        captured_request,
                    )
                    .exec(db_conn)
                    .await
                    {
                        warn!(
                            "Failed to save captured HTTP/2 request {}: {}",
                            request_id, e
                        );
                    }
                }
            }
            Ok(Ok(Some(other))) => {
                warn!("Unexpected tunnel message: {:?}", other);
                let response = http::Response::builder().status(502).body(()).unwrap();
                let mut send = send_response.send_response(response, false)?;
                send.send_data(Bytes::from("Unexpected tunnel response"), true)?;
            }
            Ok(Ok(None)) => {
                warn!("Tunnel stream closed without response");
                let response = http::Response::builder().status(502).body(()).unwrap();
                let mut send = send_response.send_response(response, false)?;
                send.send_data(Bytes::from("Tunnel closed"), true)?;
            }
            Ok(Err(e)) => {
                error!("Tunnel receive error: {}", e);
                let response = http::Response::builder().status(502).body(()).unwrap();
                let mut send = send_response.send_response(response, false)?;
                send.send_data(Bytes::from("Tunnel error"), true)?;
            }
            Err(_) => {
                error!("Tunnel response timeout after 30s");
                let response = http::Response::builder().status(504).body(()).unwrap();
                let mut send = send_response.send_response(response, false)?;
                send.send_data(Bytes::from("Gateway Timeout"), true)?;
            }
        }

        Ok(())
    }

    /// Look up an ACME HTTP-01 challenge from the database
    /// Returns the key authorization if found, None if not found
    async fn lookup_acme_challenge(
        db: &DatabaseConnection,
        domain: &str,
        token: &str,
    ) -> Result<Option<String>, sea_orm::DbErr> {
        use localup_relay_db::entities::domain_challenge::{
            self, ChallengeStatus, ChallengeType, Entity as DomainChallenge,
        };
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        // Look up pending HTTP-01 challenge by domain and token
        let challenge = DomainChallenge::find()
            .filter(domain_challenge::Column::Domain.eq(domain))
            .filter(domain_challenge::Column::TokenOrRecordName.eq(token))
            .filter(domain_challenge::Column::ChallengeType.eq(ChallengeType::Http01))
            .filter(domain_challenge::Column::Status.eq(ChallengeStatus::Pending))
            .one(db)
            .await?;

        Ok(challenge.and_then(|c| c.key_auth_or_record_value))
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
