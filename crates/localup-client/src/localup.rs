//! Tunnel protocol implementation for client

use crate::config::{ProtocolConfig, TunnelConfig};
use crate::http_proxy::HttpProxy;
use crate::metrics::MetricsStore;
use crate::relay_discovery::RelayDiscovery;
use crate::transport_discovery::TransportDiscoverer;
use crate::TunnelError;
use localup_proto::{Endpoint, Protocol, TransportProtocol, TunnelMessage};
use localup_transport::{
    TransportConnection, TransportConnector as TransportConnectorTrait, TransportStream,
};
use localup_transport_h2::{H2Config, H2Connector};
use localup_transport_quic::{QuicConfig, QuicConnector};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error, info, warn};

/// HTTP request data for processing
struct HttpRequestData {
    method: String,
    uri: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

/// Response capture for accumulating response data in transparent streaming mode
/// Always captures headers and status code. Only captures body for text-based content.
/// Uses streaming decompression to avoid memory spikes.
/// NOTE: This is kept for potential future use but currently replaced by HttpProxy
#[allow(dead_code)]
struct ResponseCapture {
    /// HTTP response parser using httparse for proper boundary detection
    parser: crate::http_parser::HttpResponseParser,
    /// Status code (captured from first chunk)
    status: u16,
    /// Response headers (captured from first chunk)
    headers: Vec<(String, String)>,
    /// Decompressed body data (only for text content types)
    body_data: Vec<u8>,
    /// Whether we've parsed the first chunk
    first_chunk_parsed: bool,
    /// Whether we should capture body (based on Content-Type)
    should_capture_body: bool,
    /// Whether response has been finalized
    finalized: bool,
    /// Maximum body bytes to capture (512KB limit for text content)
    max_body_size: usize,
    /// Content encoding (gzip, deflate, br, etc.)
    content_encoding: Option<String>,
    /// Transfer encoding (chunked, etc.)
    transfer_encoding: Option<String>,
    /// Compressed data buffer (for gzip which needs full data to decompress)
    compressed_buffer: Vec<u8>,
    /// Buffer for chunked decoding (accumulates until we have a complete chunk)
    chunk_buffer: Vec<u8>,
    /// Are we currently reading chunk data (vs chunk size line)?
    in_chunk_data: bool,
    /// Remaining bytes in current chunk
    chunk_remaining: usize,
    /// Content-Length header value (if present)
    content_length: Option<usize>,
    /// Total body bytes received so far
    body_bytes_received: usize,
    /// Whether chunked transfer is complete (saw 0-length chunk)
    /// Note: Now tracked by HttpResponseParser, kept for decode_chunked internal state
    #[allow(dead_code)]
    chunked_complete: bool,
    /// Headers ended exactly at chunk boundary (no body in first chunk)
    /// Note: Now tracked by HttpResponseParser for completion detection
    #[allow(dead_code)]
    headers_only_in_first_chunk: bool,
}

#[allow(dead_code)]
impl ResponseCapture {
    const DEFAULT_MAX_BODY_SIZE: usize = 512 * 1024; // 512KB for text content

    fn new() -> Self {
        Self {
            parser: crate::http_parser::HttpResponseParser::new(),
            status: 0,
            headers: Vec::new(),
            body_data: Vec::new(),
            first_chunk_parsed: false,
            should_capture_body: false,
            finalized: false,
            max_body_size: Self::DEFAULT_MAX_BODY_SIZE,
            content_encoding: None,
            transfer_encoding: None,
            compressed_buffer: Vec::new(),
            chunk_buffer: Vec::new(),
            in_chunk_data: false,
            chunk_remaining: 0,
            content_length: None,
            body_bytes_received: 0,
            chunked_complete: false,
            headers_only_in_first_chunk: false,
        }
    }

    /// Check if the response is complete (all body bytes received)
    /// Uses the HttpResponseParser for proper boundary detection
    fn is_response_complete(&self) -> bool {
        // Delegate to the proper HTTP parser which handles:
        // - Content-Length based completion
        // - Chunked transfer encoding completion
        // - No-body status codes (1xx, 204, 304)
        // - Headers-only responses (no Content-Length, no body data)
        let complete = self.parser.is_complete();

        // For streaming content types, override the parser's decision
        // and wait for connection close
        if complete && self.is_streaming_content_type() {
            debug!(
                "Streaming response (status={}) - not complete until connection closes",
                self.status
            );
            return false;
        }

        debug!(
            "Response complete check: parser_complete={}, status={}, body_received={}",
            complete, self.status, self.body_bytes_received
        );
        complete
    }

    /// Check if this is a streaming content type (SSE, etc.)
    fn is_streaming_content_type(&self) -> bool {
        self.headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("content-type")
                && (value.contains("text/event-stream")
                    || value.contains("application/octet-stream"))
        })
    }

    /// Check if using chunked transfer encoding
    fn is_chunked(&self) -> bool {
        self.transfer_encoding
            .as_ref()
            .map(|te| te.contains("chunked"))
            .unwrap_or(false)
    }

    /// Check if content type is text-based (JSON, HTML, XML, text, etc.)
    fn is_text_content_type(content_type: &str) -> bool {
        let ct = content_type.to_lowercase();
        ct.contains("json")
            || ct.contains("html")
            || ct.contains("xml")
            || ct.contains("text/")
            || ct.contains("javascript")
            || ct.contains("css")
            || ct.contains("form-urlencoded")
    }

    /// Decode chunked transfer encoding and return the actual body data
    /// Returns decoded chunks ready for decompression/storage
    fn decode_chunked(&mut self, data: &[u8]) -> Vec<u8> {
        let mut decoded = Vec::new();
        let mut pos = 0;

        // Add incoming data to our buffer
        self.chunk_buffer.extend_from_slice(data);

        while pos < self.chunk_buffer.len() {
            if self.in_chunk_data {
                // Reading chunk data
                let available = self.chunk_buffer.len() - pos;
                let to_read = available.min(self.chunk_remaining);
                decoded.extend_from_slice(&self.chunk_buffer[pos..pos + to_read]);
                pos += to_read;
                self.chunk_remaining -= to_read;

                if self.chunk_remaining == 0 {
                    // Chunk complete, expect \r\n
                    self.in_chunk_data = false;
                    // Skip trailing \r\n after chunk data
                    if pos + 2 <= self.chunk_buffer.len()
                        && &self.chunk_buffer[pos..pos + 2] == b"\r\n"
                    {
                        pos += 2;
                    }
                }
            } else {
                // Reading chunk size line
                if let Some(line_end) = self.chunk_buffer[pos..]
                    .windows(2)
                    .position(|w| w == b"\r\n")
                {
                    let line = &self.chunk_buffer[pos..pos + line_end];
                    // Parse hex chunk size (may have extensions after ;)
                    let size_str = std::str::from_utf8(line)
                        .ok()
                        .and_then(|s| s.split(';').next())
                        .unwrap_or("");
                    let chunk_size = usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);

                    pos += line_end + 2; // Skip past \r\n

                    if chunk_size == 0 {
                        // Final chunk - mark as complete and we're done
                        self.chunked_complete = true;
                        break;
                    }

                    self.chunk_remaining = chunk_size;
                    self.in_chunk_data = true;
                } else {
                    // Need more data to complete the line
                    break;
                }
            }
        }

        // Remove processed data from buffer
        if pos > 0 {
            self.chunk_buffer = self.chunk_buffer[pos..].to_vec();
        }

        decoded
    }

    /// Append body data - buffers compressed data for later decompression
    fn decompress_and_append(&mut self, data: &[u8]) {
        // Check if we need to decompress
        let needs_decompression = matches!(
            self.content_encoding.as_deref(),
            Some("gzip") | Some("deflate")
        );

        if needs_decompression {
            // Buffer compressed data (will decompress in finalize)
            let remaining = self.max_body_size - self.compressed_buffer.len();
            let to_append = data.len().min(remaining);
            if to_append > 0 {
                self.compressed_buffer.extend_from_slice(&data[..to_append]);
            }
        } else {
            // No compression - append directly to body_data
            let remaining = self.max_body_size - self.body_data.len();
            let to_append = data.len().min(remaining);
            if to_append > 0 {
                self.body_data.extend_from_slice(&data[..to_append]);
            }
        }
    }

    /// Process body data: decode chunked encoding if needed, then decompress
    fn process_body_data(&mut self, data: &[u8]) {
        if self.is_chunked() {
            // Decode chunked transfer encoding first
            let decoded = self.decode_chunked(data);
            if !decoded.is_empty() {
                self.decompress_and_append(&decoded);
            }
        } else {
            // Direct body data
            self.decompress_and_append(data);
        }
    }

    fn append(&mut self, chunk: &[u8]) {
        // Feed data to the proper HTTP parser
        self.parser.feed(chunk);

        // Extract parsed headers when available (first time only)
        if !self.first_chunk_parsed {
            if let Some(parsed) = self.parser.parsed() {
                self.first_chunk_parsed = true;
                self.status = parsed.status;
                self.content_length = parsed.content_length;

                // Copy headers and track important ones
                for (name, value) in &parsed.headers {
                    // Check Content-Type to decide if we should capture body
                    if name.eq_ignore_ascii_case("content-type") {
                        self.should_capture_body = Self::is_text_content_type(value);
                    }

                    // Track Content-Encoding for decompression
                    if name.eq_ignore_ascii_case("content-encoding") {
                        self.content_encoding = Some(value.to_lowercase());
                    }

                    // Track Transfer-Encoding for chunked decoding
                    if name.eq_ignore_ascii_case("transfer-encoding") {
                        self.transfer_encoding = Some(value.to_lowercase());
                    }

                    self.headers.push((name.clone(), value.clone()));
                }

                debug!(
                    "Parsed response (httparse): status={}, content_length={:?}, chunked={}, no_body={}, headers_count={}",
                    parsed.status, parsed.content_length, parsed.is_chunked, parsed.no_body, parsed.headers.len()
                );

                // Track body bytes from first chunk
                self.body_bytes_received = self.parser.body_received();

                // Check for headers-only response (no body in first chunk)
                if parsed.no_body
                    || (self.body_bytes_received == 0
                        && parsed.content_length.is_none()
                        && !parsed.is_chunked)
                {
                    self.headers_only_in_first_chunk = true;
                    debug!(
                        "Headers-only response detected (proper parsing), status={}",
                        self.status
                    );
                }

                // Process body data for capture if needed
                if self.should_capture_body {
                    // Clone body data to avoid borrow conflict
                    let body_data = self.parser.body_data().map(|d| d.to_vec());
                    if let Some(body_data) = body_data {
                        if !body_data.is_empty() {
                            self.process_body_data(&body_data);
                        }
                    }
                }
            }
        } else {
            // Track all body bytes received (even if not capturing)
            self.body_bytes_received = self.parser.body_received();

            if self.should_capture_body && self.body_data.len() < self.max_body_size {
                // Continue capturing body chunks
                self.process_body_data(chunk);
            }
        }
    }

    fn finalize(&self) -> (u16, Vec<(String, String)>, Option<Vec<u8>>) {
        // Decompress if we have compressed data buffered
        let body = if self.should_capture_body {
            if !self.compressed_buffer.is_empty() {
                // Decompress the buffered data
                match self.content_encoding.as_deref() {
                    Some("gzip") => {
                        use flate2::read::GzDecoder;
                        use std::io::Read;
                        let mut decoder = GzDecoder::new(&self.compressed_buffer[..]);
                        let mut decompressed = Vec::new();
                        match decoder.read_to_end(&mut decompressed) {
                            Ok(_) => {
                                debug!(
                                    "Gzip decompressed {} bytes to {} bytes",
                                    self.compressed_buffer.len(),
                                    decompressed.len()
                                );
                                Some(decompressed)
                            }
                            Err(e) => {
                                debug!("Gzip decompression failed: {}", e);
                                // Return raw compressed data as fallback
                                Some(self.compressed_buffer.clone())
                            }
                        }
                    }
                    Some("deflate") => {
                        use flate2::read::DeflateDecoder;
                        use std::io::Read;
                        let mut decoder = DeflateDecoder::new(&self.compressed_buffer[..]);
                        let mut decompressed = Vec::new();
                        match decoder.read_to_end(&mut decompressed) {
                            Ok(_) => {
                                debug!(
                                    "Deflate decompressed {} bytes to {} bytes",
                                    self.compressed_buffer.len(),
                                    decompressed.len()
                                );
                                Some(decompressed)
                            }
                            Err(e) => {
                                debug!("Deflate decompression failed: {}", e);
                                Some(self.compressed_buffer.clone())
                            }
                        }
                    }
                    _ => {
                        // Shouldn't happen, but return compressed data
                        Some(self.compressed_buffer.clone())
                    }
                }
            } else if !self.body_data.is_empty() {
                // Uncompressed data
                Some(self.body_data.clone())
            } else {
                None
            }
        } else {
            None
        };

        (self.status, self.headers.clone(), body)
    }
}

/// Generate a short unique ID from stream_id (8 characters)
fn generate_short_id(stream_id: u32) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    stream_id.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:08x}", (hash as u32))
}

/// Generate a deterministic localup_id from auth token
/// This ensures the same token always gets the same localup_id (and thus same port/subdomain)
fn generate_localup_id_from_token(token: &str) -> String {
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
            localup_proto::ExitNodeConfig::Custom(addr) => {
                info!("Using custom relay: {}", addr);
                addr.clone()
            }
            localup_proto::ExitNodeConfig::Auto
            | localup_proto::ExitNodeConfig::Nearest
            | localup_proto::ExitNodeConfig::Specific(_)
            | localup_proto::ExitNodeConfig::MultiRegion(_) => {
                info!("Using automatic relay selection");

                // Initialize relay discovery
                let discovery = RelayDiscovery::new().map_err(|e| {
                    TunnelError::ConnectionError(format!(
                        "Failed to initialize relay discovery: {}",
                        e
                    ))
                })?;

                // Determine protocol for relay selection based on tunnel protocol
                let relay_protocol = match self.config.protocols.first() {
                    Some(ProtocolConfig::Http { .. }) | Some(ProtocolConfig::Https { .. }) => {
                        "https"
                    }
                    Some(ProtocolConfig::Tcp { .. }) | Some(ProtocolConfig::Tls { .. }) => "tcp",
                    None => {
                        return Err(TunnelError::ConnectionError(
                            "No protocol configured".to_string(),
                        ))
                    }
                };

                // Select relay using auto policy
                // TODO: Implement region-aware selection for Nearest, Specific, MultiRegion variants
                let relay_addr =
                    discovery
                        .select_relay(relay_protocol, None, None)
                        .map_err(|e| {
                            TunnelError::ConnectionError(format!("Failed to select relay: {}", e))
                        })?;

                info!(
                    "Auto-selected relay: {} (protocol: {})",
                    relay_addr, relay_protocol
                );
                relay_addr
            }
        };

        // Parse and resolve address (supports IP:port, hostname:port, or https://hostname:port)
        let (hostname, relay_addr) = Self::parse_relay_address(&relay_addr_str).await?;

        // Discover available transports from relay
        info!("🔍 Discovering available transports from relay...");
        let discoverer = TransportDiscoverer::new().with_insecure(true); // Insecure for localhost/dev

        let discovered = discoverer
            .discover_and_select(
                &hostname,
                relay_addr.port(),
                relay_addr,
                self.config.preferred_transport,
            )
            .await
            .unwrap_or_else(|e| {
                warn!(
                    "Transport discovery failed ({}), falling back to QUIC on port {}",
                    e,
                    relay_addr.port()
                );
                crate::transport_discovery::DiscoveredTransport {
                    protocol: TransportProtocol::Quic,
                    address: relay_addr,
                    path: None,
                    full_response: None,
                }
            });

        info!(
            "🚀 Selected transport: {:?} on {}",
            discovered.protocol, discovered.address
        );

        // Create connector based on discovered transport
        let (connection, mut control_stream) = match discovered.protocol {
            TransportProtocol::Quic => {
                info!("Connecting via QUIC...");
                let quic_config = Arc::new(QuicConfig::client_insecure());
                let quic_connector = QuicConnector::new(quic_config).map_err(|e| {
                    TunnelError::ConnectionError(format!("Failed to create QUIC connector: {}", e))
                })?;

                let conn = quic_connector
                    .connect(discovered.address, &hostname)
                    .await
                    .map_err(|e| {
                        TunnelError::ConnectionError(format!(
                            "Failed to connect via QUIC to {}: {}",
                            discovered.address, e
                        ))
                    })?;

                // Open control stream
                let stream = conn.open_stream().await.map_err(|e| {
                    TunnelError::ConnectionError(format!("Failed to open control stream: {}", e))
                })?;

                (
                    ConnectionWrapper::Quic(Arc::new(conn)),
                    StreamWrapper::Quic(stream),
                )
            }
            TransportProtocol::H2 => {
                info!("Connecting via HTTP/2...");
                let h2_config = Arc::new(H2Config::client_insecure());
                let h2_connector = H2Connector::new(h2_config).map_err(|e| {
                    TunnelError::ConnectionError(format!("Failed to create H2 connector: {}", e))
                })?;

                let conn = h2_connector
                    .connect(discovered.address, &hostname)
                    .await
                    .map_err(|e| {
                        TunnelError::ConnectionError(format!(
                            "Failed to connect via HTTP/2 to {}: {}",
                            discovered.address, e
                        ))
                    })?;

                // Open control stream
                let stream = conn.open_stream().await.map_err(|e| {
                    TunnelError::ConnectionError(format!("Failed to open control stream: {}", e))
                })?;

                (
                    ConnectionWrapper::H2(Arc::new(conn)),
                    StreamWrapper::H2(stream),
                )
            }
            TransportProtocol::WebSocket => {
                return Err(TunnelError::ConnectionError(
                    "WebSocket transport not yet implemented".to_string(),
                ));
            }
        };

        info!("✅ Connected to relay via {:?}", discovered.protocol);

        // Generate deterministic tunnel ID from auth token
        // This ensures the same token always gets the same localup_id (and thus same port/subdomain)
        let localup_id = generate_localup_id_from_token(&self.config.auth_token);
        info!("🎯 Using deterministic localup_id: {}", localup_id);

        // Convert ProtocolConfig to Protocol
        let protocols: Vec<Protocol> = self
            .config
            .protocols
            .iter()
            .map(|pc| match pc {
                ProtocolConfig::Http {
                    subdomain,
                    custom_domain,
                    ..
                } => Protocol::Http {
                    // custom_domain takes precedence over subdomain
                    // Send None if no subdomain - server will auto-generate one
                    subdomain: subdomain.clone(),
                    custom_domain: custom_domain.clone(),
                },
                ProtocolConfig::Https {
                    subdomain,
                    custom_domain,
                    ..
                } => Protocol::Https {
                    // custom_domain takes precedence over subdomain
                    // Send None if no subdomain - server will auto-generate one
                    subdomain: subdomain.clone(),
                    custom_domain: custom_domain.clone(),
                },
                ProtocolConfig::Tcp { remote_port, .. } => Protocol::Tcp {
                    // 0 means auto-allocate, specific port means request that port
                    port: remote_port.unwrap_or(0),
                },

                ProtocolConfig::Tls {
                    local_port: _,
                    sni_hostnames,
                    http_port: _,
                } => Protocol::Tls {
                    port: 8443, // TLS server port (SNI-based routing)
                    // Use all provided SNI patterns, or default to "*" if none
                    sni_patterns: if sni_hostnames.is_empty() {
                        vec!["*".to_string()]
                    } else {
                        sni_hostnames.clone()
                    },
                },
            })
            .collect();

        // Send Connect message
        let connect_msg = TunnelMessage::Connect {
            localup_id: localup_id.clone(),
            auth_token: self.config.auth_token.clone(),
            protocols: protocols.clone(),
            config: localup_proto::TunnelConfig {
                local_host: self.config.local_host.clone(),
                local_port: None,
                local_https: false,
                exit_node: self.config.exit_node.clone(),
                failover: self.config.failover,
                ip_allowlist: self.config.ip_allowlist.clone(),
                enable_compression: false,
                enable_multiplexing: true,
                http_auth: self.config.http_auth.clone(),
            },
        };

        // Control stream was already opened in the match statement above
        control_stream
            .send_message(&connect_msg)
            .await
            .map_err(|e| TunnelError::ConnectionError(format!("Failed to send Connect: {}", e)))?;

        debug!("Sent Connect message");

        // Wait for Connected response
        match control_stream.recv_message().await {
            Ok(Some(TunnelMessage::Connected {
                localup_id: tid,
                endpoints,
            })) => {
                info!("✅ Tunnel registered: {}", tid);
                for endpoint in &endpoints {
                    info!("🌍 Public URL: {}", endpoint.public_url);
                }

                // Limit concurrent connections to local server (prevents overwhelming dev servers)
                // 5 parallel connections balances performance vs overwhelming dev servers
                let connection_semaphore = Arc::new(tokio::sync::Semaphore::new(5));

                Ok(TunnelConnection {
                    _connection: connection,
                    control_stream: Arc::new(tokio::sync::Mutex::new(control_stream)),
                    shutdown_tx: Arc::new(tokio::sync::Mutex::new(None)),
                    localup_id: tid,
                    endpoints,
                    config: self.config,
                    metrics: MetricsStore::default(),
                    connection_semaphore,
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

use localup_transport_h2::{H2Connection, H2Stream};
use localup_transport_quic::{QuicConnection, QuicStream};

/// TCP stream manager to route data to active streams
type TcpStreamManager =
    Arc<tokio::sync::Mutex<std::collections::HashMap<u32, tokio::sync::mpsc::Sender<Vec<u8>>>>>;

/// Wrapper for different transport connection types
#[derive(Clone)]
enum ConnectionWrapper {
    Quic(Arc<QuicConnection>),
    H2(Arc<H2Connection>),
}

impl ConnectionWrapper {
    async fn accept_stream(
        &self,
    ) -> Result<Option<StreamWrapper>, localup_transport::TransportError> {
        match self {
            ConnectionWrapper::Quic(conn) => {
                use localup_transport::TransportConnection;
                Ok(conn.accept_stream().await?.map(StreamWrapper::Quic))
            }
            ConnectionWrapper::H2(conn) => {
                use localup_transport::TransportConnection;
                Ok(conn.accept_stream().await?.map(StreamWrapper::H2))
            }
        }
    }
}

/// Wrapper for different transport stream types
#[derive(Debug)]
enum StreamWrapper {
    Quic(QuicStream),
    H2(H2Stream),
}

#[async_trait::async_trait]
impl localup_transport::TransportStream for StreamWrapper {
    async fn send_message(
        &mut self,
        message: &TunnelMessage,
    ) -> localup_transport::TransportResult<()> {
        match self {
            StreamWrapper::Quic(stream) => stream.send_message(message).await,
            StreamWrapper::H2(stream) => stream.send_message(message).await,
        }
    }

    async fn recv_message(&mut self) -> localup_transport::TransportResult<Option<TunnelMessage>> {
        match self {
            StreamWrapper::Quic(stream) => stream.recv_message().await,
            StreamWrapper::H2(stream) => stream.recv_message().await,
        }
    }

    async fn send_bytes(&mut self, data: &[u8]) -> localup_transport::TransportResult<()> {
        match self {
            StreamWrapper::Quic(stream) => stream.send_bytes(data).await,
            StreamWrapper::H2(stream) => stream.send_bytes(data).await,
        }
    }

    async fn recv_bytes(
        &mut self,
        max_size: usize,
    ) -> localup_transport::TransportResult<bytes::Bytes> {
        match self {
            StreamWrapper::Quic(stream) => stream.recv_bytes(max_size).await,
            StreamWrapper::H2(stream) => stream.recv_bytes(max_size).await,
        }
    }

    async fn finish(&mut self) -> localup_transport::TransportResult<()> {
        match self {
            StreamWrapper::Quic(stream) => stream.finish().await,
            StreamWrapper::H2(stream) => stream.finish().await,
        }
    }

    fn stream_id(&self) -> u64 {
        match self {
            StreamWrapper::Quic(stream) => stream.stream_id(),
            StreamWrapper::H2(stream) => stream.stream_id(),
        }
    }

    fn is_closed(&self) -> bool {
        match self {
            StreamWrapper::Quic(stream) => stream.is_closed(),
            StreamWrapper::H2(stream) => stream.is_closed(),
        }
    }
}

/// Active tunnel connection
#[derive(Clone)]
pub struct TunnelConnection {
    _connection: ConnectionWrapper, // Kept alive to maintain connection
    control_stream: Arc<tokio::sync::Mutex<StreamWrapper>>,
    shutdown_tx: Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::Sender<()>>>>,
    localup_id: String,
    endpoints: Vec<Endpoint>,
    config: TunnelConfig,
    metrics: MetricsStore,
    /// Semaphore to limit concurrent connections to local server
    /// This prevents overwhelming dev servers like Next.js with too many parallel connections
    connection_semaphore: Arc<tokio::sync::Semaphore>,
}

impl TunnelConnection {
    pub fn localup_id(&self) -> &str {
        &self.localup_id
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
        let _control_stream_task = tokio::spawn(async move {
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

        // Clone semaphore for use in handlers
        let connection_semaphore = self.connection_semaphore.clone();

        // Main loop: accept streams from exit node
        loop {
            tokio::select! {
                // Accept new streams
                stream_result = connection.accept_stream() => {
                    match stream_result {
                Ok(Some(mut stream)) => {
                    debug!("Accepted new QUIC stream: {}", stream.stream_id());

                    let config_clone = config.clone();
                    let metrics_clone = metrics.clone();
                    let semaphore_clone = connection_semaphore.clone();

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
                                    semaphore_clone,
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
                                // Format remote address with port
                                let full_remote_addr = format!("{}:{}", remote_addr, remote_port);
                                Self::handle_tcp_stream(
                                    stream,
                                    &config_clone,
                                    &metrics_clone,
                                    stream_id,
                                    full_remote_addr,
                                )
                                .await;
                            }
                            Ok(Some(TunnelMessage::TlsConnect {
                                stream_id,
                                sni,
                                client_hello,
                            })) => {
                                debug!("TLS connect on stream {}: SNI={}", stream.stream_id(), sni);
                                Self::handle_tls_stream(
                                    stream,
                                    &config_clone,
                                    &metrics_clone,
                                    stream_id,
                                    client_hello,
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
                            error!("❌ Relay connection closed unexpectedly");
                            return Err(TunnelError::ConnectionError(
                                "Relay server closed connection".to_string(),
                            ));
                        }
                        Err(e) => {
                            error!("❌ Error accepting stream: {}", e);
                            return Err(TunnelError::ConnectionError(format!(
                                "Stream error: {}",
                                e
                            )));
                        }
                    }
                }
            }
        }

        // Note: control_stream_task is intentionally not awaited here because
        // we return early with errors from the loop above
        // The task will be cleaned up automatically when the connection drops
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
    /// Handle transparent HTTP stream using HTTP proxy for clean metrics
    /// Falls back to raw streaming for WebSocket upgrades
    async fn handle_http_transparent_stream(
        stream: StreamWrapper,
        config: &TunnelConfig,
        metrics: &MetricsStore,
        stream_id: u32,
        initial_data: Vec<u8>,
        _connection_semaphore: Arc<tokio::sync::Semaphore>,
    ) {
        // Extract the inner QUIC stream
        let mut stream = match stream {
            StreamWrapper::Quic(s) => s,
            StreamWrapper::H2(_) => {
                error!("HTTP transparent streaming not supported over H2 transport");
                return;
            }
        };

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

        let local_addr = format!("{}:{}", config.local_host, local_port);
        let base_stream_id = generate_short_id(stream_id);

        // Check if this is a WebSocket upgrade request
        let is_websocket = Self::is_websocket_upgrade(&initial_data);

        if is_websocket {
            // Fall back to raw streaming for WebSocket
            info!("🔌 WebSocket upgrade detected, using raw streaming");
            Self::handle_raw_http_stream(stream, &local_addr, stream_id, initial_data).await;
            return;
        }

        // Create HTTP proxy with connection pooling
        let proxy = HttpProxy::new(local_addr.clone(), metrics.clone());

        // Process initial request through proxy
        let result = proxy.forward_request(&base_stream_id, &initial_data).await;

        match result {
            Ok(proxy_result) => {
                // Check if response is a WebSocket upgrade (101 Switching Protocols)
                if proxy_result.status == 101 {
                    // This shouldn't happen since we checked above, but handle gracefully
                    warn!("Unexpected 101 response, falling back to raw streaming");
                    // Can't fall back easily here since we already consumed the request
                    // Just send the response and close
                }

                // Send response back through QUIC
                let data_msg = TunnelMessage::HttpStreamData {
                    stream_id,
                    data: proxy_result.raw_response,
                };
                if let Err(e) = stream.send_message(&data_msg).await {
                    error!("Failed to send response to tunnel: {}", e);
                    return;
                }
            }
            Err(e) => {
                error!("Proxy error for initial request: {}", e);
                // Record error in metrics
                let error_metric_id = metrics
                    .record_request(
                        base_stream_id.clone(),
                        "UNKNOWN".to_string(),
                        "/".to_string(),
                        vec![],
                        None,
                    )
                    .await;
                metrics
                    .record_error(&error_metric_id, e.to_string(), 0)
                    .await;

                let _ = stream
                    .send_message(&TunnelMessage::HttpStreamClose { stream_id })
                    .await;
                return;
            }
        }

        // Request counter for keep-alive requests
        let mut request_num: u32 = 1;

        // Handle keep-alive: read more requests from QUIC stream
        loop {
            match stream.recv_message().await {
                Ok(Some(TunnelMessage::HttpStreamData { data, .. })) => {
                    // Check if this is a new HTTP request
                    if Self::looks_like_http_request(&data) {
                        request_num += 1;
                        let req_stream_id = format!("{}-{}", base_stream_id, request_num);

                        // Check for WebSocket upgrade in subsequent requests
                        if Self::is_websocket_upgrade(&data) {
                            info!("🔌 WebSocket upgrade in keep-alive, switching to raw streaming");
                            // For WebSocket upgrade in keep-alive, we need raw bidirectional streaming
                            // Send this request through raw and continue
                            Self::handle_raw_http_stream(stream, &local_addr, stream_id, data)
                                .await;
                            return;
                        }

                        // Forward through proxy
                        match proxy.forward_request(&req_stream_id, &data).await {
                            Ok(proxy_result) => {
                                let data_msg = TunnelMessage::HttpStreamData {
                                    stream_id,
                                    data: proxy_result.raw_response,
                                };
                                if let Err(e) = stream.send_message(&data_msg).await {
                                    error!("Failed to send response to tunnel: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Proxy error for keep-alive request: {}", e);
                                // Continue trying to handle more requests
                            }
                        }
                    } else {
                        // Non-HTTP data (shouldn't happen in normal HTTP flow)
                        debug!("Received non-HTTP data on keep-alive stream, ignoring");
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

        // Send close message
        let _ = stream
            .send_message(&TunnelMessage::HttpStreamClose { stream_id })
            .await;

        info!("🔌 HTTP proxy stream {} ended", stream_id);
    }

    /// Check if HTTP request is a WebSocket upgrade
    fn is_websocket_upgrade(data: &[u8]) -> bool {
        let text = String::from_utf8_lossy(data).to_lowercase();
        text.contains("upgrade: websocket") || text.contains("connection: upgrade")
    }

    /// Handle raw HTTP stream for WebSocket/SSE (bidirectional streaming)
    async fn handle_raw_http_stream(
        stream: localup_transport_quic::QuicStream,
        local_addr: &str,
        stream_id: u32,
        initial_data: Vec<u8>,
    ) {
        // Connect to local server
        let local_socket = match TcpStream::connect(local_addr).await {
            Ok(sock) => sock,
            Err(e) => {
                error!(
                    "Failed to connect to {} for raw streaming: {}",
                    local_addr, e
                );
                return;
            }
        };

        let (mut local_read, mut local_write) = local_socket.into_split();
        let (mut quic_send, mut quic_recv) = stream.split();

        // Write initial data
        if let Err(e) = local_write.write_all(&initial_data).await {
            error!("Failed to write initial data: {}", e);
            return;
        }

        // Bidirectional streaming
        let local_to_quic = tokio::spawn(async move {
            let mut buffer = vec![0u8; 16384];
            loop {
                match local_read.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let msg = TunnelMessage::HttpStreamData {
                            stream_id,
                            data: buffer[..n].to_vec(),
                        };
                        if quic_send.send_message(&msg).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = quic_send
                .send_message(&TunnelMessage::HttpStreamClose { stream_id })
                .await;
        });

        let quic_to_local = tokio::spawn(async move {
            loop {
                match quic_recv.recv_message().await {
                    Ok(Some(TunnelMessage::HttpStreamData { data, .. })) => {
                        if local_write.write_all(&data).await.is_err() {
                            break;
                        }
                    }
                    Ok(Some(TunnelMessage::HttpStreamClose { .. })) | Ok(None) | Err(_) => break,
                    _ => {}
                }
            }
        });

        let _ = tokio::join!(local_to_quic, quic_to_local);
        info!("🔌 Raw HTTP stream {} ended", stream_id);
    }

    /// Check if data looks like the start of an HTTP request
    fn looks_like_http_request(data: &[u8]) -> bool {
        if data.len() < 4 {
            return false;
        }
        // Check for common HTTP methods at the start
        data.starts_with(b"GET ")
            || data.starts_with(b"POST ")
            || data.starts_with(b"PUT ")
            || data.starts_with(b"DELETE ")
            || data.starts_with(b"PATCH ")
            || data.starts_with(b"HEAD ")
            || data.starts_with(b"OPTIONS ")
            || data.starts_with(b"CONNECT ")
            || data.starts_with(b"TRACE ")
    }

    /// Parse HTTP request line, headers, and body from raw bytes
    /// NOTE: Kept for potential future use but currently unused with HttpProxy
    #[allow(dead_code)]
    fn parse_http_request(data: &[u8]) -> HttpRequestData {
        // Helper to find subsequence in byte slice
        fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
            haystack
                .windows(needle.len())
                .position(|window| window == needle)
        }
        let text = String::from_utf8_lossy(data);
        let mut lines = text.lines();

        // Parse request line: METHOD URI HTTP/1.x
        let (method, uri) = if let Some(request_line) = lines.next() {
            let parts: Vec<&str> = request_line.split_whitespace().collect();
            if parts.len() >= 2 {
                (parts[0].to_string(), parts[1].to_string())
            } else {
                ("UNKNOWN".to_string(), "/".to_string())
            }
        } else {
            ("UNKNOWN".to_string(), "/".to_string())
        };

        // Parse headers
        let mut headers = Vec::new();
        for line in lines {
            if line.is_empty() {
                break; // End of headers
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.push((name.trim().to_string(), value.trim().to_string()));
            }
        }

        // Extract body (everything after \r\n\r\n)
        let body = if let Some(header_end) = find_subsequence(data, b"\r\n\r\n") {
            let body_start = header_end + 4;
            if body_start < data.len() {
                Some(data[body_start..].to_vec())
            } else {
                None
            }
        } else {
            None
        };

        HttpRequestData {
            method,
            uri,
            headers,
            body,
        }
    }

    async fn handle_tcp_stream(
        stream: StreamWrapper,
        config: &TunnelConfig,
        metrics: &MetricsStore,
        stream_id: u32,
        remote_addr: String,
    ) {
        // Extract the inner QUIC stream
        let mut stream = match stream {
            StreamWrapper::Quic(s) => s,
            StreamWrapper::H2(_) => {
                error!("TCP streaming not supported over H2 transport");
                return;
            }
        };
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

        // Record TCP connection in metrics
        let stream_id_str = generate_short_id(stream_id);
        let connection_id = metrics
            .record_tcp_connection(
                stream_id_str.clone(),
                remote_addr.clone(),
                local_addr.clone(),
            )
            .await;

        // Split BOTH streams for true bidirectional communication WITHOUT MUTEXES!
        let (mut local_read, mut local_write) = local_socket.into_split();
        let (mut quic_send, mut quic_recv) = stream.split();

        // Shared byte counters for metrics
        let bytes_sent = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let bytes_received = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let bytes_sent_clone = bytes_sent.clone();
        let bytes_received_clone = bytes_received.clone();

        // Periodic metrics update task - updates every second for real-time UI
        let bytes_sent_metrics = bytes_sent.clone();
        let bytes_received_metrics = bytes_received.clone();
        let metrics_clone = metrics.clone();
        let connection_id_clone = connection_id.clone();
        let metrics_update_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            interval.tick().await; // Skip first immediate tick

            loop {
                interval.tick().await;

                let current_bytes_sent =
                    bytes_sent_metrics.load(std::sync::atomic::Ordering::SeqCst);
                let current_bytes_received =
                    bytes_received_metrics.load(std::sync::atomic::Ordering::SeqCst);

                metrics_clone
                    .update_tcp_connection(
                        &connection_id_clone,
                        current_bytes_received,
                        current_bytes_sent,
                    )
                    .await;
            }
        });

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
                        bytes_sent_clone.fetch_add(n as u64, std::sync::atomic::Ordering::SeqCst);
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
                        bytes_received_clone
                            .fetch_add(data.len() as u64, std::sync::atomic::Ordering::SeqCst);
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

        // Wait for both data transfer tasks
        let _ = tokio::join!(local_to_quic, quic_to_local);

        // Stop the periodic metrics update task
        metrics_update_task.abort();

        // Finalize TCP connection metrics
        let final_bytes_sent = bytes_sent.load(std::sync::atomic::Ordering::SeqCst);
        let final_bytes_received = bytes_received.load(std::sync::atomic::Ordering::SeqCst);
        metrics
            .update_tcp_connection(&connection_id, final_bytes_received, final_bytes_sent)
            .await;
        metrics.close_tcp_connection(&connection_id, None).await;

        debug!(
            "TCP stream handler finished (stream {}): sent={}, received={}",
            stream_id, final_bytes_sent, final_bytes_received
        );
    }

    /// Handle a TLS connection on a dedicated QUIC stream
    async fn handle_tls_stream(
        stream: StreamWrapper,
        config: &TunnelConfig,
        _metrics: &MetricsStore,
        stream_id: u32,
        client_hello: Vec<u8>,
    ) {
        // Extract the inner QUIC stream
        let mut stream = match stream {
            StreamWrapper::Quic(s) => s,
            StreamWrapper::H2(_) => {
                error!("TLS streaming not supported over H2 transport");
                return;
            }
        };

        // Get TLS protocol config
        let tls_config = config.protocols.first().and_then(|p| match p {
            ProtocolConfig::Tls {
                local_port,
                http_port,
                ..
            } => Some((*local_port, *http_port)),
            _ => None,
        });

        let (tls_port, http_port) = match tls_config {
            Some(config) => config,
            None => {
                error!("No TLS protocol configured");
                let _ = stream
                    .send_message(&TunnelMessage::TlsClose { stream_id })
                    .await;
                return;
            }
        };

        // Detect if this is HTTP traffic (not TLS)
        // TLS ClientHello starts with 0x16 (handshake) followed by version bytes
        // HTTP requests start with method names like "GET ", "POST ", "PUT ", etc.
        let is_http = !client_hello.is_empty()
            && (client_hello.starts_with(b"GET ")
                || client_hello.starts_with(b"POST ")
                || client_hello.starts_with(b"PUT ")
                || client_hello.starts_with(b"DELETE ")
                || client_hello.starts_with(b"HEAD ")
                || client_hello.starts_with(b"OPTIONS ")
                || client_hello.starts_with(b"PATCH ")
                || client_hello.starts_with(b"CONNECT "));

        // Choose the appropriate port
        let local_port = if is_http {
            http_port.unwrap_or(tls_port)
        } else {
            tls_port
        };

        if is_http {
            debug!(
                "Detected HTTP traffic on TLS stream {}, routing to port {}",
                stream_id, local_port
            );
        }

        // Connect to local service
        let local_addr = format!("{}:{}", config.local_host, local_port);
        let local_socket = match TcpStream::connect(&local_addr).await {
            Ok(sock) => sock,
            Err(e) => {
                error!(
                    "Failed to connect to local {} service at {}: {}",
                    if is_http { "HTTP" } else { "TLS" },
                    local_addr,
                    e
                );
                let _ = stream
                    .send_message(&TunnelMessage::TlsClose { stream_id })
                    .await;
                return;
            }
        };

        debug!(
            "Connected to local {} service at {}",
            if is_http { "HTTP" } else { "TLS" },
            local_addr
        );

        // Split both streams for bidirectional communication WITHOUT MUTEXES
        let (mut local_read, mut local_write) = local_socket.into_split();
        let (mut quic_send, mut quic_recv) = stream.split();

        // Task to read from local TLS and send to QUIC stream
        // Now owns quic_send exclusively - no mutex needed!
        let local_to_quic = tokio::spawn(async move {
            let mut buffer = vec![0u8; 8192];
            loop {
                match local_read.read(&mut buffer).await {
                    Ok(0) => {
                        // Local socket closed
                        debug!("Local TLS socket closed (stream {})", stream_id);
                        let _ = quic_send
                            .send_message(&TunnelMessage::TlsClose { stream_id })
                            .await;
                        let _ = quic_send.finish().await;
                        break;
                    }
                    Ok(n) => {
                        debug!("Read {} bytes from local TLS (stream {})", n, stream_id);
                        let data_msg = TunnelMessage::TlsData {
                            stream_id,
                            data: buffer[..n].to_vec(),
                        };
                        if let Err(e) = quic_send.send_message(&data_msg).await {
                            error!("Failed to send TlsData on QUIC stream: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error reading from local TLS: {}", e);
                        break;
                    }
                }
            }
        });

        // Task to read from QUIC stream and send to local TLS
        // Now owns quic_recv exclusively - no mutex needed!
        let quic_to_local = tokio::spawn(async move {
            // First, send the initial client_hello to the local TLS service
            if let Err(e) = local_write.write_all(&client_hello).await {
                error!("Failed to send ClientHello to local TLS service: {}", e);
                return;
            }
            debug!(
                "Sent {} bytes (ClientHello) to local TLS service (stream {})",
                client_hello.len(),
                stream_id
            );

            // Now handle bidirectional TLS data forwarding
            loop {
                let msg = quic_recv.recv_message().await;

                match msg {
                    Ok(Some(TunnelMessage::TlsData { stream_id: _, data })) => {
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
                            error!("Failed to write to local TLS service: {}", e);
                            break;
                        }
                        if let Err(e) = local_write.flush().await {
                            error!("Failed to flush local TLS: {}", e);
                            break;
                        }
                    }
                    Ok(Some(TunnelMessage::TlsClose { stream_id: _ })) => {
                        debug!("Received TlsClose from QUIC stream (stream {})", stream_id);
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
                        warn!("Unexpected message on TLS stream: {:?}", msg);
                    }
                }
            }
        });

        // Wait for both tasks
        let _ = tokio::join!(local_to_quic, quic_to_local);
        debug!("TLS stream handler finished (stream {})", stream_id);
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
        let mut temp_buf = vec![0u8; 65536]; // 64KB buffer for better performance with large responses

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
            let mut chunked_data = response_buf[header_end_pos..].to_vec();

            // Keep reading until connection closes or end marker
            // Use a reasonable timeout per read - 5 seconds should handle most cases
            loop {
                let read_result = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    local_socket.read(&mut temp_buf),
                )
                .await;

                match read_result {
                    Ok(Ok(0)) => {
                        // Connection closed
                        debug!(
                            "Chunked response: connection closed after {} bytes",
                            chunked_data.len()
                        );
                        break;
                    }
                    Ok(Ok(n)) => {
                        chunked_data.extend_from_slice(&temp_buf[..n]);

                        // Check for chunked encoding end marker
                        // Look for "\r\n0\r\n\r\n" or just "0\r\n\r\n" at the end
                        if chunked_data.len() >= 5
                            && (chunked_data.ends_with(b"0\r\n\r\n")
                                || chunked_data.ends_with(b"\r\n0\r\n\r\n"))
                        {
                            debug!(
                                "Chunked response: found end marker after {} bytes",
                                chunked_data.len()
                            );
                            break;
                        }
                    }
                    Ok(Err(e)) => {
                        warn!("Error reading chunked body: {}", e);
                        break;
                    }
                    Err(_) => {
                        // Timeout after 5 seconds of no data
                        warn!(
                            "Chunked response: read timeout after 5s ({} bytes received so far)",
                            chunked_data.len()
                        );
                        break;
                    }
                }
            }

            // Decode chunked transfer encoding to get raw body
            // This removes chunk markers (size\r\n...data...\r\n) and extracts the actual content
            let body_data = Self::decode_chunked_body(&chunked_data);
            debug!(
                "Decoded chunked body: {} bytes -> {} bytes",
                chunked_data.len(),
                body_data.len()
            );

            if body_data.is_empty() {
                None
            } else {
                Some(body_data)
            }
        } else {
            // No Content-Length and not chunked - read until connection closes
            let mut body_data = response_buf[header_end_pos..].to_vec();

            // Use reasonable timeout - 5 seconds between reads
            loop {
                let read_result = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
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
                        // Timeout after 5 seconds of no data
                        warn!(
                            "Response read timeout after 5s ({} bytes received)",
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

    /// Decode chunked transfer encoding body
    /// Parses format: SIZE\r\n...DATA...\r\n...SIZE\r\n...DATA...\r\n0\r\n\r\n
    fn decode_chunked_body(chunked_data: &[u8]) -> Vec<u8> {
        let mut decoded = Vec::new();
        let mut pos = 0;

        while pos < chunked_data.len() {
            // Find the chunk size line (ends with \r\n)
            let size_end = match chunked_data[pos..].windows(2).position(|w| w == b"\r\n") {
                Some(p) => pos + p,
                None => break,
            };

            // Parse chunk size (hex)
            let size_str = match std::str::from_utf8(&chunked_data[pos..size_end]) {
                Ok(s) => s.split(';').next().unwrap_or("").trim(), // Handle chunk extensions
                Err(_) => break,
            };

            let chunk_size = match usize::from_str_radix(size_str, 16) {
                Ok(0) => break, // Final chunk
                Ok(size) => size,
                Err(_) => break,
            };

            // Move past the size line
            pos = size_end + 2;

            // Extract chunk data
            if pos + chunk_size <= chunked_data.len() {
                decoded.extend_from_slice(&chunked_data[pos..pos + chunk_size]);
                pos += chunk_size;
            } else {
                // Incomplete chunk - take what we have
                decoded.extend_from_slice(&chunked_data[pos..]);
                break;
            }

            // Skip trailing \r\n after chunk data
            if pos + 2 <= chunked_data.len() && &chunked_data[pos..pos + 2] == b"\r\n" {
                pos += 2;
            }
        }

        decoded
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
