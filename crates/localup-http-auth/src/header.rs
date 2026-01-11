//! Custom Header Authentication provider
//!
//! Implements authentication based on a custom header and expected values.
//! This is useful for API key authentication or custom authentication schemes.
//!
//! # Example
//!
//! ```text
//! X-API-Key: secret-key-123
//! ```

use crate::{AuthResult, HttpAuthProvider};
use std::collections::HashSet;
use tracing::debug;

/// Custom Header Authentication provider
///
/// Validates that a specific header contains one of the expected values.
pub struct HeaderAuthProvider {
    /// Name of the header to check (case-insensitive)
    header_name: String,
    /// Set of valid header values
    valid_values: HashSet<String>,
}

impl HeaderAuthProvider {
    /// Create a new header authentication provider
    ///
    /// # Arguments
    /// * `header_name` - Name of the header to check (case-insensitive)
    /// * `values` - List of valid header values
    ///
    /// # Example
    /// ```
    /// use localup_http_auth::HeaderAuthProvider;
    ///
    /// let provider = HeaderAuthProvider::new(
    ///     "X-API-Key".to_string(),
    ///     vec!["key-123".to_string(), "key-456".to_string()],
    /// );
    /// ```
    pub fn new(header_name: String, values: Vec<String>) -> Self {
        Self {
            header_name,
            valid_values: values.into_iter().collect(),
        }
    }
}

impl HttpAuthProvider for HeaderAuthProvider {
    fn authenticate(&self, headers: &[(String, String)]) -> AuthResult {
        let target_header = self.header_name.to_lowercase();

        // Find the target header
        for (name, value) in headers {
            if name.to_lowercase() == target_header {
                if self.valid_values.contains(value) {
                    debug!("Header auth: valid value for header '{}'", self.header_name);
                    return AuthResult::Authenticated;
                } else {
                    debug!(
                        "Header auth: invalid value for header '{}'",
                        self.header_name
                    );
                }
            }
        }

        debug!(
            "Header auth: header '{}' not found or invalid",
            self.header_name
        );
        AuthResult::Unauthorized(self.unauthorized_response())
    }

    fn unauthorized_response(&self) -> Vec<u8> {
        format!(
            "HTTP/1.1 401 Unauthorized\r\n\
             Content-Type: text/plain\r\n\
             Content-Length: 44\r\n\
             \r\n\
             Authentication required (header: {})",
            self.header_name
        )
        .into_bytes()
    }

    fn auth_type(&self) -> &'static str {
        "header"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_header_value() {
        let provider =
            HeaderAuthProvider::new("X-API-Key".to_string(), vec!["secret-key".to_string()]);
        let headers = vec![("X-API-Key".to_string(), "secret-key".to_string())];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Authenticated
        ));
    }

    #[test]
    fn test_invalid_header_value() {
        let provider =
            HeaderAuthProvider::new("X-API-Key".to_string(), vec!["secret-key".to_string()]);
        let headers = vec![("X-API-Key".to_string(), "wrong-key".to_string())];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Unauthorized(_)
        ));
    }

    #[test]
    fn test_missing_header() {
        let provider =
            HeaderAuthProvider::new("X-API-Key".to_string(), vec!["secret-key".to_string()]);
        let headers = vec![("Host".to_string(), "example.com".to_string())];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Unauthorized(_)
        ));
    }

    #[test]
    fn test_case_insensitive_header_name() {
        let provider =
            HeaderAuthProvider::new("X-API-Key".to_string(), vec!["secret-key".to_string()]);

        // Test lowercase
        let headers = vec![("x-api-key".to_string(), "secret-key".to_string())];
        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Authenticated
        ));

        // Test uppercase
        let headers = vec![("X-API-KEY".to_string(), "secret-key".to_string())];
        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Authenticated
        ));
    }

    #[test]
    fn test_multiple_valid_values() {
        let provider = HeaderAuthProvider::new(
            "X-API-Key".to_string(),
            vec!["key1".to_string(), "key2".to_string()],
        );

        let headers1 = vec![("X-API-Key".to_string(), "key1".to_string())];
        assert!(matches!(
            provider.authenticate(&headers1),
            AuthResult::Authenticated
        ));

        let headers2 = vec![("X-API-Key".to_string(), "key2".to_string())];
        assert!(matches!(
            provider.authenticate(&headers2),
            AuthResult::Authenticated
        ));
    }

    #[test]
    fn test_value_is_case_sensitive() {
        let provider =
            HeaderAuthProvider::new("X-API-Key".to_string(), vec!["Secret-Key".to_string()]);
        let headers = vec![("X-API-Key".to_string(), "secret-key".to_string())];

        // Value comparison should be case-sensitive
        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Unauthorized(_)
        ));
    }
}
