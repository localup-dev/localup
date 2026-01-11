//! Proper HTTP/1.x request and response parsing using httparse.
//!
//! This module provides accurate detection of request/response boundaries,
//! replacing heuristic-based parsing with proper protocol parsing.

use tracing::debug;

/// Maximum number of headers to parse
const MAX_HEADERS: usize = 100;

/// Result of parsing an HTTP request
#[derive(Debug, Clone)]
pub struct ParsedRequest {
    /// HTTP method (GET, POST, etc.)
    pub method: String,
    /// Request path/URI
    pub path: String,
    /// HTTP version
    pub version: u8,
    /// Request headers
    pub headers: Vec<(String, String)>,
    /// Total bytes consumed by the request line and headers (including \r\n\r\n)
    pub header_len: usize,
    /// Expected body length (from Content-Length), None if no body or chunked
    pub content_length: Option<usize>,
    /// Whether using chunked transfer encoding
    pub is_chunked: bool,
}

/// Result of parsing an HTTP response
#[derive(Debug, Clone)]
pub struct ParsedResponse {
    /// HTTP status code
    pub status: u16,
    /// HTTP version
    pub version: u8,
    /// Reason phrase
    pub reason: String,
    /// Response headers
    pub headers: Vec<(String, String)>,
    /// Total bytes consumed by the status line and headers (including \r\n\r\n)
    pub header_len: usize,
    /// Expected body length (from Content-Length), None if unknown
    pub content_length: Option<usize>,
    /// Whether using chunked transfer encoding
    pub is_chunked: bool,
    /// Whether this response has no body (1xx, 204, 304)
    pub no_body: bool,
}

/// HTTP request parser state
#[derive(Debug)]
pub struct HttpRequestParser {
    /// Buffer for accumulating request data
    buffer: Vec<u8>,
    /// Parsed request (once headers are complete)
    parsed: Option<ParsedRequest>,
    /// Body bytes received so far
    body_received: usize,
    /// Whether the request is fully received
    complete: bool,
    /// Chunked decoder state
    chunked_state: ChunkedState,
}

/// HTTP response parser state
#[derive(Debug)]
pub struct HttpResponseParser {
    /// Buffer for accumulating response data
    buffer: Vec<u8>,
    /// Parsed response (once headers are complete)
    parsed: Option<ParsedResponse>,
    /// Body bytes received so far
    body_received: usize,
    /// Whether the response is fully received
    complete: bool,
    /// Chunked decoder state
    chunked_state: ChunkedState,
    /// Timestamp when data was last received (for idle timeout)
    last_data_time: Option<std::time::Instant>,
    /// Whether this response has unknown length (no Content-Length, not chunked)
    has_unknown_length: bool,
}

/// State for chunked transfer encoding decoder (reserved for future incremental chunked parsing)
#[derive(Debug, Default)]
#[allow(dead_code)]
struct ChunkedState {
    /// Remaining bytes in current chunk
    remaining: usize,
    /// Whether we're reading chunk size or chunk data
    reading_size: bool,
    /// Buffer for chunk size line
    size_buffer: Vec<u8>,
    /// Whether final chunk (0) was received
    done: bool,
}

impl HttpRequestParser {
    /// Create a new request parser
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            parsed: None,
            body_received: 0,
            complete: false,
            chunked_state: ChunkedState::default(),
        }
    }

    /// Feed data to the parser. Returns number of bytes consumed.
    pub fn feed(&mut self, data: &[u8]) -> usize {
        if self.complete {
            return 0;
        }

        // If headers not yet parsed, try to parse them
        if self.parsed.is_none() {
            self.buffer.extend_from_slice(data);

            if let Some(parsed) = Self::try_parse_headers(&self.buffer) {
                debug!(
                    "Parsed HTTP request: {} {} (content_length={:?}, chunked={})",
                    parsed.method, parsed.path, parsed.content_length, parsed.is_chunked
                );

                // Calculate how much body data is already in the buffer
                let body_in_buffer = self.buffer.len() - parsed.header_len;
                self.body_received = body_in_buffer;

                // Check if request is complete
                if let Some(content_length) = parsed.content_length {
                    if self.body_received >= content_length {
                        self.complete = true;
                    }
                } else if !parsed.is_chunked {
                    // No body expected for requests without Content-Length and not chunked
                    self.complete = true;
                } else {
                    // Check chunked completion
                    let body_start = parsed.header_len;
                    if body_start < self.buffer.len() {
                        self.complete = Self::check_chunked_complete(&self.buffer[body_start..]);
                    }
                }

                self.parsed = Some(parsed);
            }
        } else {
            // Headers already parsed, just count body bytes
            if let Some(ref parsed) = self.parsed {
                if parsed.is_chunked {
                    self.buffer.extend_from_slice(data);
                    let body_start = parsed.header_len;
                    self.complete = Self::check_chunked_complete(&self.buffer[body_start..]);
                } else {
                    self.body_received += data.len();
                    if let Some(content_length) = parsed.content_length {
                        if self.body_received >= content_length {
                            self.complete = true;
                        }
                    }
                }
            }
        }

        data.len()
    }

    /// Check if request is complete
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Get parsed request (if headers are complete)
    pub fn parsed(&self) -> Option<&ParsedRequest> {
        self.parsed.as_ref()
    }

    /// Get body bytes received
    pub fn body_received(&self) -> usize {
        self.body_received
    }

    /// Try to parse headers from buffer
    fn try_parse_headers(buffer: &[u8]) -> Option<ParsedRequest> {
        let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
        let mut req = httparse::Request::new(&mut headers);

        match req.parse(buffer) {
            Ok(httparse::Status::Complete(header_len)) => {
                let method = req.method.unwrap_or("").to_string();
                let path = req.path.unwrap_or("").to_string();
                let version = req.version.unwrap_or(1);

                let mut parsed_headers = Vec::new();
                let mut content_length = None;
                let mut is_chunked = false;

                for header in req.headers.iter() {
                    let name = header.name.to_string();
                    let value = String::from_utf8_lossy(header.value).to_string();

                    if name.eq_ignore_ascii_case("content-length") {
                        content_length = value.trim().parse().ok();
                    }
                    if name.eq_ignore_ascii_case("transfer-encoding")
                        && value.to_lowercase().contains("chunked")
                    {
                        is_chunked = true;
                    }

                    parsed_headers.push((name, value));
                }

                Some(ParsedRequest {
                    method,
                    path,
                    version,
                    headers: parsed_headers,
                    header_len,
                    content_length,
                    is_chunked,
                })
            }
            Ok(httparse::Status::Partial) => None, // Need more data
            Err(e) => {
                debug!("HTTP request parse error: {:?}", e);
                None
            }
        }
    }

    /// Check if chunked transfer is complete
    fn check_chunked_complete(body: &[u8]) -> bool {
        // Simple check: look for "0\r\n\r\n" pattern indicating final chunk
        // A more robust implementation would fully decode chunks
        if body.len() >= 5 {
            // Check if ends with final chunk
            let end = &body[body.len().saturating_sub(5)..];
            if end == b"0\r\n\r\n" {
                return true;
            }
        }

        // Also check for "0\r\n" followed by optional trailer headers and "\r\n"
        body.windows(3)
            .position(|w| w == b"0\r\n")
            .map(|pos| {
                // After "0\r\n", we need another "\r\n" (possibly with trailers in between)
                let after = &body[pos + 3..];
                after.windows(2).any(|w| w == b"\r\n")
            })
            .unwrap_or(false)
    }

    /// Reset parser for a new request (for keep-alive)
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.parsed = None;
        self.body_received = 0;
        self.complete = false;
        self.chunked_state = ChunkedState::default();
    }
}

impl Default for HttpRequestParser {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpResponseParser {
    /// Idle timeout for responses with unknown length (100ms)
    const IDLE_TIMEOUT_MS: u64 = 100;

    /// Create a new response parser
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            parsed: None,
            body_received: 0,
            complete: false,
            chunked_state: ChunkedState::default(),
            last_data_time: None,
            has_unknown_length: false,
        }
    }

    /// Feed data to the parser. Returns number of bytes consumed.
    pub fn feed(&mut self, data: &[u8]) -> usize {
        if self.complete {
            return 0;
        }

        // Track when we last received data (for idle timeout)
        self.last_data_time = Some(std::time::Instant::now());

        // If headers not yet parsed, try to parse them
        if self.parsed.is_none() {
            self.buffer.extend_from_slice(data);

            if let Some(parsed) = Self::try_parse_headers(&self.buffer) {
                debug!(
                    "Parsed HTTP response: {} {} (content_length={:?}, chunked={}, no_body={})",
                    parsed.status,
                    parsed.reason,
                    parsed.content_length,
                    parsed.is_chunked,
                    parsed.no_body
                );

                // Calculate how much body data is already in the buffer
                let body_in_buffer = self.buffer.len() - parsed.header_len;
                self.body_received = body_in_buffer;

                // Check if response is complete
                if parsed.no_body {
                    // 1xx, 204, 304 have no body
                    self.complete = true;
                } else if let Some(content_length) = parsed.content_length {
                    if content_length == 0 || self.body_received >= content_length {
                        self.complete = true;
                    }
                } else if parsed.is_chunked {
                    // Check chunked completion
                    let body_start = parsed.header_len;
                    if body_start < self.buffer.len() {
                        self.complete = Self::check_chunked_complete(&self.buffer[body_start..]);
                    }
                } else {
                    // No Content-Length, not chunked
                    self.has_unknown_length = true;
                    if self.body_received == 0 {
                        // Headers ended at chunk boundary with no body
                        self.complete = true;
                        debug!(
                            "Response complete: headers only (no Content-Length, not chunked, no body in buffer)"
                        );
                    }
                    // Otherwise we'll use idle timeout to detect completion
                }

                self.parsed = Some(parsed);
            }
        } else {
            // Headers already parsed, just count body bytes
            if let Some(ref parsed) = self.parsed {
                if parsed.is_chunked {
                    self.buffer.extend_from_slice(data);
                    let body_start = parsed.header_len;
                    self.complete = Self::check_chunked_complete(&self.buffer[body_start..]);
                } else if let Some(content_length) = parsed.content_length {
                    self.body_received += data.len();
                    if self.body_received >= content_length {
                        self.complete = true;
                    }
                } else {
                    // No length info - accumulate for idle timeout detection
                    self.body_received += data.len();
                }
            }
        }

        data.len()
    }

    /// Check if response is complete
    pub fn is_complete(&self) -> bool {
        // If already marked complete by Content-Length or chunked detection, return true
        if self.complete {
            return true;
        }

        // If headers not parsed yet, not complete
        if self.parsed.is_none() {
            return false;
        }

        // For ANY response that has received body data, check idle timeout as a fallback
        // This catches responses where chunked detection failed or Content-Length was wrong
        if self.body_received > 0 {
            if let Some(last_time) = self.last_data_time {
                let elapsed_ms = last_time.elapsed().as_millis() as u64;
                if elapsed_ms >= Self::IDLE_TIMEOUT_MS {
                    debug!(
                        "Response considered complete due to idle timeout ({}ms since last data, body_received={}, has_unknown_length={})",
                        elapsed_ms, self.body_received, self.has_unknown_length
                    );
                    return true;
                }
            }
        }

        false
    }

    /// Check if response has unknown length (no Content-Length, not chunked)
    /// Useful for deciding whether to apply idle timeout logic
    pub fn has_unknown_length(&self) -> bool {
        self.has_unknown_length
    }

    /// Mark response as complete (e.g., when connection closes)
    pub fn mark_complete(&mut self) {
        self.complete = true;
    }

    /// Get parsed response (if headers are complete)
    pub fn parsed(&self) -> Option<&ParsedResponse> {
        self.parsed.as_ref()
    }

    /// Get body bytes received
    pub fn body_received(&self) -> usize {
        self.body_received
    }

    /// Get the accumulated body data
    pub fn body_data(&self) -> Option<&[u8]> {
        self.parsed.as_ref().map(|p| &self.buffer[p.header_len..])
    }

    /// Try to parse headers from buffer
    fn try_parse_headers(buffer: &[u8]) -> Option<ParsedResponse> {
        let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
        let mut resp = httparse::Response::new(&mut headers);

        match resp.parse(buffer) {
            Ok(httparse::Status::Complete(header_len)) => {
                let status = resp.code.unwrap_or(0);
                let version = resp.version.unwrap_or(1);
                let reason = resp.reason.unwrap_or("").to_string();

                let mut parsed_headers = Vec::new();
                let mut content_length = None;
                let mut is_chunked = false;

                for header in resp.headers.iter() {
                    let name = header.name.to_string();
                    let value = String::from_utf8_lossy(header.value).to_string();

                    if name.eq_ignore_ascii_case("content-length") {
                        content_length = value.trim().parse().ok();
                    }
                    if name.eq_ignore_ascii_case("transfer-encoding")
                        && value.to_lowercase().contains("chunked")
                    {
                        is_chunked = true;
                    }

                    parsed_headers.push((name, value));
                }

                // Determine if this response has no body per RFC 7230
                let no_body = matches!(status, 100..=199 | 204 | 304);

                Some(ParsedResponse {
                    status,
                    version,
                    reason,
                    headers: parsed_headers,
                    header_len,
                    content_length,
                    is_chunked,
                    no_body,
                })
            }
            Ok(httparse::Status::Partial) => None, // Need more data
            Err(e) => {
                debug!("HTTP response parse error: {:?}", e);
                None
            }
        }
    }

    /// Check if chunked transfer is complete
    fn check_chunked_complete(body: &[u8]) -> bool {
        // Simple check: look for "0\r\n\r\n" pattern indicating final chunk
        if body.len() >= 5 {
            let end = &body[body.len().saturating_sub(5)..];
            if end == b"0\r\n\r\n" {
                return true;
            }
        }

        // Also check for "0\r\n" followed by "\r\n"
        body.windows(3)
            .position(|w| w == b"0\r\n")
            .map(|pos| {
                let after = &body[pos + 3..];
                after.windows(2).any(|w| w == b"\r\n")
            })
            .unwrap_or(false)
    }

    /// Reset parser for a new response (for keep-alive)
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.parsed = None;
        self.body_received = 0;
        self.complete = false;
        self.chunked_state = ChunkedState::default();
        self.last_data_time = None;
        self.has_unknown_length = false;
    }
}

impl Default for HttpResponseParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_request() {
        let mut parser = HttpRequestParser::new();
        let request = b"GET /path HTTP/1.1\r\nHost: example.com\r\n\r\n";
        parser.feed(request);

        assert!(parser.is_complete());
        let parsed = parser.parsed().unwrap();
        assert_eq!(parsed.method, "GET");
        assert_eq!(parsed.path, "/path");
        assert_eq!(parsed.content_length, None);
        assert!(!parsed.is_chunked);
    }

    #[test]
    fn test_parse_request_with_body() {
        let mut parser = HttpRequestParser::new();
        let request = b"POST /api HTTP/1.1\r\nContent-Length: 13\r\n\r\n{\"key\":\"val\"}";
        parser.feed(request);

        assert!(parser.is_complete());
        let parsed = parser.parsed().unwrap();
        assert_eq!(parsed.method, "POST");
        assert_eq!(parsed.content_length, Some(13));
    }

    #[test]
    fn test_parse_simple_response() {
        let mut parser = HttpResponseParser::new();
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        parser.feed(response);

        assert!(parser.is_complete());
        let parsed = parser.parsed().unwrap();
        assert_eq!(parsed.status, 200);
        assert_eq!(parsed.content_length, Some(5));
    }

    #[test]
    fn test_parse_204_no_content() {
        let mut parser = HttpResponseParser::new();
        let response = b"HTTP/1.1 204 No Content\r\n\r\n";
        parser.feed(response);

        assert!(parser.is_complete());
        let parsed = parser.parsed().unwrap();
        assert_eq!(parsed.status, 204);
        assert!(parsed.no_body);
    }

    #[test]
    fn test_parse_304_not_modified() {
        let mut parser = HttpResponseParser::new();
        let response = b"HTTP/1.1 304 Not Modified\r\n\r\n";
        parser.feed(response);

        assert!(parser.is_complete());
        let parsed = parser.parsed().unwrap();
        assert_eq!(parsed.status, 304);
        assert!(parsed.no_body);
    }

    #[test]
    fn test_parse_404_no_content_length() {
        let mut parser = HttpResponseParser::new();
        // 404 with no Content-Length and no body in the same chunk
        let response = b"HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\n\r\n";
        parser.feed(response);

        // Should be complete because headers ended at chunk boundary with no body
        assert!(parser.is_complete());
        let parsed = parser.parsed().unwrap();
        assert_eq!(parsed.status, 404);
    }

    #[test]
    fn test_parse_chunked_response() {
        let mut parser = HttpResponseParser::new();
        let response =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n";
        parser.feed(response);

        assert!(parser.is_complete());
        let parsed = parser.parsed().unwrap();
        assert!(parsed.is_chunked);
    }

    #[test]
    fn test_incremental_parsing() {
        let mut parser = HttpResponseParser::new();

        // Feed headers
        parser.feed(b"HTTP/1.1 200 OK\r\n");
        assert!(!parser.is_complete());
        assert!(parser.parsed().is_none());

        // Feed more headers
        parser.feed(b"Content-Length: 5\r\n\r\n");
        assert!(!parser.is_complete()); // Headers complete but no body yet
        assert!(parser.parsed().is_some());

        // Feed body
        parser.feed(b"hello");
        assert!(parser.is_complete());
    }
}
