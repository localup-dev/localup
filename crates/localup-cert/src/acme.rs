//! ACME client for automatic certificate provisioning via Let's Encrypt
//!
//! Supports both HTTP-01 and DNS-01 challenges for domain validation.
//!
//! - HTTP-01: Requires serving a file at /.well-known/acme-challenge/{token}
//! - DNS-01: Requires adding a TXT record at _acme-challenge.{domain}

use std::collections::HashMap;
use std::sync::Arc;

use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt,
    NewAccount, NewOrder, OrderStatus,
};
use thiserror::Error;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

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

    #[error("HTTP-01 challenge not supported for this domain")]
    Http01NotSupported,

    #[error("DNS-01 challenge not supported for this domain")]
    Dns01NotSupported,

    #[error("Authorization not found for domain: {0}")]
    AuthorizationNotFound(String),

    #[error("Challenge response callback failed")]
    ChallengeCallbackFailed,

    #[error("Challenge not ready: {0}")]
    ChallengeNotReady(String),

    #[error("Order not found: {0}")]
    OrderNotFound(String),

    #[error("Account not initialized")]
    AccountNotInitialized,
}

/// Challenge type enum for API
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcmeChallengeType {
    /// HTTP-01 challenge - serve file at /.well-known/acme-challenge/{token}
    Http01,
    /// DNS-01 challenge - add TXT record at _acme-challenge.{domain}
    Dns01,
}

impl std::fmt::Display for AcmeChallengeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcmeChallengeType::Http01 => write!(f, "http-01"),
            AcmeChallengeType::Dns01 => write!(f, "dns-01"),
        }
    }
}

/// HTTP-01 challenge data
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Http01Challenge {
    /// The token from the ACME server
    pub token: String,
    /// The key authorization (token.account_thumbprint)
    pub key_authorization: String,
    /// The domain being validated
    pub domain: String,
    /// The URL path to serve the challenge at
    pub url_path: String,
}

/// DNS-01 challenge data
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Dns01Challenge {
    /// The domain being validated
    pub domain: String,
    /// The DNS record name (e.g., _acme-challenge.example.com)
    pub record_name: String,
    /// The DNS record type (always TXT)
    pub record_type: String,
    /// The DNS record value (base64url-encoded SHA256 of key authorization)
    pub record_value: String,
}

/// Challenge state for tracking
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChallengeState {
    /// Unique order ID for this challenge
    pub order_id: String,
    /// Domain being validated
    pub domain: String,
    /// Challenge type
    pub challenge_type: AcmeChallengeType,
    /// HTTP-01 challenge details (if applicable)
    pub http01: Option<Http01Challenge>,
    /// DNS-01 challenge details (if applicable)
    pub dns01: Option<Dns01Challenge>,
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
    /// Directory to store certificates and account credentials
    pub cert_dir: String,
    /// HTTP-01 challenge callback (optional - for automatic challenge response)
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

/// Stored order state (for multi-step challenge flow)
struct StoredOrder {
    /// The order URL for refreshing
    order_url: String,
    /// Domain being validated
    domain: String,
    /// Challenge type
    challenge_type: AcmeChallengeType,
}

/// ACME client for certificate provisioning via Let's Encrypt
///
/// Supports both HTTP-01 and DNS-01 challenges.
pub struct AcmeClient {
    config: AcmeConfig,
    account: Option<Account>,
    /// Pending orders keyed by order_id
    pending_orders: RwLock<HashMap<String, StoredOrder>>,
}

impl AcmeClient {
    /// Create a new ACME client
    pub fn new(config: AcmeConfig) -> Self {
        Self {
            config,
            account: None,
            pending_orders: RwLock::new(HashMap::new()),
        }
    }

    /// Initialize the ACME client and create/load account
    pub async fn init(&mut self) -> Result<(), AcmeError> {
        // Ensure cert directory exists
        fs::create_dir_all(&self.config.cert_dir).await?;

        // Try to load existing account credentials
        let account_path = format!("{}/account.json", self.config.cert_dir);

        let account = if let Ok(creds_json) = fs::read_to_string(&account_path).await {
            // Load existing account
            let creds: AccountCredentials = serde_json::from_str(&creds_json).map_err(|e| {
                AcmeError::AccountCreationFailed(format!(
                    "Failed to parse account credentials: {}",
                    e
                ))
            })?;

            let account = Account::builder()
                .map_err(|e| AcmeError::AccountCreationFailed(e.to_string()))?
                .from_credentials(creds)
                .await
                .map_err(|e| AcmeError::AccountCreationFailed(e.to_string()))?;

            info!("ACME account loaded from {}", account_path);
            account
        } else {
            // Create new account
            let directory_url = if self.config.use_staging {
                info!("Using Let's Encrypt STAGING environment");
                LetsEncrypt::Staging.url().to_string()
            } else {
                info!("Using Let's Encrypt PRODUCTION environment");
                LetsEncrypt::Production.url().to_string()
            };

            let (account, creds) = Account::builder()
                .map_err(|e| AcmeError::AccountCreationFailed(e.to_string()))?
                .create(
                    &NewAccount {
                        contact: &[&format!("mailto:{}", self.config.contact_email)],
                        terms_of_service_agreed: true,
                        only_return_existing: false,
                    },
                    directory_url,
                    None,
                )
                .await
                .map_err(|e| AcmeError::AccountCreationFailed(e.to_string()))?;

            // Save account credentials
            let creds_json = serde_json::to_string_pretty(&creds).map_err(|e| {
                AcmeError::AccountCreationFailed(format!(
                    "Failed to serialize account credentials: {}",
                    e
                ))
            })?;
            fs::write(&account_path, creds_json).await?;

            info!("ACME account created and saved to {}", account_path);
            account
        };

        self.account = Some(account);

        info!(
            "ACME client initialized (cert_dir: {}, staging: {})",
            self.config.cert_dir, self.config.use_staging
        );

        Ok(())
    }

    /// Initiate a certificate order for a domain
    ///
    /// Returns challenge information that must be satisfied before calling complete_order.
    /// For HTTP-01: serve the key_authorization at /.well-known/acme-challenge/{token}
    /// For DNS-01: create a TXT record at _acme-challenge.{domain} with the record_value
    pub async fn initiate_order(
        &self,
        domain: &str,
        challenge_type: AcmeChallengeType,
    ) -> Result<ChallengeState, AcmeError> {
        Self::validate_domain(domain, challenge_type)?;

        let account = self
            .account
            .as_ref()
            .ok_or(AcmeError::AccountNotInitialized)?;

        // Create the order
        let identifiers = [Identifier::Dns(domain.to_string())];
        let new_order = NewOrder::new(&identifiers);
        let mut order = account
            .new_order(&new_order)
            .await
            .map_err(|e| AcmeError::OrderCreationFailed(e.to_string()))?;

        // Get the order URL before consuming authorizations
        let order_url = order.url().to_string();

        // Get authorizations
        let mut authorizations = order.authorizations();
        let mut authz_handle = authorizations
            .next()
            .await
            .ok_or_else(|| AcmeError::AuthorizationNotFound(domain.to_string()))?
            .map_err(|e| {
                AcmeError::OrderCreationFailed(format!("Failed to get authorization: {}", e))
            })?;

        // Check authorization status
        match authz_handle.status {
            AuthorizationStatus::Valid => {
                info!("Domain {} is already authorized", domain);
            }
            AuthorizationStatus::Pending => {
                debug!("Domain {} authorization is pending", domain);
            }
            other => {
                return Err(AcmeError::ChallengeFailed(format!(
                    "Authorization status is {:?}",
                    other
                )));
            }
        }

        // Find the appropriate challenge
        let acme_challenge_type = match challenge_type {
            AcmeChallengeType::Http01 => ChallengeType::Http01,
            AcmeChallengeType::Dns01 => ChallengeType::Dns01,
        };

        let challenge_handle =
            authz_handle
                .challenge(acme_challenge_type)
                .ok_or(match challenge_type {
                    AcmeChallengeType::Http01 => AcmeError::Http01NotSupported,
                    AcmeChallengeType::Dns01 => AcmeError::Dns01NotSupported,
                })?;

        // Get key authorization
        let key_auth = challenge_handle.key_authorization();
        let key_auth_str = key_auth.as_str().to_string();
        let dns_value = key_auth.dns_value();

        // Get challenge details
        let token = challenge_handle.token.clone();

        // Generate a unique order ID
        let order_id = uuid::Uuid::new_v4().to_string();

        // Build challenge state based on type
        let (http01, dns01) = match challenge_type {
            AcmeChallengeType::Http01 => {
                let http01_challenge = Http01Challenge {
                    token: token.clone(),
                    key_authorization: key_auth_str.clone(),
                    domain: domain.to_string(),
                    url_path: format!("/.well-known/acme-challenge/{}", token),
                };
                (Some(http01_challenge), None)
            }
            AcmeChallengeType::Dns01 => {
                let dns01_challenge = Dns01Challenge {
                    domain: domain.to_string(),
                    record_name: format!("_acme-challenge.{}", domain.trim_start_matches("*.")),
                    record_type: "TXT".to_string(),
                    record_value: dns_value.clone(),
                };
                (None, Some(dns01_challenge))
            }
        };

        let challenge_state = ChallengeState {
            order_id: order_id.clone(),
            domain: domain.to_string(),
            challenge_type,
            http01,
            dns01,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        };

        // Store the order for later completion
        let stored_order = StoredOrder {
            order_url,
            domain: domain.to_string(),
            challenge_type,
        };

        self.pending_orders
            .write()
            .await
            .insert(order_id.clone(), stored_order);

        info!(
            "ACME order {} initiated for {} using {:?} challenge",
            order_id, domain, challenge_type
        );

        Ok(challenge_state)
    }

    /// Complete the certificate order after challenge has been satisfied
    ///
    /// Call this after you've set up the HTTP-01 or DNS-01 challenge response.
    pub async fn complete_order(&self, order_id: &str) -> Result<Certificate, AcmeError> {
        let account = self
            .account
            .as_ref()
            .ok_or(AcmeError::AccountNotInitialized)?;

        // Get the stored order
        let stored_order = self
            .pending_orders
            .write()
            .await
            .remove(order_id)
            .ok_or_else(|| AcmeError::OrderNotFound(order_id.to_string()))?;

        let domain = &stored_order.domain;

        // Restore the order from URL
        let mut order = account
            .order(stored_order.order_url.clone())
            .await
            .map_err(|e| AcmeError::ChallengeFailed(format!("Failed to restore order: {}", e)))?;

        // Get the authorization and challenge again to set ready
        let mut authorizations = order.authorizations();
        if let Some(authz_result) = authorizations.next().await {
            let mut authz_handle = authz_result
                .map_err(|e| AcmeError::ChallengeFailed(format!("Failed to get authz: {}", e)))?;

            let acme_challenge_type = match stored_order.challenge_type {
                AcmeChallengeType::Http01 => ChallengeType::Http01,
                AcmeChallengeType::Dns01 => ChallengeType::Dns01,
            };

            if let Some(mut challenge_handle) = authz_handle.challenge(acme_challenge_type) {
                // Tell ACME server we're ready for the challenge
                challenge_handle.set_ready().await.map_err(|e| {
                    AcmeError::ChallengeFailed(format!("Failed to set challenge ready: {}", e))
                })?;
            }
        }

        // Wait for the order to be ready using polling
        let retry_policy = instant_acme::RetryPolicy::new()
            .timeout(std::time::Duration::from_secs(60))
            .initial_delay(std::time::Duration::from_secs(2));

        let status = order.poll_ready(&retry_policy).await.map_err(|e| {
            AcmeError::ChallengeFailed(format!("Challenge verification failed: {}", e))
        })?;

        match status {
            OrderStatus::Ready => {
                info!("Order {} is ready for finalization", order_id);
            }
            OrderStatus::Invalid => {
                return Err(AcmeError::ChallengeFailed(
                    "Order became invalid - challenge verification failed".to_string(),
                ));
            }
            other => {
                return Err(AcmeError::ChallengeFailed(format!(
                    "Unexpected order status: {:?}",
                    other
                )));
            }
        }

        // Finalize the order - this generates the CSR and gets the certificate
        // Returns the private key PEM
        let private_key_pem = order.finalize().await.map_err(|e| {
            AcmeError::FinalizationFailed(format!("Failed to finalize order: {}", e))
        })?;

        // Get the certificate
        let cert_chain_pem = order.poll_certificate(&retry_policy).await.map_err(|e| {
            AcmeError::FinalizationFailed(format!("Failed to get certificate: {}", e))
        })?;

        // Save certificate and key to files
        let cert_path = format!("{}/{}.crt", self.config.cert_dir, domain);
        let key_path = format!("{}/{}.key", self.config.cert_dir, domain);

        fs::write(&cert_path, &cert_chain_pem).await?;
        fs::write(&key_path, &private_key_pem).await?;

        info!("Certificate saved to {} and {}", cert_path, key_path);

        // Parse and return the certificate
        Self::load_certificate_from_files(&cert_path, &key_path).await
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
    fn validate_domain(domain: &str, challenge_type: AcmeChallengeType) -> Result<(), AcmeError> {
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

        // Wildcard domains require DNS-01
        if domain.starts_with('*') && challenge_type == AcmeChallengeType::Http01 {
            return Err(AcmeError::InvalidDomain(
                "Wildcard domains require DNS-01 challenge".to_string(),
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

    /// Get certificate paths for a domain
    pub fn get_cert_paths(&self, domain: &str) -> (String, String) {
        let cert_path = format!("{}/{}.crt", self.config.cert_dir, domain);
        let key_path = format!("{}/{}.key", self.config.cert_dir, domain);
        (cert_path, key_path)
    }

    /// Check if using staging environment
    pub fn is_staging(&self) -> bool {
        self.config.use_staging
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
        assert!(AcmeClient::validate_domain("example.com", AcmeChallengeType::Http01).is_ok());
        assert!(AcmeClient::validate_domain("sub.example.com", AcmeChallengeType::Http01).is_ok());
        assert!(AcmeClient::validate_domain("", AcmeChallengeType::Http01).is_err());
        assert!(
            AcmeClient::validate_domain("invalid domain.com", AcmeChallengeType::Http01).is_err()
        );
        assert!(AcmeClient::validate_domain(".example.com", AcmeChallengeType::Http01).is_err());
        assert!(AcmeClient::validate_domain("example.com.", AcmeChallengeType::Http01).is_err());

        // Wildcard requires DNS-01
        assert!(AcmeClient::validate_domain("*.example.com", AcmeChallengeType::Http01).is_err());
        assert!(AcmeClient::validate_domain("*.example.com", AcmeChallengeType::Dns01).is_ok());
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

    #[test]
    fn test_challenge_type_display() {
        assert_eq!(AcmeChallengeType::Http01.to_string(), "http-01");
        assert_eq!(AcmeChallengeType::Dns01.to_string(), "dns-01");
    }
}
