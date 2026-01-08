//! HTTP Reverse Proxy for local server forwarding
//!
//! Uses hyper with connection pooling to forward HTTP requests to the local server.
//! This provides:
//! - Proper HTTP parsing (request/response boundaries)
//! - Connection pooling (reuses TCP connections)
//! - Clean metrics capture at HTTP layer
//! - HTTP/1.1 keep-alive support

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::client::conn::http1;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use crate::metrics::MetricsStore;

/// Maximum number of pooled connections per target
const MAX_POOL_SIZE: usize = 10;

/// Connection pool entry
struct PooledConnection {
    sender: http1::SendRequest<Full<Bytes>>,
    #[allow(dead_code)]
    created_at: Instant,
}

/// HTTP Proxy with connection pooling
pub struct HttpProxy {
    /// Target address (host:port)
    target: String,
    /// Connection pool
    pool: Arc<Mutex<Vec<PooledConnection>>>,
    /// Metrics store for recording request/response data
    metrics: MetricsStore,
}

/// Result of proxying a request
pub struct ProxyResult {
    /// Metric ID for this request
    pub metric_id: String,
    /// HTTP status code
    pub status: u16,
    /// Response headers
    pub headers: Vec<(String, String)>,
    /// Response body (if text content)
    pub body: Option<Vec<u8>>,
    /// Request duration in milliseconds
    pub duration_ms: u64,
    /// Raw response bytes to forward
    pub raw_response: Vec<u8>,
}

impl HttpProxy {
    /// Create a new HTTP proxy for the given target
    pub fn new(target: String, metrics: MetricsStore) -> Self {
        Self {
            target,
            pool: Arc::new(Mutex::new(Vec::with_capacity(MAX_POOL_SIZE))),
            metrics,
        }
    }

    /// Get or create a connection to the target
    async fn get_connection(&self) -> Result<http1::SendRequest<Full<Bytes>>, ProxyError> {
        // Try to get a connection from the pool
        {
            let mut pool = self.pool.lock().await;
            while let Some(conn) = pool.pop() {
                // Check if connection is still usable
                if conn.sender.is_ready() {
                    debug!("Reusing pooled connection to {}", self.target);
                    return Ok(conn.sender);
                }
                debug!("Discarding stale connection from pool");
            }
        }

        // Create a new connection
        debug!("Creating new connection to {}", self.target);
        let stream = TcpStream::connect(&self.target).await.map_err(|e| {
            ProxyError::ConnectionFailed(format!("Failed to connect to {}: {}", self.target, e))
        })?;

        let io = TokioIo::new(stream);

        let (sender, conn) = http1::handshake(io)
            .await
            .map_err(|e| ProxyError::ConnectionFailed(format!("HTTP handshake failed: {}", e)))?;

        // Spawn connection driver
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                debug!("Connection closed: {}", e);
            }
        });

        Ok(sender)
    }

    /// Return a connection to the pool
    async fn return_connection(&self, sender: http1::SendRequest<Full<Bytes>>) {
        if !sender.is_ready() {
            debug!("Not returning closed connection to pool");
            return;
        }

        let mut pool = self.pool.lock().await;
        if pool.len() < MAX_POOL_SIZE {
            pool.push(PooledConnection {
                sender,
                created_at: Instant::now(),
            });
            debug!("Returned connection to pool (size: {})", pool.len());
        }
    }

    /// Parse raw HTTP request bytes into a hyper Request
    fn parse_request(data: &[u8]) -> Result<(Request<Full<Bytes>>, usize), ProxyError> {
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);

        match req.parse(data) {
            Ok(httparse::Status::Complete(header_len)) => {
                let method = req.method.unwrap_or("GET");
                let path = req.path.unwrap_or("/");

                // Build hyper request
                let mut builder = Request::builder().method(method).uri(path);

                for header in req.headers.iter() {
                    builder = builder.header(header.name, header.value);
                }

                // Get body if present
                let body_data = if header_len < data.len() {
                    Bytes::copy_from_slice(&data[header_len..])
                } else {
                    Bytes::new()
                };

                let request = builder.body(Full::new(body_data)).map_err(|e| {
                    ProxyError::InvalidRequest(format!("Failed to build request: {}", e))
                })?;

                Ok((request, header_len))
            }
            Ok(httparse::Status::Partial) => Err(ProxyError::InvalidRequest(
                "Incomplete HTTP request".to_string(),
            )),
            Err(e) => Err(ProxyError::InvalidRequest(format!(
                "Invalid HTTP request: {}",
                e
            ))),
        }
    }

    /// Serialize a hyper Response to raw HTTP bytes
    async fn serialize_response(
        response: Response<Incoming>,
    ) -> Result<(u16, Vec<(String, String)>, Vec<u8>, Option<Vec<u8>>), ProxyError> {
        let status = response.status().as_u16();
        let status_text = response.status().canonical_reason().unwrap_or("OK");

        // Collect headers, filtering out transfer-encoding since we'll use content-length
        let mut headers: Vec<(String, String)> = Vec::new();
        let mut is_text_content = false;

        for (name, value) in response.headers() {
            let name_str = name.to_string();
            let value_str = value.to_str().unwrap_or("").to_string();

            // Skip transfer-encoding - we collect the full body so we'll use content-length
            if name_str.eq_ignore_ascii_case("transfer-encoding") {
                continue;
            }
            // Skip content-length - we'll add our own after collecting body
            if name_str.eq_ignore_ascii_case("content-length") {
                continue;
            }

            if name_str.eq_ignore_ascii_case("content-type") {
                is_text_content = is_text_content_type(&value_str);
            }

            headers.push((name_str, value_str));
        }

        // Collect body first so we know the length
        let body_bytes = response
            .into_body()
            .collect()
            .await
            .map_err(|e| ProxyError::ResponseError(format!("Failed to read response body: {}", e)))?
            .to_bytes();

        // Add content-length header with actual body size
        headers.push(("content-length".to_string(), body_bytes.len().to_string()));

        // Build raw response
        let mut raw = Vec::new();

        // Status line
        raw.extend_from_slice(format!("HTTP/1.1 {} {}\r\n", status, status_text).as_bytes());

        // Headers
        for (name, value) in &headers {
            raw.extend_from_slice(format!("{}: {}\r\n", name, value).as_bytes());
        }
        raw.extend_from_slice(b"\r\n");

        // Body
        raw.extend_from_slice(&body_bytes);

        // Only capture body for text content types (for metrics)
        let captured_body = if is_text_content && body_bytes.len() <= 512 * 1024 {
            Some(body_bytes.to_vec())
        } else {
            None
        };

        Ok((status, headers, raw, captured_body))
    }

    /// Forward an HTTP request to the local server and return the response
    pub async fn forward_request(
        &self,
        stream_id: &str,
        data: &[u8],
    ) -> Result<ProxyResult, ProxyError> {
        let start_time = Instant::now();

        // Parse the request
        let (request, header_len) = Self::parse_request(data)?;

        let method = request.method().to_string();
        let uri = request.uri().to_string();
        let req_headers: Vec<(String, String)> = request
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        // Extract request body for metrics (only capture text/json content up to 512KB)
        let request_body = if header_len < data.len() {
            let body_data = &data[header_len..];
            let content_type = req_headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.as_str())
                .unwrap_or("");
            let is_text = is_text_content_type(content_type);
            if is_text && body_data.len() <= 512 * 1024 {
                Some(body_data.to_vec())
            } else {
                None
            }
        } else {
            None
        };

        // Record the request in metrics
        let metric_id = self
            .metrics
            .record_request(
                stream_id.to_string(),
                method.clone(),
                uri.clone(),
                req_headers,
                request_body,
            )
            .await;

        info!("📤 Proxying {} {} to {}", method, uri, self.target);

        // Get a connection and send the request
        let mut sender = self.get_connection().await?;

        let response = sender
            .send_request(request)
            .await
            .map_err(|e| ProxyError::RequestFailed(format!("Failed to send request: {}", e)))?;

        // Return connection to pool if still usable
        self.return_connection(sender).await;

        // Serialize response
        let (status, headers, raw_response, body) = Self::serialize_response(response).await?;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        info!(
            "📥 Response {} {} -> {} ({}ms)",
            method, uri, status, duration_ms
        );

        // Record response in metrics
        self.metrics
            .record_response(
                &metric_id,
                status,
                headers.clone(),
                body.clone(),
                duration_ms,
            )
            .await;

        Ok(ProxyResult {
            metric_id,
            status,
            headers,
            body,
            duration_ms,
            raw_response,
        })
    }

    /// Forward request and handle errors gracefully
    pub async fn forward_request_safe(
        &self,
        stream_id: &str,
        data: &[u8],
    ) -> (Vec<u8>, Option<String>) {
        match self.forward_request(stream_id, data).await {
            Ok(result) => (result.raw_response, Some(result.metric_id)),
            Err(e) => {
                error!("Proxy error: {}", e);

                // Generate error response
                let error_response = format!(
                    "HTTP/1.1 502 Bad Gateway\r\n\
                     Content-Type: text/plain\r\n\
                     Content-Length: {}\r\n\
                     \r\n\
                     {}",
                    e.to_string().len(),
                    e
                );

                (error_response.into_bytes(), None)
            }
        }
    }
}

/// Check if content type is text-based
fn is_text_content_type(content_type: &str) -> bool {
    let ct = content_type.to_lowercase();
    ct.contains("json")
        || ct.contains("html")
        || ct.contains("xml")
        || ct.contains("text/")
        || ct.contains("javascript")
        || ct.contains("css")
}

/// Proxy errors
#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Response error: {0}")]
    ResponseError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_get() {
        let request = b"GET /api/test HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let (req, header_len) = HttpProxy::parse_request(request).unwrap();

        assert_eq!(req.method(), "GET");
        assert_eq!(req.uri(), "/api/test");
        assert_eq!(header_len, request.len());
    }

    #[test]
    fn test_parse_post_with_body() {
        let request = b"POST /api/data HTTP/1.1\r\nHost: localhost\r\nContent-Length: 13\r\n\r\n{\"key\":\"val\"}";
        let (req, header_len) = HttpProxy::parse_request(request).unwrap();

        assert_eq!(req.method(), "POST");
        assert_eq!(req.uri(), "/api/data");
        assert!(header_len < request.len());
    }

    #[test]
    fn test_is_text_content_type() {
        assert!(is_text_content_type("application/json"));
        assert!(is_text_content_type("text/html; charset=utf-8"));
        assert!(is_text_content_type("application/javascript"));
        assert!(!is_text_content_type("image/png"));
        assert!(!is_text_content_type("application/octet-stream"));
    }
}
