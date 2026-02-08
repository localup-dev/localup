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
    /// Subject (tunnel ID or user ID depending on token_type)
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
    /// User ID who owns this token (for session and auth tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Team ID this token belongs to (optional, for team auth tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    /// User role in the system (admin, user)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_role: Option<String>,
    /// User role in the team (owner, admin, member)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_role: Option<String>,
    /// Token type: "session" (web UI) or "auth" (API key for tunnels)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_type: Option<String>,
    /// Custom: allowed subdomain patterns for HTTP/HTTPS tunnels.
    ///
    /// Supports glob patterns:
    /// - Exact match: "myapp" - only allows subdomain "myapp"
    /// - Prefix wildcard: "*-dviejo" - allows any subdomain ending with "-dviejo"
    /// - Suffix wildcard: "myapp-*" - allows any subdomain starting with "myapp-"
    /// - Full wildcard: "*" - allows any subdomain
    ///
    /// If None or empty, all subdomains are allowed (default for backward compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_subdomains: Option<Vec<String>>,
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
            user_id: None,
            team_id: None,
            user_role: None,
            team_role: None,
            token_type: None,
            allowed_subdomains: None,
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

    /// Set user ID who owns this token
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Set team ID this token belongs to
    pub fn with_team_id(mut self, team_id: String) -> Self {
        self.team_id = Some(team_id);
        self
    }

    /// Set user role (admin, user)
    pub fn with_user_role(mut self, role: String) -> Self {
        self.user_role = Some(role);
        self
    }

    /// Set team role (owner, admin, member)
    pub fn with_team_role(mut self, role: String) -> Self {
        self.team_role = Some(role);
        self
    }

    /// Set token type (session, auth)
    pub fn with_token_type(mut self, token_type: String) -> Self {
        self.token_type = Some(token_type);
        self
    }

    /// Restrict tunnel access to specific subdomain patterns.
    ///
    /// Supports glob patterns:
    /// - Exact match: "myapp" - only allows subdomain "myapp"
    /// - Prefix wildcard: "*-dviejo" - allows any subdomain ending with "-dviejo"
    /// - Suffix wildcard: "myapp-*" - allows any subdomain starting with "myapp-"
    /// - Full wildcard: "*" - allows any subdomain
    ///
    /// If not called or empty Vec, all subdomains are allowed (default for backward compatibility)
    pub fn with_allowed_subdomains(mut self, subdomains: Vec<String>) -> Self {
        self.allowed_subdomains = if subdomains.is_empty() {
            None
        } else {
            Some(subdomains)
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

    /// Validate subdomain access for HTTP/HTTPS tunnel creation
    ///
    /// Returns Ok(()) if access is allowed, Err(String) with error message otherwise.
    ///
    /// # Arguments
    /// * `subdomain` - The subdomain the client wants to use for their tunnel
    ///
    /// # Pattern Matching
    /// Supports glob patterns:
    /// - Exact match: "myapp" - only allows subdomain "myapp"
    /// - Prefix wildcard: "*-dviejo" - allows any subdomain ending with "-dviejo"
    /// - Suffix wildcard: "myapp-*" - allows any subdomain starting with "myapp-"
    /// - Full wildcard: "*" - allows any subdomain
    /// - Contains wildcard: "dev-*-test" - allows subdomains matching pattern
    ///
    /// # Backward Compatibility
    /// - If `allowed_subdomains` is None/empty, all subdomains are allowed
    ///
    /// # Examples
    ///
    /// ```rust
    /// use localup_auth::JwtClaims;
    /// use chrono::Duration;
    ///
    /// // Permissive token (all subdomains allowed)
    /// let claims = JwtClaims::new(
    ///     "client-1".to_string(),
    ///     "issuer".to_string(),
    ///     "audience".to_string(),
    ///     Duration::hours(1),
    /// );
    /// assert!(claims.validate_subdomain_access("any-subdomain").is_ok());
    ///
    /// // Restrictive token (specific patterns only)
    /// let claims = JwtClaims::new(
    ///     "client-2".to_string(),
    ///     "issuer".to_string(),
    ///     "audience".to_string(),
    ///     Duration::hours(1),
    /// )
    /// .with_allowed_subdomains(vec!["*-dviejo".to_string()]);
    ///
    /// assert!(claims.validate_subdomain_access("myapp-dviejo").is_ok());
    /// assert!(claims.validate_subdomain_access("test-dviejo").is_ok());
    /// assert!(claims.validate_subdomain_access("myapp").is_err()); // doesn't end with -dviejo
    /// ```
    pub fn validate_subdomain_access(&self, subdomain: &str) -> Result<(), String> {
        // If no restrictions, allow all subdomains
        let Some(ref allowed_subdomains) = self.allowed_subdomains else {
            return Ok(());
        };

        if allowed_subdomains.is_empty() {
            return Ok(());
        }

        // Check if subdomain matches any allowed pattern
        for pattern in allowed_subdomains {
            if Self::matches_subdomain_pattern(subdomain, pattern) {
                return Ok(());
            }
        }

        Err(format!(
            "Access denied: subdomain '{}' does not match allowed patterns {:?}",
            subdomain, allowed_subdomains
        ))
    }

    /// Check if a subdomain matches a glob pattern
    ///
    /// Supports:
    /// - Exact match: "myapp" matches only "myapp"
    /// - Prefix wildcard: "*-dviejo" matches "anything-dviejo"
    /// - Suffix wildcard: "myapp-*" matches "myapp-anything"
    /// - Full wildcard: "*" matches anything
    /// - Contains wildcard: "dev-*-test" matches "dev-anything-test"
    fn matches_subdomain_pattern(subdomain: &str, pattern: &str) -> bool {
        // Full wildcard matches everything
        if pattern == "*" {
            return true;
        }

        // No wildcards = exact match
        if !pattern.contains('*') {
            return subdomain == pattern;
        }

        // Split pattern by '*' and match each part
        let parts: Vec<&str> = pattern.split('*').collect();

        if parts.len() == 2 {
            // Single wildcard: prefix or suffix pattern
            let prefix = parts[0];
            let suffix = parts[1];

            // Check prefix match
            if !prefix.is_empty() && !subdomain.starts_with(prefix) {
                return false;
            }

            // Check suffix match
            if !suffix.is_empty() && !subdomain.ends_with(suffix) {
                return false;
            }

            // For patterns like "prefix-*-suffix", ensure middle part exists
            if !prefix.is_empty() && !suffix.is_empty() {
                let middle_start = prefix.len();
                let middle_end = subdomain.len().saturating_sub(suffix.len());
                if middle_start > middle_end {
                    return false;
                }
            }

            return true;
        }

        // Multiple wildcards: more complex matching
        // Use a simple greedy approach
        let mut remaining = subdomain;

        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }

            if i == 0 {
                // First part must be a prefix
                if !remaining.starts_with(part) {
                    return false;
                }
                remaining = &remaining[part.len()..];
            } else if i == parts.len() - 1 {
                // Last part must be a suffix
                if !remaining.ends_with(part) {
                    return false;
                }
            } else {
                // Middle parts must be found somewhere
                if let Some(pos) = remaining.find(part) {
                    remaining = &remaining[pos + part.len()..];
                } else {
                    return false;
                }
            }
        }

        true
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

    // ==================== Subdomain Access Authorization Tests ====================

    #[test]
    fn test_subdomain_access_permissive_token() {
        // Permissive token - all subdomains allowed (no restriction)
        let claims = JwtClaims::new(
            "client-1".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        );

        // Should allow any subdomain
        assert!(claims.validate_subdomain_access("myapp").is_ok());
        assert!(claims.validate_subdomain_access("test-dviejo").is_ok());
        assert!(claims.validate_subdomain_access("anything-at-all").is_ok());
    }

    #[test]
    fn test_subdomain_access_exact_match() {
        // Exact match pattern
        let claims = JwtClaims::new(
            "client-2".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["myapp".to_string()]);

        // Exact match should work
        assert!(claims.validate_subdomain_access("myapp").is_ok());

        // Other subdomains should be rejected
        let result = claims.validate_subdomain_access("otherapp");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("does not match allowed patterns"));
    }

    #[test]
    fn test_subdomain_access_prefix_wildcard() {
        // Suffix pattern: *-dviejo (matches anything ending with -dviejo)
        let claims = JwtClaims::new(
            "client-3".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["*-dviejo".to_string()]);

        // Should match subdomains ending with -dviejo
        assert!(claims.validate_subdomain_access("myapp-dviejo").is_ok());
        assert!(claims.validate_subdomain_access("test-dviejo").is_ok());
        assert!(claims.validate_subdomain_access("x-dviejo").is_ok());

        // Should not match other subdomains
        assert!(claims.validate_subdomain_access("myapp").is_err());
        assert!(claims.validate_subdomain_access("dviejo").is_err()); // no prefix
        assert!(claims.validate_subdomain_access("dviejo-test").is_err()); // wrong position
    }

    #[test]
    fn test_subdomain_access_suffix_wildcard() {
        // Prefix pattern: myapp-* (matches anything starting with myapp-)
        let claims = JwtClaims::new(
            "client-4".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["myapp-*".to_string()]);

        // Should match subdomains starting with myapp-
        assert!(claims.validate_subdomain_access("myapp-dev").is_ok());
        assert!(claims.validate_subdomain_access("myapp-prod").is_ok());
        assert!(claims.validate_subdomain_access("myapp-123").is_ok());

        // Should not match other subdomains
        assert!(claims.validate_subdomain_access("myapp").is_err());
        assert!(claims.validate_subdomain_access("otherapp-dev").is_err());
    }

    #[test]
    fn test_subdomain_access_full_wildcard() {
        // Full wildcard: * (matches anything)
        let claims = JwtClaims::new(
            "client-5".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["*".to_string()]);

        // Should match any subdomain
        assert!(claims.validate_subdomain_access("anything").is_ok());
        assert!(claims.validate_subdomain_access("myapp-dviejo").is_ok());
        assert!(claims.validate_subdomain_access("x").is_ok());
    }

    #[test]
    fn test_subdomain_access_contains_wildcard() {
        // Contains pattern: dev-*-test (matches dev-anything-test)
        let claims = JwtClaims::new(
            "client-6".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["dev-*-test".to_string()]);

        // Should match subdomains with pattern
        assert!(claims.validate_subdomain_access("dev-myapp-test").is_ok());
        assert!(claims.validate_subdomain_access("dev-123-test").is_ok());

        // Should not match other patterns
        assert!(claims.validate_subdomain_access("dev-test").is_err()); // no middle
        assert!(claims.validate_subdomain_access("dev-myapp").is_err()); // no -test
        assert!(claims.validate_subdomain_access("myapp-test").is_err()); // no dev-
    }

    #[test]
    fn test_subdomain_access_multiple_patterns() {
        // Multiple allowed patterns
        let claims = JwtClaims::new(
            "client-7".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec![
            "*-dviejo".to_string(),
            "api".to_string(),
            "test-*".to_string(),
        ]);

        // Should match any of the patterns
        assert!(claims.validate_subdomain_access("myapp-dviejo").is_ok()); // matches *-dviejo
        assert!(claims.validate_subdomain_access("api").is_ok()); // exact match
        assert!(claims.validate_subdomain_access("test-123").is_ok()); // matches test-*

        // Should not match if no pattern matches
        assert!(claims.validate_subdomain_access("myapp").is_err());
        assert!(claims.validate_subdomain_access("other").is_err());
    }

    #[test]
    fn test_subdomain_access_empty_restrictions() {
        // Empty vector should be treated as None (no restrictions)
        let claims = JwtClaims::new(
            "client-8".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec![]); // Empty = all allowed

        assert_eq!(claims.allowed_subdomains, None);

        // Should allow any subdomain
        assert!(claims.validate_subdomain_access("anything").is_ok());
    }

    #[test]
    fn test_subdomain_access_encode_decode() {
        // Test that subdomain claims survive encode/decode
        let original_claims = JwtClaims::new(
            "client-9".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["*-dviejo".to_string(), "api".to_string()]);

        let token = JwtValidator::encode(TEST_SECRET, &original_claims).unwrap();

        let validator = JwtValidator::new(TEST_SECRET)
            .with_issuer("issuer".to_string())
            .with_audience("audience".to_string());

        let decoded_claims = validator.validate(&token).unwrap();

        // Verify claims are preserved
        assert_eq!(
            decoded_claims.allowed_subdomains,
            Some(vec!["*-dviejo".to_string(), "api".to_string()])
        );

        // Verify validation works on decoded claims
        assert!(decoded_claims
            .validate_subdomain_access("myapp-dviejo")
            .is_ok());
        assert!(decoded_claims.validate_subdomain_access("api").is_ok());
        assert!(decoded_claims.validate_subdomain_access("other").is_err());
    }

    #[test]
    fn test_subdomain_skip_serialization_when_none() {
        // Test that None allowed_subdomains is not serialized
        let claims = JwtClaims::new(
            "client-10".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        );

        let json = serde_json::to_string(&claims).unwrap();

        // Should NOT contain allowed_subdomains
        assert!(!json.contains("allowed_subdomains"));
    }

    // ==================== Additional Edge Case Tests ====================

    #[test]
    fn test_subdomain_case_sensitivity() {
        // Patterns should be case-sensitive
        let claims = JwtClaims::new(
            "client-11".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["MyApp".to_string(), "*-Dviejo".to_string()]);

        // Exact case matches
        assert!(claims.validate_subdomain_access("MyApp").is_ok());
        assert!(claims.validate_subdomain_access("test-Dviejo").is_ok());

        // Different cases should not match
        assert!(claims.validate_subdomain_access("myapp").is_err());
        assert!(claims.validate_subdomain_access("MYAPP").is_err());
        assert!(claims.validate_subdomain_access("test-dviejo").is_err());
        assert!(claims.validate_subdomain_access("test-DVIEJO").is_err());
    }

    #[test]
    fn test_subdomain_single_character() {
        // Single character subdomains and patterns
        let claims = JwtClaims::new(
            "client-12".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec![
            "a".to_string(),
            "*-b".to_string(),
            "c-*".to_string(),
        ]);

        assert!(claims.validate_subdomain_access("a").is_ok());
        assert!(claims.validate_subdomain_access("x-b").is_ok());
        assert!(claims.validate_subdomain_access("c-y").is_ok());

        assert!(claims.validate_subdomain_access("b").is_err());
        assert!(claims.validate_subdomain_access("c").is_err());
    }

    #[test]
    fn test_subdomain_with_numbers() {
        // Subdomains with numbers
        let claims = JwtClaims::new(
            "client-13".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec![
            "app123".to_string(),
            "*-v2".to_string(),
            "pr-*".to_string(),
        ]);

        assert!(claims.validate_subdomain_access("app123").is_ok());
        assert!(claims.validate_subdomain_access("api-v2").is_ok());
        assert!(claims.validate_subdomain_access("pr-456").is_ok());
        assert!(claims
            .validate_subdomain_access("pr-feature-branch")
            .is_ok());

        assert!(claims.validate_subdomain_access("app124").is_err());
        assert!(claims.validate_subdomain_access("api-v3").is_err());
    }

    #[test]
    fn test_subdomain_hyphen_edge_cases() {
        // Edge cases with hyphens
        let claims = JwtClaims::new(
            "client-14".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["*-dviejo".to_string(), "app-*-test".to_string()]);

        // Single hyphen before suffix
        assert!(claims.validate_subdomain_access("-dviejo").is_ok());
        // Multiple hyphens
        assert!(claims.validate_subdomain_access("my-app-dviejo").is_ok());
        assert!(claims.validate_subdomain_access("a-b-c-dviejo").is_ok());

        // Contains pattern with hyphens
        assert!(claims.validate_subdomain_access("app-foo-test").is_ok());
        assert!(claims.validate_subdomain_access("app-foo-bar-test").is_ok());

        // Should not match without proper suffix/prefix
        assert!(claims.validate_subdomain_access("dviejo").is_err());
        assert!(claims.validate_subdomain_access("app-test").is_err());
    }

    #[test]
    fn test_subdomain_real_world_user_pattern() {
        // Real-world scenario: User-specific subdomains
        // Pattern: *-dviejo for user "dviejo"
        let claims = JwtClaims::new(
            "user-dviejo".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(24),
        )
        .with_allowed_subdomains(vec!["*-dviejo".to_string()]);

        // User can create any subdomain ending with their username
        assert!(claims.validate_subdomain_access("myapp-dviejo").is_ok());
        assert!(claims.validate_subdomain_access("frontend-dviejo").is_ok());
        assert!(claims.validate_subdomain_access("api-v2-dviejo").is_ok());
        assert!(claims
            .validate_subdomain_access("dev-feature-123-dviejo")
            .is_ok());

        // Cannot create subdomains for other users
        assert!(claims.validate_subdomain_access("myapp-smith").is_err());
        assert!(claims.validate_subdomain_access("myapp").is_err());
        assert!(claims.validate_subdomain_access("dviejo-myapp").is_err());
    }

    #[test]
    fn test_subdomain_real_world_team_pattern() {
        // Real-world scenario: Team-specific subdomains
        let claims = JwtClaims::new(
            "team-platform".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(24),
        )
        .with_allowed_subdomains(vec![
            "platform-*".to_string(), // Team prefix
            "*-staging".to_string(),  // Staging environments
            "*-prod".to_string(),     // Production environments
        ]);

        // Team can use their prefix
        assert!(claims.validate_subdomain_access("platform-api").is_ok());
        assert!(claims.validate_subdomain_access("platform-web").is_ok());

        // Team can use staging/prod suffixes
        assert!(claims.validate_subdomain_access("myapp-staging").is_ok());
        assert!(claims.validate_subdomain_access("myapp-prod").is_ok());

        // Cannot use other team prefixes
        assert!(claims.validate_subdomain_access("backend-api").is_err());
        assert!(claims.validate_subdomain_access("myapp-dev").is_err());
    }

    #[test]
    fn test_subdomain_real_world_environment_pattern() {
        // Real-world scenario: Environment-specific tokens
        let claims = JwtClaims::new(
            "ci-pipeline".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec![
            "pr-*".to_string(),     // Pull request previews
            "branch-*".to_string(), // Branch previews
            "commit-*".to_string(), // Commit previews
        ]);

        assert!(claims.validate_subdomain_access("pr-123").is_ok());
        assert!(claims.validate_subdomain_access("pr-456-preview").is_ok());
        assert!(claims
            .validate_subdomain_access("branch-feature-login")
            .is_ok());
        assert!(claims.validate_subdomain_access("commit-abc123").is_ok());

        // Production subdomains not allowed
        assert!(claims.validate_subdomain_access("api").is_err());
        assert!(claims.validate_subdomain_access("www").is_err());
        assert!(claims.validate_subdomain_access("staging").is_err());
    }

    #[test]
    fn test_subdomain_overlapping_patterns() {
        // Patterns that could overlap
        let claims = JwtClaims::new(
            "client-15".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec![
            "api".to_string(),   // Exact
            "api-*".to_string(), // Prefix
            "*-api".to_string(), // Suffix
        ]);

        // All should match
        assert!(claims.validate_subdomain_access("api").is_ok());
        assert!(claims.validate_subdomain_access("api-v2").is_ok());
        assert!(claims.validate_subdomain_access("internal-api").is_ok());
        assert!(claims.validate_subdomain_access("api-api").is_ok()); // matches both patterns

        // Should not match
        assert!(claims.validate_subdomain_access("apis").is_err());
        assert!(claims.validate_subdomain_access("myapi").is_err());
    }

    #[test]
    fn test_subdomain_pattern_with_dots() {
        // Subdomains shouldn't have dots, but test behavior anyway
        let claims = JwtClaims::new(
            "client-16".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["*-test".to_string()]);

        // Dots in subdomain (unusual but test pattern matching)
        assert!(claims.validate_subdomain_access("app.v1-test").is_ok());
        assert!(claims.validate_subdomain_access("my.app-test").is_ok());
    }

    #[test]
    fn test_subdomain_empty_string() {
        // Empty subdomain should be handled gracefully
        let claims = JwtClaims::new(
            "client-17".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["*".to_string()]);

        // Empty string with wildcard
        assert!(claims.validate_subdomain_access("").is_ok());

        // With specific pattern
        let claims2 = JwtClaims::new(
            "client-17b".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["*-test".to_string()]);

        // Empty string should not match suffix pattern
        assert!(claims2.validate_subdomain_access("").is_err());
    }

    #[test]
    fn test_subdomain_pattern_only_wildcard_suffix() {
        // Pattern like "-*" (starts with hyphen, anything after)
        let claims = JwtClaims::new(
            "client-18".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["-*".to_string()]);

        assert!(claims.validate_subdomain_access("-test").is_ok());
        assert!(claims.validate_subdomain_access("-").is_ok());

        assert!(claims.validate_subdomain_access("test").is_err());
        assert!(claims.validate_subdomain_access("test-").is_err());
    }

    #[test]
    fn test_subdomain_long_pattern() {
        // Long subdomain names
        let claims = JwtClaims::new(
            "client-19".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec![
            "*-preview-environment".to_string(),
            "very-long-subdomain-name-for-testing".to_string(),
        ]);

        assert!(claims
            .validate_subdomain_access("my-app-preview-environment")
            .is_ok());
        assert!(claims
            .validate_subdomain_access("feature-123-preview-environment")
            .is_ok());
        assert!(claims
            .validate_subdomain_access("very-long-subdomain-name-for-testing")
            .is_ok());

        assert!(claims
            .validate_subdomain_access("preview-environment")
            .is_err());
        assert!(claims
            .validate_subdomain_access("very-long-subdomain-name")
            .is_err());
    }

    #[test]
    fn test_subdomain_unicode() {
        // Unicode characters in subdomains (IDN-style, though not recommended)
        let claims = JwtClaims::new(
            "client-20".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["*-тест".to_string(), "приложение".to_string()]);

        assert!(claims.validate_subdomain_access("app-тест").is_ok());
        assert!(claims.validate_subdomain_access("приложение").is_ok());

        assert!(claims.validate_subdomain_access("app-test").is_err());
    }

    #[test]
    fn test_matches_subdomain_pattern_directly() {
        // Direct tests of the pattern matching function
        assert!(JwtClaims::matches_subdomain_pattern("test", "test"));
        assert!(JwtClaims::matches_subdomain_pattern("anything", "*"));
        assert!(JwtClaims::matches_subdomain_pattern("foo-bar", "*-bar"));
        assert!(JwtClaims::matches_subdomain_pattern("foo-bar", "foo-*"));
        assert!(JwtClaims::matches_subdomain_pattern(
            "foo-baz-bar",
            "foo-*-bar"
        ));

        assert!(!JwtClaims::matches_subdomain_pattern("test", "other"));
        assert!(!JwtClaims::matches_subdomain_pattern("foo", "*-bar"));
        assert!(!JwtClaims::matches_subdomain_pattern("bar", "foo-*"));
        assert!(!JwtClaims::matches_subdomain_pattern(
            "foo-bar",
            "foo-*-bar"
        )); // no middle
    }

    #[test]
    fn test_subdomain_multiple_wildcards() {
        // Pattern with multiple wildcards: a-*-b-*-c
        let claims = JwtClaims::new(
            "client-21".to_string(),
            "issuer".to_string(),
            "audience".to_string(),
            Duration::hours(1),
        )
        .with_allowed_subdomains(vec!["a-*-b-*-c".to_string()]);

        assert!(claims.validate_subdomain_access("a-x-b-y-c").is_ok());
        assert!(claims.validate_subdomain_access("a-foo-b-bar-c").is_ok());
        assert!(claims.validate_subdomain_access("a-1-b-2-c").is_ok());

        // Missing parts
        assert!(claims.validate_subdomain_access("a-b-c").is_err());
        assert!(claims.validate_subdomain_access("a-x-b-c").is_err());
        assert!(claims.validate_subdomain_access("x-b-y-c").is_err());
    }
}
