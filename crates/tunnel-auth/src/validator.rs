//! Authentication validator trait for pluggable authentication strategies
//!
//! This module provides a trait-based authentication system that allows you to
//! implement custom authentication logic (JWT, API keys, OAuth, database lookup, etc.)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Authentication result containing validated identity and claims
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthResult {
    /// Tunnel ID (used for routing)
    pub tunnel_id: String,

    /// User ID (optional, for your application's user tracking)
    pub user_id: Option<String>,

    /// Allowed protocols (empty = all allowed)
    pub allowed_protocols: Vec<String>,

    /// Allowed regions (empty = all allowed)
    pub allowed_regions: Vec<String>,

    /// Custom metadata (plan tier, rate limits, etc.)
    pub metadata: HashMap<String, String>,
}

impl AuthResult {
    /// Create a new auth result with just a tunnel ID
    pub fn new(tunnel_id: String) -> Self {
        Self {
            tunnel_id,
            user_id: None,
            allowed_protocols: Vec::new(),
            allowed_regions: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add user ID
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Add allowed protocols
    pub fn with_protocols(mut self, protocols: Vec<String>) -> Self {
        self.allowed_protocols = protocols;
        self
    }

    /// Add allowed regions
    pub fn with_regions(mut self, regions: Vec<String>) -> Self {
        self.allowed_regions = regions;
        self
    }

    /// Add custom metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Check if a protocol is allowed
    pub fn is_protocol_allowed(&self, protocol: &str) -> bool {
        self.allowed_protocols.is_empty() || self.allowed_protocols.contains(&protocol.to_string())
    }

    /// Check if a region is allowed
    pub fn is_region_allowed(&self, region: &str) -> bool {
        self.allowed_regions.is_empty() || self.allowed_regions.contains(&region.to_string())
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
}

/// Authentication errors
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Authentication validator trait
///
/// Implement this trait to provide custom authentication logic.
/// The validator takes an authentication token (JWT, API key, etc.) and
/// returns an `AuthResult` with the authenticated identity and permissions.
///
/// # Example: API Key Validator
///
/// ```ignore
/// use tunnel_auth::{AuthValidator, AuthResult, AuthError};
/// use async_trait::async_trait;
///
/// struct ApiKeyValidator {
///     valid_keys: HashMap<String, String>, // api_key -> tunnel_id
/// }
///
/// #[async_trait]
/// impl AuthValidator for ApiKeyValidator {
///     async fn validate(&self, token: &str) -> Result<AuthResult, AuthError> {
///         match self.valid_keys.get(token) {
///             Some(tunnel_id) => Ok(AuthResult::new(tunnel_id.clone())),
///             None => Err(AuthError::InvalidToken("Unknown API key".to_string())),
///         }
///     }
/// }
/// ```
#[async_trait]
pub trait AuthValidator: Send + Sync {
    /// Validate an authentication token and return the authenticated identity
    ///
    /// # Arguments
    ///
    /// * `token` - The authentication token (JWT, API key, etc.)
    ///
    /// # Returns
    ///
    /// * `Ok(AuthResult)` - Successfully authenticated with identity and claims
    /// * `Err(AuthError)` - Authentication failed
    async fn validate(&self, token: &str) -> Result<AuthResult, AuthError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_result_builder() {
        let result = AuthResult::new("tunnel-123".to_string())
            .with_user_id("user-456".to_string())
            .with_protocols(vec!["http".to_string(), "https".to_string()])
            .with_regions(vec!["us-east".to_string()])
            .with_metadata("plan".to_string(), "pro".to_string());

        assert_eq!(result.tunnel_id, "tunnel-123");
        assert_eq!(result.user_id, Some("user-456".to_string()));
        assert!(result.is_protocol_allowed("http"));
        assert!(result.is_protocol_allowed("https"));
        assert!(!result.is_protocol_allowed("tcp"));
        assert!(result.is_region_allowed("us-east"));
        assert!(!result.is_region_allowed("eu-west"));
        assert_eq!(result.get_metadata("plan"), Some(&"pro".to_string()));
    }

    #[test]
    fn test_empty_allowed_means_all_allowed() {
        let result = AuthResult::new("tunnel-123".to_string());

        // Empty allowed lists mean everything is allowed
        assert!(result.is_protocol_allowed("http"));
        assert!(result.is_protocol_allowed("tcp"));
        assert!(result.is_region_allowed("us-east"));
        assert!(result.is_region_allowed("eu-west"));
    }
}
