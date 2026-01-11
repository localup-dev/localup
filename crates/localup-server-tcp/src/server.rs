//! TCP server implementation

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use localup_control::{PendingRequests, TunnelConnectionManager};
use localup_proto::TunnelMessage;
use localup_router::{RouteKey, RouteRegistry};
use localup_transport::TransportConnection;
use sea_orm::DatabaseConnection;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn}; // For open_stream() method

/// TCP server errors
#[derive(Debug, Error)]
pub enum TcpServerError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to bind to {address}: {reason}\n\nTroubleshooting:\n  • Check if another process is using this port: lsof -i :{port}\n  • Try using a different address or port")]
    BindError {
        address: String,
        port: u16,
        reason: String,
    },
}

/// TCP server configuration
#[derive(Debug, Clone)]
pub struct TcpServerConfig {
    pub bind_addr: SocketAddr,
}

/// Captured response data from transparent proxy
struct ResponseCapture {
    status: Option<u16>,
    headers: Option<Vec<(String, String)>>,
    body: Option<Vec<u8>>,
}

impl Default for TcpServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:0".parse().unwrap(),
        }
    }
}

/// TCP tunnel server
pub struct TcpServer {
    config: TcpServerConfig,
    registry: Arc<RouteRegistry>,
    localup_manager: Option<Arc<TunnelConnectionManager>>,
    pending_requests: Arc<PendingRequests>,
    db: Option<DatabaseConnection>,
}

impl TcpServer {
    pub fn new(config: TcpServerConfig, registry: Arc<RouteRegistry>) -> Self {
        Self {
            config,
            registry,
            localup_manager: None,
            pending_requests: Arc::new(PendingRequests::new()),
            db: None,
        }
    }

    pub fn with_localup_manager(mut self, manager: Arc<TunnelConnectionManager>) -> Self {
        self.localup_manager = Some(manager);
        self
    }

    pub fn with_pending_requests(mut self, pending_requests: Arc<PendingRequests>) -> Self {
        self.pending_requests = pending_requests;
        self
    }

    pub fn with_database(mut self, db: DatabaseConnection) -> Self {
        self.db = Some(db);
        self
    }

    /// Start the TCP server
    pub async fn start(&self) -> Result<(), TcpServerError> {
        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| {
                let port = self.config.bind_addr.port();
                let address = self.config.bind_addr.ip().to_string();
                let reason = e.to_string();
                TcpServerError::BindError {
                    address,
                    port,
                    reason,
                }
            })?;
        let local_addr = listener.local_addr()?;

        info!("TCP server listening on {}", local_addr);

        loop {
            match listener.accept().await {
                Ok((socket, peer_addr)) => {
                    debug!("Accepted TCP connection from {}", peer_addr);
                    let registry = self.registry.clone();
                    let localup_manager = self.localup_manager.clone();
                    let pending_requests = self.pending_requests.clone();
                    let db = self.db.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_http_connection(
                            socket,
                            peer_addr,
                            registry,
                            localup_manager,
                            pending_requests,
                            db,
                        )
                        .await
                        {
                            error!("Failed to handle connection from {}: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Handle HTTP connection with routing
    async fn handle_http_connection(
        mut client_socket: TcpStream,
        peer_addr: SocketAddr,
        registry: Arc<RouteRegistry>,
        localup_manager: Option<Arc<TunnelConnectionManager>>,
        pending_requests: Arc<PendingRequests>,
        db: Option<DatabaseConnection>,
    ) -> Result<(), TcpServerError> {
        // Read HTTP request to extract Host header
        let mut buffer = vec![0u8; 4096];
        let n = client_socket.read(&mut buffer).await?;

        if n == 0 {
            return Ok(());
        }

        let request = String::from_utf8_lossy(&buffer[..n]);

        // Extract Host header
        let host = Self::extract_host_from_request(&request);

        if host.is_none() {
            warn!("No Host header found in request");
            let response =
                b"HTTP/1.1 400 Bad Request\r\nContent-Length: 16\r\n\r\nNo Host header\n";
            client_socket.write_all(response).await?;
            return Ok(());
        }

        let host = host.unwrap();
        debug!("Routing HTTP request for host: {}", host);

        // Parse request path from request line
        let request_path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");

        // Handle ACME HTTP-01 challenges BEFORE route lookup
        // This allows responding to challenges for domains that don't have tunnels yet
        if request_path.starts_with("/.well-known/acme-challenge/") {
            let token = request_path
                .strip_prefix("/.well-known/acme-challenge/")
                .unwrap_or("");

            if !token.is_empty() {
                if let Some(ref db_conn) = db {
                    match Self::lookup_acme_challenge(db_conn, &host, token).await {
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
                            client_socket.write_all(response.as_bytes()).await?;
                            return Ok(());
                        }
                        Ok(None) => {
                            debug!(
                                "ACME challenge not found for domain {} token {}, continuing to route lookup",
                                host, token
                            );
                            // Don't return - fall through to normal routing
                            // The tunnel might handle the challenge itself
                        }
                        Err(e) => {
                            error!("Database error looking up ACME challenge: {}", e);
                            // Don't return - fall through to normal routing
                        }
                    }
                }
                // If no database or challenge not found, continue to route lookup
                // This allows the tunnel to handle the challenge if it wants to
            }
        }

        // Look up route
        let route_key = RouteKey::HttpHost(host.to_string());
        let target = registry.lookup(&route_key);

        if target.is_err() {
            warn!("No route found for host: {}", host);
            let response = b"HTTP/1.1 404 Not Found\r\nContent-Length: 16\r\n\r\nRoute not found\n";
            client_socket.write_all(response).await?;
            return Ok(());
        }

        let target = target.unwrap();

        // Check IP filtering
        if !target.is_ip_allowed(&peer_addr) {
            warn!(
                "Connection from {} denied by IP filter for host: {}",
                peer_addr, host
            );
            let body = format!("Access denied for IP: {}\n", peer_addr.ip());
            let response = format!(
                "HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            client_socket.write_all(response.as_bytes()).await?;
            return Ok(());
        }

        debug!("Proxying to: {}", target.target_addr);

        // Check if this is a tunnel route
        if target.target_addr.starts_with("tunnel:") {
            // Extract tunnel ID
            let localup_id = target.target_addr.strip_prefix("tunnel:").unwrap();

            if let Some(ref manager) = localup_manager {
                // Forward through tunnel
                return Self::handle_localup_request(
                    client_socket,
                    manager.clone(),
                    pending_requests,
                    localup_id,
                    &request,
                    &buffer[..n],
                    db,
                )
                .await;
            } else {
                error!("Tunnel route found but no tunnel manager configured");
                let response = b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 23\r\n\r\nTunnel not configured\n";
                client_socket.write_all(response).await?;
                return Ok(());
            }
        }

        // Direct TCP proxy (for non-tunnel routes)
        let mut target_socket = TcpStream::connect(&target.target_addr).await?;

        // Forward the original request
        target_socket.write_all(&buffer[..n]).await?;

        // Bidirectional proxy: stream data in both directions until one side closes
        match tokio::io::copy_bidirectional(&mut client_socket, &mut target_socket).await {
            Ok((client_to_target, target_to_client)) => {
                debug!(
                    "Proxy complete: {} bytes to target, {} bytes from target",
                    client_to_target + n as u64,
                    target_to_client
                );
            }
            Err(e) => {
                debug!("Proxy connection closed: {}", e);
            }
        }

        Ok(())
    }

    /// Handle HTTP request through tunnel using multi-stream QUIC
    async fn handle_localup_request(
        mut client_socket: TcpStream,
        localup_manager: Arc<TunnelConnectionManager>,
        _pending_requests: Arc<PendingRequests>, // Not needed with multi-stream
        localup_id: &str,
        request: &str,
        request_bytes: &[u8],
        db: Option<DatabaseConnection>,
    ) -> Result<(), TcpServerError> {
        debug!("Forwarding request through tunnel: {}", localup_id);

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
                        client_socket.write_all(&response).await?;
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
                client_socket.write_all(response).await?;
                return Ok(());
            }
        };

        // Generate unique request ID and stream ID
        let request_id = uuid::Uuid::new_v4().to_string();
        let stream_id = rand::random::<u32>();
        let request_start = chrono::Utc::now();

        // Parse HTTP request
        let (method, uri, headers) = Self::parse_http_request(request);

        // Extract body (if any)
        let body = if let Some(body_start) = request.find("\r\n\r\n") {
            let body_offset = body_start + 4;
            if body_offset < request_bytes.len() {
                Some(request_bytes[body_offset..].to_vec())
            } else {
                None
            }
        } else {
            None
        };

        // Open a new QUIC stream for this HTTP request
        let stream = match connection.open_stream().await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to open QUIC stream: {}", e);
                let response =
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 23\r\n\r\nTunnel stream error\n";
                client_socket.write_all(response).await?;
                return Ok(());
            }
        };

        // Split stream for bidirectional communication without mutexes
        let (mut quic_send, quic_recv) = stream.split();

        // Use transparent streaming - send raw HTTP request bytes through tunnel
        // This preserves all headers including Content-Length and Transfer-Encoding
        let connect_msg = TunnelMessage::HttpStreamConnect {
            stream_id,
            host: localup_id.to_string(),
            initial_data: request_bytes.to_vec(),
        };

        if let Err(e) = quic_send.send_message(&connect_msg).await {
            error!("Failed to send stream connect: {}", e);
            let response = b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 12\r\n\r\nTunnel error";
            client_socket.write_all(response).await?;
            return Ok(());
        }

        debug!(
            "HTTP transparent stream initiated for tunnel {} (stream {})",
            localup_id, stream_id
        );

        // Bidirectional transparent streaming - passes bytes through unchanged
        let response_capture =
            Self::proxy_transparent_stream(client_socket, quic_send, quic_recv, stream_id).await?;

        // Save to database (metrics capture)
        if let Some(ref db_conn) = db {
            let response_end = chrono::Utc::now();
            let latency_ms = (response_end - request_start).num_milliseconds() as i32;

            let captured_request = localup_relay_db::entities::captured_request::ActiveModel {
                id: sea_orm::Set(request_id.clone()),
                localup_id: sea_orm::Set(localup_id.to_string()),
                method: sea_orm::Set(method.clone()),
                path: sea_orm::Set(uri.clone()),
                host: sea_orm::Set(Self::extract_host_from_request(request)),
                headers: sea_orm::Set(serde_json::to_string(&headers).unwrap_or_default()),
                body: sea_orm::Set(body.as_ref().map(|b| BASE64.encode(b))),
                status: sea_orm::Set(response_capture.status.map(|s| s as i32)),
                response_headers: sea_orm::Set(
                    response_capture
                        .headers
                        .as_ref()
                        .map(|h| serde_json::to_string(h).unwrap_or_default()),
                ),
                response_body: sea_orm::Set(
                    response_capture.body.as_ref().map(|b| BASE64.encode(b)),
                ),
                created_at: sea_orm::Set(request_start),
                responded_at: sea_orm::Set(Some(response_end)),
                latency_ms: sea_orm::Set(Some(latency_ms)),
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
        mut client_socket: TcpStream,
        mut quic_send: localup_transport_quic::QuicSendHalf,
        mut quic_recv: localup_transport_quic::QuicRecvHalf,
        stream_id: u32,
    ) -> Result<ResponseCapture, TcpServerError> {
        let mut client_buffer = vec![0u8; 16384];
        let mut response_buffer = Vec::new();
        let mut headers_parsed = false;
        let mut status: Option<u16> = None;
        let mut response_headers: Option<Vec<(String, String)>> = None;

        loop {
            tokio::select! {
                // Client → Tunnel
                result = client_socket.read(&mut client_buffer) => {
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

                            if let Err(e) = client_socket.write_all(&data).await {
                                warn!("Failed to write to client: {}", e);
                                break;
                            }
                            if let Err(e) = client_socket.flush().await {
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
        let _ = client_socket.shutdown().await;

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
    fn test_tcp_server_config() {
        let config = TcpServerConfig::default();
        assert_eq!(config.bind_addr.to_string(), "0.0.0.0:0");
    }
}
