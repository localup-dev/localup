//! ACME client for automatic certificate provisioning via Let's Encrypt
//!
//! NOTE: Full ACME implementation is in progress. For now, use manual certificate upload.

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

    #[error("Not implemented - use manual certificate upload for now")]
    NotImplemented,
}

/// HTTP-01 challenge data
#[derive(Debug, Clone)]
pub struct Http01Challenge {
    pub token: String,
    pub key_authorization: String,
}

/// Callback for HTTP-01 challenge validation
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
pub struct AcmeClient {
    #[allow(dead_code)]
    config: AcmeConfig,
}

impl AcmeClient {
    pub fn new(config: AcmeConfig) -> Self {
        Self { config }
    }

    /// Request a certificate for a domain using HTTP-01 challenge
    ///
    /// NOTE: This is not yet implemented. Use load_certificate_from_files() instead.
    pub async fn request_certificate(&self, _domain: &str) -> Result<Certificate, AcmeError> {
        Err(AcmeError::NotImplemented)
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

    /// Renew a certificate
    pub async fn renew_certificate(&self, _domain: &str) -> Result<Certificate, AcmeError> {
        Err(AcmeError::NotImplemented)
    }

    /// Validate domain name
    #[allow(dead_code)]
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

        Ok(())
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
    }

    #[tokio::test]
    async fn test_request_certificate_not_implemented() {
        let config = AcmeConfig::default();
        let client = AcmeClient::new(config);

        let result = client.request_certificate("example.com").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AcmeError::NotImplemented));
    }
}
