//! Pluggable configuration traits for relay builders
//!
//! This module provides trait-based configuration that allows users to customize:
//! - Where tunnels are persisted (in-memory, database, file-based)
//! - How domains are generated (simple counter, UUID-based, etc.)
//! - Port allocation strategies (sequential, random, reserved pools)
//! - Certificate providers (self-signed, ACME, cached)

use std::sync::Arc;
use thiserror::Error;

/// Errors that can occur in configuration implementations
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Domain generation error: {0}")]
    DomainError(String),

    #[error("Port allocation error: {0}")]
    PortError(String),

    #[error("Certificate error: {0}")]
    CertificateError(String),
}

/// Tunnel metadata persisted to storage
#[derive(Clone, Debug)]
pub struct TunnelRecord {
    pub localup_id: String,
    pub protocol: String,
    pub public_url: String,
    pub local_port: u16,
    pub public_port: Option<u16>,
    pub subdomain: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_active: chrono::DateTime<chrono::Utc>,
}

/// Trait for persisting tunnel information
///
/// Implement this to customize where and how tunnels are stored.
/// Default implementation stores everything in-memory.
///
/// # Example
/// ```ignore
/// struct DatabaseStorage { db: Arc<SqlitePool> }
///
/// #[async_trait]
/// impl TunnelStorage for DatabaseStorage {
///     async fn save(&self, record: TunnelRecord) -> Result<(), ConfigError> {
///         // Save to database
///     }
///
///     async fn get(&self, localup_id: &str) -> Result<Option<TunnelRecord>, ConfigError> {
///         // Query database
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait TunnelStorage: Send + Sync {
    /// Save or update a tunnel record
    async fn save(&self, record: TunnelRecord) -> Result<(), ConfigError>;

    /// Retrieve a tunnel record by ID
    async fn get(&self, localup_id: &str) -> Result<Option<TunnelRecord>, ConfigError>;

    /// List all active tunnels
    async fn list_active(&self) -> Result<Vec<TunnelRecord>, ConfigError>;

    /// Mark a tunnel as inactive
    async fn delete(&self, localup_id: &str) -> Result<(), ConfigError>;

    /// Update last_active timestamp (for activity tracking)
    async fn touch(&self, localup_id: &str) -> Result<(), ConfigError>;
}

/// Trait for generating subdomains and public URLs
///
/// Default implementation uses a simple counter (tunnel-1, tunnel-2, etc.)
/// or UUID-based names (tunnel-{uuid}).
///
/// # Example
/// ```ignore
/// struct CustomDomainProvider;
///
/// #[async_trait]
/// impl DomainProvider for CustomDomainProvider {
///     async fn generate_subdomain(&self) -> Result<String, ConfigError> {
///         // Generate from database, config file, etc.
///         Ok("my-app".to_string())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait DomainProvider: Send + Sync {
    /// Generate a unique subdomain for this tunnel
    async fn generate_subdomain(&self) -> Result<String, ConfigError>;

    /// Generate the full public URL for a tunnel
    /// Called after port is allocated (for TCP) or domain is generated (for HTTP/HTTPS)
    async fn generate_public_url(
        &self,
        subdomain: Option<&str>,
        port: Option<u16>,
        protocol: &str,
        public_domain: &str,
    ) -> Result<String, ConfigError>;

    /// Check if a subdomain is already taken
    async fn is_available(&self, subdomain: &str) -> Result<bool, ConfigError>;

    /// Reserve a subdomain (prevent others from using it)
    async fn reserve(&self, subdomain: &str) -> Result<(), ConfigError>;

    /// Release a reserved subdomain
    async fn release(&self, subdomain: &str) -> Result<(), ConfigError>;
}

/// Trait for certificate handling
///
/// This allows custom certificate providers (ACME, cached files, etc.)
///
/// # Example
/// ```ignore
/// struct AcmeCertificateProvider { client: Arc<AcmeClient> }
///
/// #[async_trait]
/// impl CertificateProvider for AcmeCertificateProvider {
///     async fn get_or_create(&self, domain: &str) -> Result<CertificateData, ConfigError> {
///         // Get from ACME or cache
///     }
/// }
/// ```
#[derive(Clone, Debug)]
pub struct CertificateData {
    pub certificate_pem: Vec<u8>,
    pub private_key_pem: Vec<u8>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait::async_trait]
pub trait CertificateProvider: Send + Sync {
    /// Get or create a certificate for the given domain
    async fn get_or_create(&self, domain: &str) -> Result<CertificateData, ConfigError>;

    /// Revoke/delete a certificate
    async fn revoke(&self, domain: &str) -> Result<(), ConfigError>;

    /// Check if a certificate is expiring soon (within 7 days)
    async fn needs_renewal(&self, domain: &str) -> Result<bool, ConfigError>;
}

// Note: PortAllocator trait is defined in localup_control::PortAllocator
// and re-exported from localup_lib

// ============================================================================
// Default In-Memory Implementations
// ============================================================================

use std::collections::HashMap;
use std::sync::Mutex;

/// In-memory tunnel storage (default implementation)
/// All data is lost when the relay restarts.
pub struct InMemoryTunnelStorage {
    tunnels: Arc<Mutex<HashMap<String, TunnelRecord>>>,
}

impl InMemoryTunnelStorage {
    pub fn new() -> Self {
        Self {
            tunnels: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryTunnelStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TunnelStorage for InMemoryTunnelStorage {
    async fn save(&self, record: TunnelRecord) -> Result<(), ConfigError> {
        self.tunnels
            .lock()
            .unwrap()
            .insert(record.localup_id.clone(), record);
        Ok(())
    }

    async fn get(&self, localup_id: &str) -> Result<Option<TunnelRecord>, ConfigError> {
        Ok(self.tunnels.lock().unwrap().get(localup_id).cloned())
    }

    async fn list_active(&self) -> Result<Vec<TunnelRecord>, ConfigError> {
        Ok(self.tunnels.lock().unwrap().values().cloned().collect())
    }

    async fn delete(&self, localup_id: &str) -> Result<(), ConfigError> {
        self.tunnels.lock().unwrap().remove(localup_id);
        Ok(())
    }

    async fn touch(&self, localup_id: &str) -> Result<(), ConfigError> {
        if let Some(record) = self.tunnels.lock().unwrap().get_mut(localup_id) {
            record.last_active = chrono::Utc::now();
        }
        Ok(())
    }
}

/// Simple counter-based domain provider (default implementation)
/// Generates domains like: tunnel-1, tunnel-2, tunnel-{uuid}
pub struct SimpleCounterDomainProvider {
    counter: Arc<Mutex<u64>>,
    reserved: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl SimpleCounterDomainProvider {
    pub fn new() -> Self {
        Self {
            counter: Arc::new(Mutex::new(0)),
            reserved: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }

    /// Use UUID-based domain generation instead of counter
    pub fn with_uuid() -> Self {
        Self {
            counter: Arc::new(Mutex::new(0)),
            reserved: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }
}

impl Default for SimpleCounterDomainProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl DomainProvider for SimpleCounterDomainProvider {
    async fn generate_subdomain(&self) -> Result<String, ConfigError> {
        let mut counter = self.counter.lock().unwrap();
        *counter += 1;
        Ok(format!("tunnel-{}", counter))
    }

    async fn generate_public_url(
        &self,
        subdomain: Option<&str>,
        port: Option<u16>,
        protocol: &str,
        public_domain: &str,
    ) -> Result<String, ConfigError> {
        match protocol {
            "tcp" => {
                // TCP: use port number
                port.map(|p| format!("{}:{}", public_domain, p))
                    .ok_or_else(|| ConfigError::DomainError("TCP requires port".into()))
            }
            "https" | "http" => {
                // HTTP(S): use subdomain
                subdomain
                    .map(|s| format!("{}://{}.{}", protocol, s, public_domain))
                    .ok_or_else(|| ConfigError::DomainError("HTTP requires subdomain".into()))
            }
            "tls" => {
                // TLS/SNI: use subdomain with port
                match (subdomain, port) {
                    (Some(s), Some(p)) => Ok(format!("{}:{}", s, p)),
                    (Some(s), None) => Ok(s.to_string()),
                    _ => Err(ConfigError::DomainError(
                        "TLS requires subdomain and/or port".into(),
                    )),
                }
            }
            _ => Err(ConfigError::DomainError(format!(
                "Unknown protocol: {}",
                protocol
            ))),
        }
    }

    async fn is_available(&self, subdomain: &str) -> Result<bool, ConfigError> {
        Ok(!self.reserved.lock().unwrap().contains(subdomain))
    }

    async fn reserve(&self, subdomain: &str) -> Result<(), ConfigError> {
        self.reserved.lock().unwrap().insert(subdomain.to_string());
        Ok(())
    }

    async fn release(&self, subdomain: &str) -> Result<(), ConfigError> {
        self.reserved.lock().unwrap().remove(subdomain);
        Ok(())
    }
}

/// Self-signed certificate provider (default implementation)
/// Generates new certificates on demand, no caching.
pub struct SelfSignedCertificateProvider;

#[async_trait::async_trait]
impl CertificateProvider for SelfSignedCertificateProvider {
    async fn get_or_create(&self, domain: &str) -> Result<CertificateData, ConfigError> {
        // Generate self-signed certificate
        let cert = localup_cert::generate_self_signed_cert_with_domains(domain, &[domain])
            .map_err(|e| {
                ConfigError::CertificateError(format!("Failed to generate cert: {}", e))
            })?;

        Ok(CertificateData {
            certificate_pem: cert.pem_cert.into_bytes(),
            private_key_pem: cert.pem_key.into_bytes(),
            expires_at: chrono::Utc::now() + chrono::Duration::days(90),
        })
    }

    async fn revoke(&self, _domain: &str) -> Result<(), ConfigError> {
        // No-op for self-signed certificates
        Ok(())
    }

    async fn needs_renewal(&self, _domain: &str) -> Result<bool, ConfigError> {
        // Self-signed certs always need renewal (they expire in 90 days)
        Ok(false)
    }
}
