//! HTTP Authentication middleware for localup tunnels
//!
//! This crate provides an extensible authentication framework for HTTP requests
//! flowing through tunnels. It supports multiple authentication methods via
//! a trait-based design.
//!
//! # Supported Authentication Methods
//!
//! - **Basic**: HTTP Basic Authentication (RFC 7617)
//! - **BearerToken**: Authorization header with Bearer token
//! - **HeaderAuth**: Custom header-based authentication
//!
//! # Usage
//!
//! ```ignore
//! use localup_http_auth::{HttpAuthenticator, AuthResult};
//! use localup_proto::HttpAuthConfig;
//!
//! let config = HttpAuthConfig::Basic {
//!     credentials: vec!["user:password".to_string()],
//! };
//!
//! let authenticator = HttpAuthenticator::from_config(&config);
//! let headers = vec![("Authorization".to_string(), "Basic dXNlcjpwYXNzd29yZA==".to_string())];
//!
//! match authenticator.authenticate(&headers) {
//!     AuthResult::Authenticated => { /* proceed */ }
//!     AuthResult::Unauthorized(response) => { /* return 401 */ }
//! }
//! ```
//!
//! # Extensibility
//!
//! To add a new authentication method:
//!
//! 1. Add variant to `HttpAuthConfig` in `localup-proto`
//! 2. Implement `HttpAuthProvider` trait
//! 3. Add case to `HttpAuthenticator::from_config()`

mod basic;
mod bearer;
mod header;

pub use basic::BasicAuthProvider;
pub use bearer::BearerTokenProvider;
pub use header::HeaderAuthProvider;

use localup_proto::HttpAuthConfig;
use thiserror::Error;

/// Authentication result
#[derive(Debug, Clone)]
pub enum AuthResult {
    /// Request is authenticated (no auth required or valid credentials)
    Authenticated,
    /// Request requires authentication - includes the 401 response bytes
    Unauthorized(Vec<u8>),
}

/// Error type for authentication operations
#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid credentials format: {0}")]
    InvalidFormat(String),

    #[error("Decoding error: {0}")]
    DecodingError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Trait for implementing HTTP authentication providers
///
/// Implement this trait to add support for new authentication methods.
/// The trait is designed to be simple and stateless - each call to `authenticate`
/// should be independent.
///
/// # Example Implementation
///
/// ```ignore
/// use localup_http_auth::{HttpAuthProvider, AuthResult};
///
/// struct MyCustomAuth {
///     api_key: String,
/// }
///
/// impl HttpAuthProvider for MyCustomAuth {
///     fn authenticate(&self, headers: &[(String, String)]) -> AuthResult {
///         for (name, value) in headers {
///             if name.to_lowercase() == "x-api-key" && value == &self.api_key {
///                 return AuthResult::Authenticated;
///             }
///         }
///         AuthResult::Unauthorized(self.unauthorized_response())
///     }
///
///     fn unauthorized_response(&self) -> Vec<u8> {
///         b"HTTP/1.1 401 Unauthorized\r\n\
///           WWW-Authenticate: ApiKey\r\n\
///           Content-Length: 12\r\n\r\n\
///           Unauthorized".to_vec()
///     }
///
///     fn auth_type(&self) -> &'static str {
///         "api-key"
///     }
/// }
/// ```
pub trait HttpAuthProvider: Send + Sync {
    /// Authenticate the request based on headers
    ///
    /// # Arguments
    /// * `headers` - List of (header_name, header_value) pairs from the HTTP request
    ///
    /// # Returns
    /// - `AuthResult::Authenticated` if the request should proceed
    /// - `AuthResult::Unauthorized(response)` if auth failed (with HTTP 401 response)
    fn authenticate(&self, headers: &[(String, String)]) -> AuthResult;

    /// Generate the 401 Unauthorized response for this auth type
    fn unauthorized_response(&self) -> Vec<u8>;

    /// Return the authentication type name (for logging)
    fn auth_type(&self) -> &'static str;
}

/// No-op authentication provider (always allows requests)
pub struct NoAuthProvider;

impl HttpAuthProvider for NoAuthProvider {
    fn authenticate(&self, _headers: &[(String, String)]) -> AuthResult {
        AuthResult::Authenticated
    }

    fn unauthorized_response(&self) -> Vec<u8> {
        // Should never be called, but provide a sensible default
        b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 12\r\n\r\nUnauthorized".to_vec()
    }

    fn auth_type(&self) -> &'static str {
        "none"
    }
}

/// Main HTTP authenticator that wraps any authentication provider
///
/// This is the primary interface for authentication. Create an instance
/// from `HttpAuthConfig` and use it to authenticate incoming requests.
pub struct HttpAuthenticator {
    provider: Box<dyn HttpAuthProvider>,
}

impl HttpAuthenticator {
    /// Create a new authenticator from the given configuration
    ///
    /// # Arguments
    /// * `config` - The authentication configuration from the tunnel
    ///
    /// # Returns
    /// An `HttpAuthenticator` configured with the appropriate provider
    pub fn from_config(config: &HttpAuthConfig) -> Self {
        let provider: Box<dyn HttpAuthProvider> = match config {
            HttpAuthConfig::None => Box::new(NoAuthProvider),
            HttpAuthConfig::Basic { credentials } => {
                Box::new(BasicAuthProvider::new(credentials.clone()))
            }
            HttpAuthConfig::BearerToken { tokens } => {
                Box::new(BearerTokenProvider::new(tokens.clone()))
            }
            HttpAuthConfig::HeaderAuth {
                header_name,
                values,
            } => Box::new(HeaderAuthProvider::new(header_name.clone(), values.clone())),
        };

        Self { provider }
    }

    /// Create a new authenticator with a custom provider
    ///
    /// Use this method when you have a custom authentication provider
    /// that implements `HttpAuthProvider`.
    pub fn with_provider(provider: Box<dyn HttpAuthProvider>) -> Self {
        Self { provider }
    }

    /// Authenticate an HTTP request
    ///
    /// # Arguments
    /// * `headers` - List of (header_name, header_value) pairs
    ///
    /// # Returns
    /// `AuthResult::Authenticated` or `AuthResult::Unauthorized(response)`
    pub fn authenticate(&self, headers: &[(String, String)]) -> AuthResult {
        self.provider.authenticate(headers)
    }

    /// Get the authentication type name
    pub fn auth_type(&self) -> &'static str {
        self.provider.auth_type()
    }

    /// Check if authentication is required
    pub fn requires_auth(&self) -> bool {
        self.provider.auth_type() != "none"
    }
}

impl Default for HttpAuthenticator {
    fn default() -> Self {
        Self::from_config(&HttpAuthConfig::None)
    }
}

/// Parse HTTP headers from raw request bytes
///
/// This is a utility function to extract headers from the initial HTTP request
/// data received from the client.
///
/// # Arguments
/// * `data` - Raw HTTP request bytes
///
/// # Returns
/// A vector of (header_name, header_value) pairs
pub fn parse_headers_from_request(data: &[u8]) -> Vec<(String, String)> {
    let request_str = String::from_utf8_lossy(data);
    let mut headers = Vec::new();

    // Skip the request line, parse headers
    for line in request_str.lines().skip(1) {
        if line.is_empty() {
            break; // End of headers
        }
        if let Some(colon_pos) = line.find(':') {
            let name = line[..colon_pos].trim().to_string();
            let value = line[colon_pos + 1..].trim().to_string();
            headers.push((name, value));
        }
    }

    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_auth_provider_always_authenticates() {
        let provider = NoAuthProvider;
        let headers = vec![("Host".to_string(), "example.com".to_string())];
        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Authenticated
        ));
    }

    #[test]
    fn test_authenticator_from_none_config() {
        let config = HttpAuthConfig::None;
        let auth = HttpAuthenticator::from_config(&config);
        assert_eq!(auth.auth_type(), "none");
        assert!(!auth.requires_auth());
    }

    #[test]
    fn test_parse_headers_from_request() {
        let request =
            b"GET / HTTP/1.1\r\nHost: example.com\r\nAuthorization: Basic dXNlcjpwYXNz\r\n\r\n";
        let headers = parse_headers_from_request(request);

        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0], ("Host".to_string(), "example.com".to_string()));
        assert_eq!(
            headers[1],
            (
                "Authorization".to_string(),
                "Basic dXNlcjpwYXNz".to_string()
            )
        );
    }

    #[test]
    fn test_parse_headers_handles_empty_request() {
        let request = b"";
        let headers = parse_headers_from_request(request);
        assert!(headers.is_empty());
    }
}
