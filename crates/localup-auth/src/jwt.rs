//! JWT (JSON Web Token) handling

use async_trait::async_trait;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::validator::{AuthError, AuthResult, AuthValidator};

/// JWT claims for tunnel authentication
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JwtClaims {
    /// Subject (tunnel ID)
    pub sub: String,
    /// Issued at (timestamp)
    pub iat: i64,
    /// Expiration time (timestamp)
    pub exp: i64,
    /// Issuer
    pub iss: String,
    /// Audience
    pub aud: String,
    /// Custom: allowed protocols
    #[serde(default)]
    pub protocols: Vec<String>,
    /// Custom: allowed regions
    #[serde(default)]
    pub regions: Vec<String>,
    /// Custom: whether client can request reverse tunnels (agent-to-client connections)
    /// Default: None (backward compatibility - assume allowed if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_tunnel: Option<bool>,
    /// Custom: list of agent IDs client can connect to via reverse tunnels
    /// If None or empty, all agents are allowed (default for backward compatibility)
    /// If Some([...]), only specified agent IDs are allowed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_agents: Option<Vec<String>>,
    /// Custom: list of target addresses client can access via reverse tunnels
    /// Format: "host:port" or "192.168.1.100:8080"
    /// If None or empty, all addresses are allowed (default for backward compatibility)
    /// If Some([...]), only specified addresses are allowed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_addresses: Option<Vec<String>>,
}

impl JwtClaims {
    pub fn new(localup_id: String, issuer: String, audience: String, validity: Duration) -> Self {
        let now = Utc::now();
        let exp = now + validity;

        Self {
            sub: localup_id,
            iat: now.timestamp(),
            exp: exp.timestamp(),
            iss: issuer,
            aud: audience,
            protocols: Vec::new(),
            regions: Vec::new(),
            reverse_tunnel: None,
            allowed_agents: None,
            allowed_addresses: None,
        }
    }

    pub fn with_protocols(mut self, protocols: Vec<String>) -> Self {
        self.protocols = protocols;
        self
    }

    pub fn with_regions(mut self, regions: Vec<String>) -> Self {
        self.regions = regions;
        self
    }

    /// Enable reverse tunnel access for this client
    /// If not called, reverse_tunnel will be None (backward compatible - assumed allowed)
    pub fn with_reverse_tunnel(mut self, enabled: bool) -> Self {
        self.reverse_tunnel = Some(enabled);
        self
    }

    /// Restrict reverse tunnel access to specific agent IDs
    /// If not called or empty Vec, all agents are allowed (default for backward compatibility)
    pub fn with_allowed_agents(mut self, agents: Vec<String>) -> Self {
        self.allowed_agents = if agents.is_empty() {
            None
        } else {
            Some(agents)
        };
        self
    }

    /// Restrict reverse tunnel access to specific target addresses
    /// Format: ["host:port", "192.168.1.100:8080"]
    /// If not called or empty Vec, all addresses are allowed (default for backward compatibility)
    pub fn with_allowed_addresses(mut self, addresses: Vec<String>) -> Self {
        self.allowed_addresses = if addresses.is_empty() {
            None
        } else {
            Some(addresses)
        };
        self
    }

    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.exp
    }

    pub fn exp_formatted(&self) -> String {
        use chrono::{DateTime, Local};
        let dt = DateTime::<Utc>::from_timestamp(self.exp, 0).unwrap_or_else(Utc::now);
        let local: DateTime<Local> = dt.into();
        local.format("%Y-%m-%d %H:%M:%S %Z").to_string()
    }

    /// Validate reverse tunnel access for a specific agent and target address
    ///
    /// Returns Ok(()) if access is allowed, Err(String) with error message otherwise.
    ///
    /// # Arguments
    /// * `agent_id` - The agent ID client wants to connect to
    /// * `remote_address` - The target address client wants to access (format: "host:port")
    ///
    /// # Backward Compatibility
    /// - If `reverse_tunnel` is None, assume allowed (for existing tokens)
    /// - If `allowed_agents` is None/empty, all agents are allowed
    /// - If `allowed_addresses` is None/empty, all addresses are allowed
    ///
    /// # Examples
    ///
    /// ```rust
    /// use localup_auth::JwtClaims;
    /// use chrono::Duration;
    ///
    /// // Permissive token (all reverse tunnels allowed)
    /// let claims = JwtClaims::new(
    ///     "client-1".to_string(),
    ///     "issuer".to_string(),
    ///     "audience".to_string(),
    ///     Duration::hours(1),
    /// ).with_reverse_tunnel(true);
    ///
    /// assert!(claims.validate_reverse_localup_access("agent-1", "192.168.1.100:8080").is_ok());
    ///
    /// // Restrictive token (specific agent and addresses only)
    /// let claims = JwtClaims::new(
    ///     "client-2".to_string(),
    ///     "issuer".to_string(),
    ///     "audience".to_string(),
    ///     Duration::hours(1),
    /// )
    /// .with_reverse_tunnel(true)
    /// .with_allowed_agents(vec!["agent-1".to_string()])
    /// .with_allowed_addresses(vec!["192.168.1.100:8080".to_string()]);
    ///
    /// assert!(claims.validate_reverse_localup_access("agent-1", "192.168.1.100:8080").is_ok());
    /// assert!(claims.validate_reverse_localup_access("agent-2", "192.168.1.100:8080").is_err());
    /// assert!(claims.validate_reverse_localup_access("agent-1", "192.168.1.200:8080").is_err());
    /// ```
    pub fn validate_reverse_localup_access(
        &self,
        agent_id: &str,
        remote_address: &str,
    ) -> Result<(), String> {
        // Check if reverse tunnel is explicitly disabled
        if let Some(false) = self.reverse_tunnel {
            return Err("Reverse tunnel access is not allowed for this token".to_string());
        }

        // Check agent ID restriction (if specified)
        if let Some(ref allowed_agents) = self.allowed_agents {
            if !allowed_agents.is_empty() && !allowed_agents.contains(&agent_id.to_string()) {
                return Err(format!(
                    "Access denied: agent '{}' is not in allowed agents list",
                    agent_id
                ));
            }
        }

        // Check address restriction (if specified)
        if let Some(ref allowed_addresses) = self.allowed_addresses {
            if !allowed_addresses.is_empty()
                && !allowed_addresses.contains(&remote_address.to_string())
            {
                return Err(format!(
                    "Access denied: address '{}' is not in allowed addresses list",
                    remote_address
                ));
            }
        }

        // All checks passed
        Ok(())
    }
}

/// JWT errors
#[derive(Debug, Error)]
pub enum JwtError {
    #[error("JWT encoding error: {0}")]
    EncodingError(#[from] jsonwebtoken::errors::Error),

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid token")]
    InvalidToken,
}

/// JWT validator
pub struct JwtValidator {
    decoding_key: DecodingKey,
    validation: Validation,
}

impl JwtValidator {
    /// Create a new JWT validator using HMAC-SHA256 (symmetric secret)
    ///
    /// Validates ONLY:
    /// - Signature verification (using the secret)
    /// - Token expiration
    ///
    /// Does NOT validate:
    /// - Issuer claim
    /// - Audience claim
    /// - Not-before claim
    /// - Any other claims
    pub fn new(secret: &[u8]) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        // Only validate expiration - skip all other claims
        validation.validate_exp = true;
        validation.validate_aud = false;
        validation.validate_nbf = false;
        // Note: Issuer validation is disabled by default (only enabled if set_issuer() is called)

        Self {
            decoding_key: DecodingKey::from_secret(secret),
            validation,
        }
    }

    /// Create a new JWT validator using RSA public key (asymmetric)
    ///
    /// The public key should be in PEM format (begins with "-----BEGIN PUBLIC KEY-----")
    ///
    /// Validates ONLY:
    /// - Signature verification (using the public key)
    /// - Token expiration
    ///
    /// Does NOT validate:
    /// - Issuer claim
    /// - Audience claim
    /// - Not-before claim
    /// - Any other claims
    pub fn from_rsa_pem(public_key_pem: &[u8]) -> Result<Self, JwtError> {
        let mut validation = Validation::new(Algorithm::RS256);
        // Only validate expiration - skip all other claims
        validation.validate_exp = true;
        validation.validate_aud = false;
        validation.validate_nbf = false;
        // Note: Issuer validation is disabled by default (only enabled if set_issuer() is called)

        Ok(Self {
            decoding_key: DecodingKey::from_rsa_pem(public_key_pem)
                .map_err(JwtError::EncodingError)?,
            validation,
        })
    }

    pub fn with_audience(mut self, audience: String) -> Self {
        self.validation.set_audience(&[audience]);
        self
    }

    pub fn with_issuer(mut self, issuer: String) -> Self {
        self.validation.set_issuer(&[issuer]);
        self
    }

    pub fn validate(&self, token: &str) -> Result<JwtClaims, JwtError> {
        let token_data = decode::<JwtClaims>(token, &self.decoding_key, &self.validation)?;

        if token_data.claims.is_expired() {
            return Err(JwtError::TokenExpired);
        }

        Ok(token_data.claims)
    }

    /// Encode JWT using HMAC-SHA256 (symmetric secret)
    pub fn encode(secret: &[u8], claims: &JwtClaims) -> Result<String, JwtError> {
        let header = Header::new(Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(secret);

        Ok(encode(&header, claims, &encoding_key)?)
    }

    /// Encode JWT using RSA private key (asymmetric)
    ///
    /// The private key should be in PEM format (begins with "-----BEGIN RSA PRIVATE KEY-----")
    pub fn encode_rsa(private_key_pem: &[u8], claims: &JwtClaims) -> Result<String, JwtError> {
        let header = Header::new(Algorithm::RS256);
        let encoding_key =
            EncodingKey::from_rsa_pem(private_key_pem).map_err(JwtError::EncodingError)?;

        Ok(encode(&header, claims, &encoding_key)?)
    }
}

/// Implement AuthValidator trait for JwtValidator
#[async_trait]
impl AuthValidator for JwtValidator {
    async fn validate(&self, token: &str) -> Result<AuthResult, AuthError> {
        // Validate JWT using existing method
        let claims = self.validate(token).map_err(|e| match e {
            JwtError::TokenExpired => AuthError::TokenExpired,
            JwtError::InvalidToken => AuthError::InvalidToken("Invalid JWT".to_string()),
            JwtError::EncodingError(e) => AuthError::AuthenticationFailed(e.to_string()),
        })?;

        // Convert JWT claims to AuthResult
        let mut result = AuthResult::new(claims.sub.clone())
            .with_protocols(claims.protocols.clone())
            .with_regions(claims.regions.clone());

        // Add issuer and audience as metadata
        result = result
            .with_metadata("iss".to_string(), claims.iss.clone())
            .with_metadata("aud".to_string(), claims.aud.clone())
            .with_metadata("exp".to_string(), claims.exp.to_string());

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &[u8] = b"test_secret_key_1234567890";

    #[test]
    fn test_jwt_encode_decode() {
        let claims = JwtClaims::new(
            "localup-123".to_string(),
            "test-issuer".to_string(),
            "test-audience".to_string(),
            Duration::hours(1),
        );

        let token = JwtValidator::encode(TEST_SECRET, &claims).unwrap();

        let validator = JwtValidator::new(TEST_SECRET)
            .with_issuer("test-issuer".to_string())
            .with_audience("test-audience".to_string());

        let decoded_claims = validator.validate(&token).unwrap();

        assert_eq!(decoded_claims.sub, claims.sub);
        assert_eq!(decoded_claims.iss, claims.iss);
        assert_eq!(decoded_claims.aud, claims.aud);
    }

    #[test]
    fn test_jwt_with_protocols() {
        let claims = JwtClaims::new(
            "localup-456".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_protocols(vec!["tcp".to_string(), "https".to_string()]);

        let token = JwtValidator::encode(TEST_SECRET, &claims).unwrap();

        let validator = JwtValidator::new(TEST_SECRET)
            .with_issuer("issuer".to_string())
            .with_audience("audience".to_string());
        let decoded = validator.validate(&token).unwrap();

        assert_eq!(decoded.protocols, vec!["tcp", "https"]);
    }

    #[test]
    fn test_expired_token() {
        let claims = JwtClaims::new(
            "localup-789".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::seconds(-10), // Already expired
        );

        assert!(claims.is_expired());

        let token = JwtValidator::encode(TEST_SECRET, &claims).unwrap();

        let validator = JwtValidator::new(TEST_SECRET);
        let result = validator.validate(&token);

        assert!(result.is_err());
    }

    // ==================== Reverse Tunnel Authorization Tests ====================

    #[test]
    fn test_reverse_localup_permissive_token() {
        // Permissive token - all reverse tunnels allowed
        let claims = JwtClaims::new(
            "client-1".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_reverse_tunnel(true);

        // Should allow any agent and any address
        assert!(claims
            .validate_reverse_localup_access("agent-1", "192.168.1.100:8080")
            .is_ok());
        assert!(claims
            .validate_reverse_localup_access("agent-2", "10.0.0.5:22")
            .is_ok());
        assert!(claims
            .validate_reverse_localup_access("any-agent", "any-host:9999")
            .is_ok());
    }

    #[test]
    fn test_reverse_localup_restrictive_agent() {
        // Restrict to specific agents only
        let claims = JwtClaims::new(
            "client-2".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_reverse_tunnel(true)
        .with_allowed_agents(vec!["agent-1".to_string(), "agent-2".to_string()]);

        // Allowed agents
        assert!(claims
            .validate_reverse_localup_access("agent-1", "192.168.1.100:8080")
            .is_ok());
        assert!(claims
            .validate_reverse_localup_access("agent-2", "10.0.0.5:22")
            .is_ok());

        // Disallowed agent
        let result = claims.validate_reverse_localup_access("agent-3", "192.168.1.100:8080");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("agent 'agent-3' is not in allowed agents list"));
    }

    #[test]
    fn test_reverse_localup_restrictive_address() {
        // Restrict to specific addresses only
        let claims = JwtClaims::new(
            "client-3".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_reverse_tunnel(true)
        .with_allowed_addresses(vec![
            "192.168.1.100:8080".to_string(),
            "10.0.0.5:22".to_string(),
        ]);

        // Allowed addresses
        assert!(claims
            .validate_reverse_localup_access("agent-1", "192.168.1.100:8080")
            .is_ok());
        assert!(claims
            .validate_reverse_localup_access("agent-2", "10.0.0.5:22")
            .is_ok());

        // Disallowed address
        let result = claims.validate_reverse_localup_access("agent-1", "192.168.1.200:8080");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("address '192.168.1.200:8080' is not in allowed addresses list"));
    }

    #[test]
    fn test_reverse_localup_fully_restrictive() {
        // Restrict both agents AND addresses
        let claims = JwtClaims::new(
            "client-4".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_reverse_tunnel(true)
        .with_allowed_agents(vec!["agent-1".to_string()])
        .with_allowed_addresses(vec!["192.168.1.100:8080".to_string()]);

        // Valid: allowed agent + allowed address
        assert!(claims
            .validate_reverse_localup_access("agent-1", "192.168.1.100:8080")
            .is_ok());

        // Invalid: wrong agent
        assert!(claims
            .validate_reverse_localup_access("agent-2", "192.168.1.100:8080")
            .is_err());

        // Invalid: wrong address
        assert!(claims
            .validate_reverse_localup_access("agent-1", "10.0.0.5:22")
            .is_err());

        // Invalid: both wrong
        assert!(claims
            .validate_reverse_localup_access("agent-2", "10.0.0.5:22")
            .is_err());
    }

    #[test]
    fn test_reverse_localup_explicitly_disabled() {
        // Explicitly disable reverse tunnels
        let claims = JwtClaims::new(
            "client-5".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_reverse_tunnel(false);

        let result = claims.validate_reverse_localup_access("agent-1", "192.168.1.100:8080");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Reverse tunnel access is not allowed"));
    }

    #[test]
    fn test_reverse_localup_backward_compatibility() {
        // Old token without reverse_tunnel claim (None)
        // Should be allowed for backward compatibility
        let claims = JwtClaims::new(
            "client-6".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        );

        // reverse_tunnel should be None
        assert_eq!(claims.reverse_tunnel, None);

        // Should allow reverse tunnel access (backward compatible)
        assert!(claims
            .validate_reverse_localup_access("agent-1", "192.168.1.100:8080")
            .is_ok());
    }

    #[test]
    fn test_reverse_localup_empty_restrictions() {
        // Empty vectors should be treated as None (no restrictions)
        let claims = JwtClaims::new(
            "client-7".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_reverse_tunnel(true)
        .with_allowed_agents(vec![]) // Empty = all allowed
        .with_allowed_addresses(vec![]); // Empty = all allowed

        assert_eq!(claims.allowed_agents, None);
        assert_eq!(claims.allowed_addresses, None);

        // Should allow any agent and address
        assert!(claims
            .validate_reverse_localup_access("any-agent", "any-address:1234")
            .is_ok());
    }

    #[test]
    fn test_reverse_localup_encode_decode_with_restrictions() {
        // Test that reverse tunnel claims survive encode/decode
        let original_claims = JwtClaims::new(
            "client-8".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_reverse_tunnel(true)
        .with_allowed_agents(vec!["agent-1".to_string()])
        .with_allowed_addresses(vec!["192.168.1.100:8080".to_string()]);

        let token = JwtValidator::encode(TEST_SECRET, &original_claims).unwrap();

        let validator = JwtValidator::new(TEST_SECRET)
            .with_issuer("issuer".to_string())
            .with_audience("audience".to_string());

        let decoded_claims = validator.validate(&token).unwrap();

        // Verify all claims are preserved
        assert_eq!(decoded_claims.reverse_tunnel, Some(true));
        assert_eq!(
            decoded_claims.allowed_agents,
            Some(vec!["agent-1".to_string()])
        );
        assert_eq!(
            decoded_claims.allowed_addresses,
            Some(vec!["192.168.1.100:8080".to_string()])
        );

        // Verify validation works on decoded claims
        assert!(decoded_claims
            .validate_reverse_localup_access("agent-1", "192.168.1.100:8080")
            .is_ok());
        assert!(decoded_claims
            .validate_reverse_localup_access("agent-2", "192.168.1.100:8080")
            .is_err());
    }

    #[test]
    fn test_reverse_localup_skip_serialization_when_none() {
        // Test that None fields are not serialized (for backward compatibility)
        let claims = JwtClaims::new(
            "client-9".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        );

        // Serialize to JSON
        let json = serde_json::to_string(&claims).unwrap();

        // Should NOT contain reverse_tunnel, allowed_agents, or allowed_addresses
        assert!(!json.contains("reverse_tunnel"));
        assert!(!json.contains("allowed_agents"));
        assert!(!json.contains("allowed_addresses"));
    }
}
