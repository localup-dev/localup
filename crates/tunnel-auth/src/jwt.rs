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
}

impl JwtClaims {
    pub fn new(tunnel_id: String, issuer: String, audience: String, validity: Duration) -> Self {
        let now = Utc::now();
        let exp = now + validity;

        Self {
            sub: tunnel_id,
            iat: now.timestamp(),
            exp: exp.timestamp(),
            iss: issuer,
            aud: audience,
            protocols: Vec::new(),
            regions: Vec::new(),
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

    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.exp
    }

    pub fn exp_formatted(&self) -> String {
        use chrono::{DateTime, Local};
        let dt = DateTime::<Utc>::from_timestamp(self.exp, 0).unwrap_or_else(Utc::now);
        let local: DateTime<Local> = dt.into();
        local.format("%Y-%m-%d %H:%M:%S %Z").to_string()
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
    pub fn new(secret: &[u8]) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        Self {
            decoding_key: DecodingKey::from_secret(secret),
            validation,
        }
    }

    /// Create a new JWT validator using RSA public key (asymmetric)
    ///
    /// The public key should be in PEM format (begins with "-----BEGIN PUBLIC KEY-----")
    pub fn from_rsa_pem(public_key_pem: &[u8]) -> Result<Self, JwtError> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.validate_exp = true;

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
            "tunnel-123".to_string(),
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
            "tunnel-456".to_string(),
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
            "tunnel-789".to_string(),
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
}
