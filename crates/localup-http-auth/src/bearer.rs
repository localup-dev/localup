//! Bearer Token Authentication provider (RFC 6750)
//!
//! Implements Bearer token authentication where tokens are transmitted
//! in the Authorization header.
//!
//! # Format
//!
//! ```text
//! Authorization: Bearer <token>
//! ```

use crate::{AuthResult, HttpAuthProvider};
use std::collections::HashSet;
use tracing::debug;

/// Bearer Token Authentication provider
///
/// Validates tokens against a list of allowed tokens.
pub struct BearerTokenProvider {
    /// Set of valid tokens
    valid_tokens: HashSet<String>,
}

impl BearerTokenProvider {
    /// Create a new Bearer token provider
    ///
    /// # Arguments
    /// * `tokens` - List of valid bearer tokens
    ///
    /// # Example
    /// ```
    /// use localup_http_auth::BearerTokenProvider;
    ///
    /// let provider = BearerTokenProvider::new(vec![
    ///     "secret-token-123".to_string(),
    ///     "another-token".to_string(),
    /// ]);
    /// ```
    pub fn new(tokens: Vec<String>) -> Self {
        Self {
            valid_tokens: tokens.into_iter().collect(),
        }
    }

    /// Extract token from Authorization header
    fn extract_token(&self, auth_header: &str) -> Option<String> {
        // Check for "Bearer " prefix (case-insensitive)
        let auth_lower = auth_header.to_lowercase();
        if !auth_lower.starts_with("bearer ") {
            return None;
        }

        // Extract the token part
        let token = auth_header[7..].trim().to_string();
        if token.is_empty() {
            return None;
        }

        Some(token)
    }
}

impl HttpAuthProvider for BearerTokenProvider {
    fn authenticate(&self, headers: &[(String, String)]) -> AuthResult {
        // Find the Authorization header
        for (name, value) in headers {
            if name.to_lowercase() == "authorization" {
                if let Some(token) = self.extract_token(value) {
                    if self.valid_tokens.contains(&token) {
                        debug!("Bearer auth: valid token");
                        return AuthResult::Authenticated;
                    } else {
                        debug!("Bearer auth: invalid token");
                    }
                } else {
                    debug!("Bearer auth: could not extract token");
                }
            }
        }

        debug!("Bearer auth: no valid Authorization header found");
        AuthResult::Unauthorized(self.unauthorized_response())
    }

    fn unauthorized_response(&self) -> Vec<u8> {
        b"HTTP/1.1 401 Unauthorized\r\n\
          WWW-Authenticate: Bearer\r\n\
          Content-Type: text/plain\r\n\
          Content-Length: 33\r\n\
          \r\n\
          Authentication required (Bearer)"
            .to_vec()
    }

    fn auth_type(&self) -> &'static str {
        "bearer"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_token() {
        let provider = BearerTokenProvider::new(vec!["my-secret-token".to_string()]);
        let headers = vec![(
            "Authorization".to_string(),
            "Bearer my-secret-token".to_string(),
        )];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Authenticated
        ));
    }

    #[test]
    fn test_invalid_token() {
        let provider = BearerTokenProvider::new(vec!["my-secret-token".to_string()]);
        let headers = vec![(
            "Authorization".to_string(),
            "Bearer wrong-token".to_string(),
        )];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Unauthorized(_)
        ));
    }

    #[test]
    fn test_missing_authorization_header() {
        let provider = BearerTokenProvider::new(vec!["token".to_string()]);
        let headers = vec![("Host".to_string(), "example.com".to_string())];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Unauthorized(_)
        ));
    }

    #[test]
    fn test_wrong_auth_scheme() {
        let provider = BearerTokenProvider::new(vec!["token".to_string()]);
        let headers = vec![(
            "Authorization".to_string(),
            "Basic dXNlcjpwYXNz".to_string(),
        )];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Unauthorized(_)
        ));
    }

    #[test]
    fn test_multiple_valid_tokens() {
        let provider = BearerTokenProvider::new(vec!["token1".to_string(), "token2".to_string()]);

        let headers1 = vec![("Authorization".to_string(), "Bearer token1".to_string())];
        assert!(matches!(
            provider.authenticate(&headers1),
            AuthResult::Authenticated
        ));

        let headers2 = vec![("Authorization".to_string(), "Bearer token2".to_string())];
        assert!(matches!(
            provider.authenticate(&headers2),
            AuthResult::Authenticated
        ));
    }

    #[test]
    fn test_case_insensitive_bearer_prefix() {
        let provider = BearerTokenProvider::new(vec!["mytoken".to_string()]);

        let headers = vec![("Authorization".to_string(), "BEARER mytoken".to_string())];
        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Authenticated
        ));
    }

    #[test]
    fn test_empty_token_rejected() {
        let provider = BearerTokenProvider::new(vec!["token".to_string()]);
        let headers = vec![("Authorization".to_string(), "Bearer ".to_string())];

        assert!(matches!(
            provider.authenticate(&headers),
            AuthResult::Unauthorized(_)
        ));
    }
}
