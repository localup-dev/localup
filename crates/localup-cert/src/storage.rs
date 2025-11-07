//! Certificate storage

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use tracing::{debug, trace};

/// Certificate storage errors
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Certificate not found: {0}")]
    NotFound(String),

    #[error("Certificate expired: {0}")]
    Expired(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Stored certificate with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCertificate {
    pub domain: String,
    pub certificate_pem: String,
    pub private_key_pem: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl StoredCertificate {
    pub fn new(
        domain: String,
        certificate_pem: String,
        private_key_pem: String,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            domain,
            certificate_pem,
            private_key_pem,
            expires_at,
            created_at: Utc::now(),
        }
    }

    /// Check if certificate is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Check if certificate needs renewal (within 30 days of expiry)
    pub fn needs_renewal(&self) -> bool {
        let now = Utc::now();
        let renewal_threshold = self.expires_at - chrono::Duration::days(30);
        now > renewal_threshold
    }

    /// Days until expiry
    pub fn days_until_expiry(&self) -> i64 {
        (self.expires_at - Utc::now()).num_days()
    }
}

/// In-memory certificate store
pub struct CertificateStore {
    certificates: Arc<RwLock<HashMap<String, StoredCertificate>>>,
}

impl CertificateStore {
    pub fn new() -> Self {
        Self {
            certificates: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store a certificate
    pub fn store(&self, cert: StoredCertificate) -> Result<(), StorageError> {
        debug!("Storing certificate for domain: {}", cert.domain);

        let mut certs = self.certificates.write().unwrap();
        certs.insert(cert.domain.clone(), cert);

        Ok(())
    }

    /// Retrieve a certificate
    pub fn get(&self, domain: &str) -> Result<StoredCertificate, StorageError> {
        trace!("Retrieving certificate for domain: {}", domain);

        let certs = self.certificates.read().unwrap();
        let cert = certs
            .get(domain)
            .ok_or_else(|| StorageError::NotFound(domain.to_string()))?;

        if cert.is_expired() {
            return Err(StorageError::Expired(domain.to_string()));
        }

        Ok(cert.clone())
    }

    /// Delete a certificate
    pub fn delete(&self, domain: &str) -> Result<(), StorageError> {
        debug!("Deleting certificate for domain: {}", domain);

        let mut certs = self.certificates.write().unwrap();
        certs
            .remove(domain)
            .ok_or_else(|| StorageError::NotFound(domain.to_string()))?;

        Ok(())
    }

    /// List all domains with certificates
    pub fn list_domains(&self) -> Vec<String> {
        let certs = self.certificates.read().unwrap();
        certs.keys().cloned().collect()
    }

    /// Get certificates that need renewal
    pub fn get_renewal_candidates(&self) -> Vec<StoredCertificate> {
        let certs = self.certificates.read().unwrap();
        certs
            .values()
            .filter(|cert| cert.needs_renewal())
            .cloned()
            .collect()
    }

    /// Check if a certificate exists
    pub fn exists(&self, domain: &str) -> bool {
        let certs = self.certificates.read().unwrap();
        certs.contains_key(domain)
    }

    /// Get number of stored certificates
    pub fn count(&self) -> usize {
        let certs = self.certificates.read().unwrap();
        certs.len()
    }
}

impl Default for CertificateStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_cert(domain: &str, days_until_expiry: i64) -> StoredCertificate {
        let expires_at = Utc::now() + chrono::Duration::days(days_until_expiry);
        StoredCertificate::new(
            domain.to_string(),
            "cert_pem".to_string(),
            "key_pem".to_string(),
            expires_at,
        )
    }

    #[test]
    fn test_certificate_store() {
        let store = CertificateStore::new();
        let cert = create_test_cert("example.com", 90);

        store.store(cert.clone()).unwrap();
        assert_eq!(store.count(), 1);

        let retrieved = store.get("example.com").unwrap();
        assert_eq!(retrieved.domain, "example.com");

        store.delete("example.com").unwrap();
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn test_certificate_not_found() {
        let store = CertificateStore::new();
        let result = store.get("nonexistent.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_certificate_needs_renewal() {
        // Certificate expiring in 20 days should need renewal
        let cert = create_test_cert("example.com", 20);
        assert!(cert.needs_renewal());

        // Certificate expiring in 60 days should not need renewal
        let cert = create_test_cert("example.com", 60);
        assert!(!cert.needs_renewal());
    }

    #[test]
    fn test_certificate_expired() {
        // Certificate expired 1 day ago
        let cert = create_test_cert("example.com", -1);
        assert!(cert.is_expired());

        // Certificate expiring in 1 day
        let cert = create_test_cert("example.com", 1);
        assert!(!cert.is_expired());
    }

    #[test]
    fn test_get_renewal_candidates() {
        let store = CertificateStore::new();

        // Add certificates with different expiry dates
        store
            .store(create_test_cert("needs-renewal.com", 20))
            .unwrap();
        store.store(create_test_cert("valid.com", 60)).unwrap();
        store.store(create_test_cert("also-needs.com", 15)).unwrap();

        let candidates = store.get_renewal_candidates();
        assert_eq!(candidates.len(), 2);
    }

    #[test]
    fn test_list_domains() {
        let store = CertificateStore::new();

        store.store(create_test_cert("example1.com", 90)).unwrap();
        store.store(create_test_cert("example2.com", 90)).unwrap();
        store.store(create_test_cert("example3.com", 90)).unwrap();

        let domains = store.list_domains();
        assert_eq!(domains.len(), 3);
        assert!(domains.contains(&"example1.com".to_string()));
        assert!(domains.contains(&"example2.com".to_string()));
        assert!(domains.contains(&"example3.com".to_string()));
    }
}
