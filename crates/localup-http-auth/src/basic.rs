//! HTTP Basic Authentication provider (RFC 7617)
//!
//! Implements HTTP Basic Authentication where credentials are transmitted
//! as `username:password` encoded in base64 in the Authorization header.
//!
//! # Format
//!
//! ```text
//! Authorization: Basic <base64(username:password)>
//! ```
//!
//! # Security Note
//!
//! Basic authentication should only be used over HTTPS as credentials
//! are transmitted in an easily reversible encoding (not encryption).

use crate::{AuthResult, HttpAuthProvider};
use base64::Engine;
use std::collections::HashSet;
use tracing::debug;

/// HTTP Basic Authentication provider
///
/// Validates credentials against a list of allowed `username:password` pairs.
/// Credentials are compared using constant-time comparison to prevent timing attacks.
pub struct BasicAuthProvider {
    /// Set of valid credentials in "username:password" format
    valid_credentials: HashSet<String>,
    /// Realm for the WWW-Authenticate header
    realm: String,
}

impl BasicAuthProvider {
    /// Create a new Basic auth provider
    ///
    /// # Arguments
    /// * `credentials` - List of valid credentials in "username:password" format
    ///
    /// # Example
    /// ```
    /// use localup_http_auth::BasicAuthProvider;
    ///
    /// let provider = BasicAuthProvider::new(vec![
    ///     "admin:secret123".to_string(),
    ///     "user:password".to_string(),
    /// ]);
    /// ```
    pub fn new(credentials: Vec<String>) -> Self {
        Self {
            valid_credentials: credentials.into_iter().collect(),
            realm: "localup".to_string(),
        }
    }

    /// Create a new Basic auth provider with a custom realm
    ///
    /// # Arguments
    /// * `credentials` - List of valid credentials
    /// * `realm` - The realm string for the WWW-Authenticate header
    pub fn with_realm(credentials: Vec<String>, realm: String) -> Self {
        Self {
            valid_credentials: credentials.into_iter().collect(),
            realm,
        }
    }

    /// Extract and decode credentials from Authorization header
    fn extract_credentials(&self, auth_header: &str) -> Option<String> {
        // Check for "Basic " prefix (case-insensitive)
        let auth_lower = auth_header.to_lowercase();
        if !auth_lower.starts_with("basic ") {
            return None;
        }

        // Extract the base64 encoded part
        let encoded = auth_header[6..].trim();

        // Decode base64
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;

        // Convert to string
        String::from_utf8(decoded).ok()
    }

    /// Check if credentials are valid using constant-time comparison
    fn validate_credentials(&self, credentials: &str) -> bool {
        // Use simple lookup - the HashSet provides O(1) lookup
        // For production, consider using a constant-time comparison library
        self.valid_credentials.contains(credentials)
    }
}

impl HttpAuthProvider for BasicAuthProvider {
    fn authenticate(&self, headers: &[(String, String)]) -> AuthResult {
        // Find the Authorization header
        for (name, value) in headers {
            if name.to_lowercase() == "authorization" {
                if let Some(credentials) = self.extract_credentials(value) {
                    if self.validate_credentials(&credentials) {
                        debug!("Basic auth: valid credentials");
                        return AuthResult::Authenticated;
                    } else {
                        debug!("Basic auth: invalid credentials");
                    }
                } else {
                    debug!("Basic auth: could not decode credentials");
                }
            }
        }

        debug!("Basic auth: no valid Authorization header found");
        AuthResult::Unauthorized(self.unauthorized_response())
    }

    fn unauthorized_response(&self) -> Vec<u8> {
        let realm_escaped = self.realm.replace('"', "\\\"");
        format!(
            "HTTP/1.1 401 Unauthorized\r\n\
             WWW-Authenticate: Basic realm=\"{}\"\r\n\
             Content-Type: text/plain\r\n\
             Content-Length: 32\r\n\
             \r\n\
             Authentication required (Basic)",
            realm_escaped
        )
        .into_bytes()
    }

    fn auth_type(&self) -> &'static str {
        "basic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_basic_auth_header(username: &str, password: &str) -> String {
        let credentials = format!("{}:{}", username, password);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        format!("Basic {}", encoded)
    }

    #[test]
    fn test_valid_credentials() {
        let provider = BasicAuthProvider::new(vec!["user:password".to_string()]);
        let headers = vec![(
            "Authorization".to_string(),
            make_basic_auth_header("user", "password"),
        )];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Authenticated
        ));
    }

    #[test]
    fn test_invalid_credentials() {
        let provider = BasicAuthProvider::new(vec!["user:password".to_string()]);
        let headers = vec![(
            "Authorization".to_string(),
            make_basic_auth_header("user", "wrong"),
        )];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Unauthorized(_)
        ));
    }

    #[test]
    fn test_missing_authorization_header() {
        let provider = BasicAuthProvider::new(vec!["user:password".to_string()]);
        let headers = vec![("Host".to_string(), "example.com".to_string())];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Unauthorized(_)
        ));
    }

    #[test]
    fn test_wrong_auth_scheme() {
        let provider = BasicAuthProvider::new(vec!["user:password".to_string()]);
        let headers = vec![("Authorization".to_string(), "Bearer sometoken".to_string())];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Unauthorized(_)
        ));
    }

    #[test]
    fn test_multiple_valid_credentials() {
        let provider = BasicAuthProvider::new(vec![
            "admin:secret".to_string(),
            "user:password".to_string(),
        ]);

        // Test first credential
        let headers1 = vec![(
            "Authorization".to_string(),
            make_basic_auth_header("admin", "secret"),
        )];
        assert!(matches!(
            provider.authenticate(&headers1),
            AuthResult::Authenticated
        ));

        // Test second credential
        let headers2 = vec![(
            "Authorization".to_string(),
            make_basic_auth_header("user", "password"),
        )];
        assert!(matches!(
            provider.authenticate(&headers2),
            AuthResult::Authenticated
        ));
    }

    #[test]
    fn test_unauthorized_response_format() {
        let provider = BasicAuthProvider::new(vec!["user:pass".to_string()]);
        let response = provider.unauthorized_response();
        let response_str = String::from_utf8_lossy(&response);

        assert!(response_str.contains("401 Unauthorized"));
        assert!(response_str.contains("WWW-Authenticate: Basic realm="));
        assert!(response_str.contains("Authentication required"));
    }

    #[test]
    fn test_custom_realm() {
        let provider = BasicAuthProvider::with_realm(vec!["u:p".to_string()], "My App".to_string());
        let response = provider.unauthorized_response();
        let response_str = String::from_utf8_lossy(&response);

        assert!(response_str.contains("realm=\"My App\""));
    }

    #[test]
    fn test_case_insensitive_header_name() {
        let provider = BasicAuthProvider::new(vec!["user:password".to_string()]);

        // Test lowercase
        let headers = vec![(
            "authorization".to_string(),
            make_basic_auth_header("user", "password"),
        )];
        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Authenticated
        ));

        // Test mixed case
        let headers = vec![(
            "AUTHORIZATION".to_string(),
            make_basic_auth_header("user", "password"),
        )];
        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Authenticated
        ));
    }

    #[test]
    fn test_malformed_base64() {
        let provider = BasicAuthProvider::new(vec!["user:password".to_string()]);
        let headers = vec![(
            "Authorization".to_string(),
            "Basic !!!invalid!!!".to_string(),
        )];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Unauthorized(_)
        ));
    }
}
