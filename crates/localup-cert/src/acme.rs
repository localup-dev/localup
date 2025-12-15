//! ACME client for automatic certificate provisioning via Let's Encrypt
//!
//! Supports HTTP-01 challenges for domain validation.
//!
//! Note: Full ACME implementation requires instant-acme integration.
//! Current implementation provides the interface but returns NotImplemented
//! for actual ACME operations. Manual certificate upload is supported.

use std::sync::Arc;

use thiserror::Error;
use tokio::fs;
use tracing::info;

use crate::Certificate;

/// ACME errors
#[derive(Debug, Error)]
pub enum AcmeError {
    #[error("ACME error: {0}")]
    AcmeError(String),

    #[error("Account creation failed: {0}")]
    AccountCreationFailed(String),

    #[error("Order creation failed: {0}")]
    OrderCreationFailed(String),

    #[error("Challenge failed: {0}")]
    ChallengeFailed(String),

    #[error("Certificate finalization failed: {0}")]
    FinalizationFailed(String),

    #[error("Invalid domain: {0}")]
    InvalidDomain(String),

    #[error("Timeout waiting for order")]
    Timeout,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Certificate generation error: {0}")]
    CertGen(String),

    #[error("HTTP-01 challenge not supported")]
    Http01NotSupported,

    #[error("Authorization not found for domain: {0}")]
    AuthorizationNotFound(String),

    #[error("Challenge response callback failed")]
    ChallengeCallbackFailed,

    #[error("Challenge not ready: {0}")]
    ChallengeNotReady(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

/// HTTP-01 challenge data
#[derive(Debug, Clone)]
pub struct Http01Challenge {
    /// The token from the ACME server
    pub token: String,
    /// The key authorization (token.account_thumbprint)
    pub key_authorization: String,
    /// The domain being validated
    pub domain: String,
}

/// Challenge state for tracking
#[derive(Debug, Clone)]
pub struct ChallengeState {
    /// Challenge ID
    pub challenge_id: String,
    /// Domain being validated
    pub domain: String,
    /// Challenge type
    pub challenge_type: String,
    /// Challenge token (for HTTP-01)
    pub token: Option<String>,
    /// Key authorization (for HTTP-01)
    pub key_authorization: Option<String>,
    /// DNS record name (for DNS-01)
    pub dns_record_name: Option<String>,
    /// DNS record value (for DNS-01)
    pub dns_record_value: Option<String>,
    /// Expiration timestamp
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Callback for HTTP-01 challenge validation
/// Should return true if the challenge file was successfully served
pub type Http01ChallengeCallback = Arc<dyn Fn(Http01Challenge) -> bool + Send + Sync>;

/// ACME configuration
#[derive(Clone)]
pub struct AcmeConfig {
    /// Contact email for Let's Encrypt
    pub contact_email: String,
    /// Use Let's Encrypt staging environment (for testing)
    pub use_staging: bool,
    /// Directory to store certificates
    pub cert_dir: String,
    /// HTTP-01 challenge callback
    pub http01_callback: Option<Http01ChallengeCallback>,
}

impl std::fmt::Debug for AcmeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcmeConfig")
            .field("contact_email", &self.contact_email)
            .field("use_staging", &self.use_staging)
            .field("cert_dir", &self.cert_dir)
            .field("http01_callback", &self.http01_callback.is_some())
            .finish()
    }
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            contact_email: String::new(),
            use_staging: false,
            cert_dir: "./.certs".to_string(),
            http01_callback: None,
        }
    }
}

/// ACME client for certificate provisioning
///
/// Note: Full ACME integration with Let's Encrypt is planned but not yet implemented.
/// Use manual certificate upload (POST /api/domains) for now.
pub struct AcmeClient {
    config: AcmeConfig,
}

impl AcmeClient {
    /// Create a new ACME client
    pub fn new(config: AcmeConfig) -> Self {
        Self { config }
    }

    /// Initialize the ACME account
    pub async fn init(&mut self) -> Result<(), AcmeError> {
        // Ensure cert directory exists
        fs::create_dir_all(&self.config.cert_dir).await?;
        info!(
            "ACME client initialized (cert_dir: {})",
            self.config.cert_dir
        );
        Ok(())
    }

    /// Initiate a certificate order for a domain (step 1)
    /// Returns the challenge information that the caller must satisfy
    pub async fn initiate_order(&self, domain: &str) -> Result<ChallengeState, AcmeError> {
        Self::validate_domain(domain)?;

        // For now, return a mock challenge that indicates manual setup is needed
        // Full ACME implementation will be added in a future version
        let challenge_id = uuid::Uuid::new_v4().to_string();
        let token = format!("manual-{}", &challenge_id[..8]);

        info!(
            "Certificate request initiated for {} (manual verification required)",
            domain
        );

        Ok(ChallengeState {
            challenge_id,
            domain: domain.to_string(),
            challenge_type: "http-01".to_string(),
            token: Some(token.clone()),
            key_authorization: Some(format!("{}.placeholder-key-auth", token)),
            dns_record_name: None,
            dns_record_value: None,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        })
    }

    /// Complete the certificate order after challenge is satisfied (step 2)
    pub async fn complete_order(&self, domain: &str) -> Result<Certificate, AcmeError> {
        Self::validate_domain(domain)?;

        // Check if certificate files already exist (from manual upload)
        let cert_path = format!("{}/{}.crt", self.config.cert_dir, domain);
        let key_path = format!("{}/{}.key", self.config.cert_dir, domain);

        if self.certificate_exists(domain).await {
            // Load existing certificate
            return Self::load_certificate_from_files(&cert_path, &key_path).await;
        }

        // Full ACME not implemented yet
        Err(AcmeError::NotImplemented(
            "Automatic Let's Encrypt certificate provisioning is not yet implemented. \
            Please upload your certificate manually via POST /api/domains."
                .to_string(),
        ))
    }

    /// Load certificate from PEM files
    pub async fn load_certificate_from_files(
        cert_path: &str,
        key_path: &str,
    ) -> Result<Certificate, AcmeError> {
        let cert_pem = fs::read(cert_path).await?;
        let key_pem = fs::read(key_path).await?;

        // Parse certificate chain
        let cert_chain = rustls_pemfile::certs(&mut cert_pem.as_slice())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AcmeError::CertGen(format!("Failed to parse certificate: {}", e)))?;

        // Parse private key
        let private_key = rustls_pemfile::private_key(&mut key_pem.as_slice())
            .map_err(|e| AcmeError::CertGen(format!("Failed to parse private key: {}", e)))?
            .ok_or_else(|| AcmeError::CertGen("No private key found in file".to_string()))?;

        info!("Certificate loaded from {} and {}", cert_path, key_path);

        Ok(Certificate {
            cert_chain,
            private_key,
        })
    }

    /// Validate domain name
    fn validate_domain(domain: &str) -> Result<(), AcmeError> {
        if domain.is_empty() {
            return Err(AcmeError::InvalidDomain(
                "Domain cannot be empty".to_string(),
            ));
        }

        if domain.contains(' ') {
            return Err(AcmeError::InvalidDomain(
                "Domain cannot contain spaces".to_string(),
            ));
        }

        if domain.starts_with('.') || domain.ends_with('.') {
            return Err(AcmeError::InvalidDomain(
                "Domain cannot start or end with a dot".to_string(),
            ));
        }

        // Check for wildcard (not supported with HTTP-01)
        if domain.starts_with('*') {
            return Err(AcmeError::InvalidDomain(
                "Wildcard domains require DNS-01 challenge (not yet supported)".to_string(),
            ));
        }

        Ok(())
    }

    /// Get the certificate directory path
    pub fn cert_dir(&self) -> &str {
        &self.config.cert_dir
    }

    /// Check if a certificate exists for a domain
    pub async fn certificate_exists(&self, domain: &str) -> bool {
        let cert_path = format!("{}/{}.crt", self.config.cert_dir, domain);
        let key_path = format!("{}/{}.key", self.config.cert_dir, domain);

        fs::metadata(&cert_path).await.is_ok() && fs::metadata(&key_path).await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acme_config() {
        let config = AcmeConfig {
            contact_email: "admin@example.com".to_string(),
            use_staging: true,
            cert_dir: "/tmp/certs".to_string(),
            http01_callback: None,
        };

        assert_eq!(config.contact_email, "admin@example.com");
        assert!(config.use_staging);
    }

    #[test]
    fn test_validate_domain() {
        assert!(AcmeClient::validate_domain("example.com").is_ok());
        assert!(AcmeClient::validate_domain("sub.example.com").is_ok());
        assert!(AcmeClient::validate_domain("").is_err());
        assert!(AcmeClient::validate_domain("invalid domain.com").is_err());
        assert!(AcmeClient::validate_domain(".example.com").is_err());
        assert!(AcmeClient::validate_domain("example.com.").is_err());
        assert!(AcmeClient::validate_domain("*.example.com").is_err());
    }

    #[tokio::test]
    async fn test_acme_client_new() {
        let config = AcmeConfig::default();
        let _client = AcmeClient::new(config);
    }

    #[tokio::test]
    async fn test_certificate_exists() {
        let config = AcmeConfig {
            cert_dir: "/tmp/nonexistent_certs_dir_12345".to_string(),
            ..Default::default()
        };
        let client = AcmeClient::new(config);
        assert!(!client.certificate_exists("example.com").await);
    }
}
