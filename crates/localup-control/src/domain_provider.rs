//! Domain provider trait for customizable subdomain generation and management
//!
//! This module provides trait-based configuration for how subdomains are generated,
//! validated, and managed. Default implementations support simple counter-based
//! generation, but custom implementations can provide sticky domains, rules-based
//! assignment, or integration with external systems.

use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Errors that can occur in domain provider operations
#[derive(Error, Debug, Clone)]
pub enum DomainProviderError {
    #[error("Domain generation error: {0}")]
    DomainError(String),

    #[error("Invalid subdomain: {0}")]
    InvalidSubdomain(String),

    #[error("Subdomain unavailable: {0}")]
    Unavailable(String),
}

/// Context information passed to DomainProvider methods
///
/// Contains client identity and connection details needed for intelligent
/// subdomain assignment (e.g., sticky domains based on client_id + port).
#[derive(Clone, Debug)]
pub struct DomainContext {
    /// Client identifier from authentication token
    /// Used for sticky domain assignment, company-based rules, etc.
    pub client_id: Option<String>,

    /// Local port being tunneled
    /// Used in sticky domain assignment (client_id + port = unique key)
    pub local_port: Option<u16>,

    /// Protocol being used (http, https, tcp, tls)
    pub protocol: Option<String>,
}

impl DomainContext {
    /// Create a new domain context
    pub fn new() -> Self {
        Self {
            client_id: None,
            local_port: None,
            protocol: None,
        }
    }

    /// Set client ID
    pub fn with_client_id(mut self, client_id: String) -> Self {
        self.client_id = Some(client_id);
        self
    }

    /// Set local port
    pub fn with_local_port(mut self, port: u16) -> Self {
        self.local_port = Some(port);
        self
    }

    /// Set protocol
    pub fn with_protocol(mut self, protocol: String) -> Self {
        self.protocol = Some(protocol);
        self
    }
}

impl Default for DomainContext {
    fn default() -> Self {
        Self::new()
    }
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
///     async fn generate_subdomain(&self, context: &DomainContext) -> Result<String, DomainProviderError> {
///         // Generate from database, config file, etc.
///         Ok("my-app".to_string())
///     }
/// }
/// ```
#[async_trait]
pub trait DomainProvider: Send + Sync {
    /// Generate a unique subdomain for this tunnel
    ///
    /// Called when a tunnel is registered. The context contains:
    /// - `client_id`: From the auth token (enables sticky domains per client)
    /// - `local_port`: The local port being tunneled (enables sticky: client_id + port)
    /// - `protocol`: Protocol being used (enables protocol-specific logic)
    async fn generate_subdomain(
        &self,
        context: &DomainContext,
    ) -> Result<String, DomainProviderError>;

    /// Generate the full public URL for a tunnel
    /// Called after port is allocated (for TCP) or domain is generated (for HTTP/HTTPS)
    async fn generate_public_url(
        &self,
        context: &DomainContext,
        subdomain: Option<&str>,
        port: Option<u16>,
        protocol: &str,
        public_domain: &str,
    ) -> Result<String, DomainProviderError>;

    /// Check if a subdomain is already taken
    async fn is_available(&self, subdomain: &str) -> Result<bool, DomainProviderError>;

    /// Reserve a subdomain (prevent others from using it)
    async fn reserve(&self, subdomain: &str) -> Result<(), DomainProviderError>;

    /// Release a reserved subdomain
    async fn release(&self, subdomain: &str) -> Result<(), DomainProviderError>;

    /// Check if manual subdomain selection is allowed
    ///
    /// If `false`, only auto-generated subdomains are permitted.
    /// If `true`, users can specify custom subdomains (subject to validation).
    ///
    /// Default: `true` (allows manual selection)
    fn allow_manual_subdomain(&self) -> bool {
        true
    }

    /// Validate a user-provided subdomain
    ///
    /// Called when a user specifies a custom subdomain.
    /// Should check format, length, allowed characters, etc.
    ///
    /// Returns `Ok(())` if the subdomain is valid, or an error with details.
    ///
    /// Default implementation checks:
    /// - Not empty
    /// - Alphanumeric and hyphens only (no underscores, dots, etc.)
    /// - Between 3-63 characters (DNS label requirements)
    /// - Doesn't start or end with hyphen
    fn validate_subdomain(&self, subdomain: &str) -> Result<(), DomainProviderError> {
        if subdomain.is_empty() {
            return Err(DomainProviderError::InvalidSubdomain(
                "Subdomain cannot be empty".to_string(),
            ));
        }

        if subdomain.len() > 63 {
            return Err(DomainProviderError::InvalidSubdomain(format!(
                "Subdomain too long (max 63 characters): {}",
                subdomain.len()
            )));
        }

        if subdomain.len() < 3 {
            return Err(DomainProviderError::InvalidSubdomain(
                "Subdomain too short (minimum 3 characters)".to_string(),
            ));
        }

        if subdomain.starts_with('-') || subdomain.ends_with('-') {
            return Err(DomainProviderError::InvalidSubdomain(
                "Subdomain cannot start or end with hyphen".to_string(),
            ));
        }

        for ch in subdomain.chars() {
            if !ch.is_alphanumeric() && ch != '-' {
                return Err(DomainProviderError::InvalidSubdomain(format!(
                    "Subdomain contains invalid character '{}' (only alphanumeric and hyphens allowed)",
                    ch
                )));
            }
        }

        Ok(())
    }
}

/// Simple counter-based domain provider (default implementation)
/// Generates domains like: tunnel-1, tunnel-2, tunnel-{uuid}
pub struct SimpleCounterDomainProvider {
    counter: Arc<Mutex<u64>>,
    reserved: Arc<Mutex<HashSet<String>>>,
}

impl SimpleCounterDomainProvider {
    pub fn new() -> Self {
        Self {
            counter: Arc::new(Mutex::new(0)),
            reserved: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

impl Default for SimpleCounterDomainProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DomainProvider for SimpleCounterDomainProvider {
    async fn generate_subdomain(
        &self,
        _context: &DomainContext,
    ) -> Result<String, DomainProviderError> {
        let mut counter = self.counter.lock().unwrap();
        *counter += 1;
        Ok(format!("tunnel-{}", counter))
    }

    async fn generate_public_url(
        &self,
        _context: &DomainContext,
        subdomain: Option<&str>,
        port: Option<u16>,
        protocol: &str,
        public_domain: &str,
    ) -> Result<String, DomainProviderError> {
        match protocol {
            "tcp" => {
                // TCP: use port number
                port.map(|p| format!("{}:{}", public_domain, p))
                    .ok_or_else(|| DomainProviderError::DomainError("TCP requires port".into()))
            }
            "https" | "http" => {
                // HTTP(S): use subdomain
                subdomain
                    .map(|s| format!("{}://{}.{}", protocol, s, public_domain))
                    .ok_or_else(|| {
                        DomainProviderError::DomainError("HTTP requires subdomain".into())
                    })
            }
            "tls" => {
                // TLS/SNI: use subdomain with port
                match (subdomain, port) {
                    (Some(s), Some(p)) => Ok(format!("{}:{}", s, p)),
                    (Some(s), None) => Ok(s.to_string()),
                    _ => Err(DomainProviderError::DomainError(
                        "TLS requires subdomain and/or port".into(),
                    )),
                }
            }
            _ => Err(DomainProviderError::DomainError(format!(
                "Unknown protocol: {}",
                protocol
            ))),
        }
    }

    async fn is_available(&self, subdomain: &str) -> Result<bool, DomainProviderError> {
        Ok(!self.reserved.lock().unwrap().contains(subdomain))
    }

    async fn reserve(&self, subdomain: &str) -> Result<(), DomainProviderError> {
        self.reserved.lock().unwrap().insert(subdomain.to_string());
        Ok(())
    }

    async fn release(&self, subdomain: &str) -> Result<(), DomainProviderError> {
        self.reserved.lock().unwrap().remove(subdomain);
        Ok(())
    }
}

/// Domain provider that restricts manual subdomain selection
/// Forces all tunnels to use auto-generated subdomains
///
/// Useful for:
/// - Multi-tenant deployments where subdomain allocation is controlled
/// - Security-focused setups that disallow custom subdomains
/// - Simplified domain management (no user input validation needed)
pub struct RestrictedDomainProvider {
    counter: Arc<Mutex<u64>>,
    reserved: Arc<Mutex<HashSet<String>>>,
}

impl RestrictedDomainProvider {
    pub fn new() -> Self {
        Self {
            counter: Arc::new(Mutex::new(0)),
            reserved: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

impl Default for RestrictedDomainProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DomainProvider for RestrictedDomainProvider {
    async fn generate_subdomain(
        &self,
        _context: &DomainContext,
    ) -> Result<String, DomainProviderError> {
        let mut counter = self.counter.lock().unwrap();
        *counter += 1;
        Ok(format!("tunnel-{}", counter))
    }

    async fn generate_public_url(
        &self,
        _context: &DomainContext,
        subdomain: Option<&str>,
        port: Option<u16>,
        protocol: &str,
        public_domain: &str,
    ) -> Result<String, DomainProviderError> {
        match protocol {
            "tcp" => port
                .map(|p| format!("{}:{}", public_domain, p))
                .ok_or_else(|| DomainProviderError::DomainError("TCP requires port".into())),
            "https" | "http" => subdomain
                .map(|s| format!("{}://{}.{}", protocol, s, public_domain))
                .ok_or_else(|| DomainProviderError::DomainError("HTTP requires subdomain".into())),
            "tls" => match (subdomain, port) {
                (Some(s), Some(p)) => Ok(format!("{}:{}", s, p)),
                (Some(s), None) => Ok(s.to_string()),
                _ => Err(DomainProviderError::DomainError(
                    "TLS requires subdomain and/or port".into(),
                )),
            },
            _ => Err(DomainProviderError::DomainError(format!(
                "Unknown protocol: {}",
                protocol
            ))),
        }
    }

    async fn is_available(&self, subdomain: &str) -> Result<bool, DomainProviderError> {
        Ok(!self.reserved.lock().unwrap().contains(subdomain))
    }

    async fn reserve(&self, subdomain: &str) -> Result<(), DomainProviderError> {
        self.reserved.lock().unwrap().insert(subdomain.to_string());
        Ok(())
    }

    async fn release(&self, subdomain: &str) -> Result<(), DomainProviderError> {
        self.reserved.lock().unwrap().remove(subdomain);
        Ok(())
    }

    /// Restrict manual subdomain selection
    fn allow_manual_subdomain(&self) -> bool {
        false
    }

    /// Reject manual subdomains - use auto-generated only
    fn validate_subdomain(&self, _subdomain: &str) -> Result<(), DomainProviderError> {
        Err(DomainProviderError::InvalidSubdomain(
            "Manual subdomain selection is not allowed. Only auto-generated subdomains are permitted."
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_counter_generates_sequential_subdomains() {
        let provider = SimpleCounterDomainProvider::new();
        let context = DomainContext::new();

        let first = provider.generate_subdomain(&context).await.unwrap();
        let second = provider.generate_subdomain(&context).await.unwrap();
        let third = provider.generate_subdomain(&context).await.unwrap();

        assert_eq!(first, "tunnel-1");
        assert_eq!(second, "tunnel-2");
        assert_eq!(third, "tunnel-3");
    }

    #[tokio::test]
    async fn test_restricted_provider_generates_sequential_subdomains() {
        let provider = RestrictedDomainProvider::new();
        let context = DomainContext::new();

        let first = provider.generate_subdomain(&context).await.unwrap();
        let second = provider.generate_subdomain(&context).await.unwrap();

        assert_eq!(first, "tunnel-1");
        assert_eq!(second, "tunnel-2");
    }

    #[test]
    fn test_restricted_provider_rejects_manual_subdomains() {
        let provider = RestrictedDomainProvider::new();
        match provider.validate_subdomain("any") {
            Err(DomainProviderError::InvalidSubdomain(msg)) => {
                assert!(msg.contains("not allowed"));
            }
            _ => panic!("Expected InvalidSubdomain error"),
        }
    }

    #[tokio::test]
    async fn test_subdomain_reservation_workflow() {
        let provider = SimpleCounterDomainProvider::new();

        // Check available
        assert!(provider.is_available("my-domain").await.unwrap());

        // Reserve it
        provider.reserve("my-domain").await.unwrap();

        // Now it's taken
        assert!(!provider.is_available("my-domain").await.unwrap());

        // Release it
        provider.release("my-domain").await.unwrap();

        // Back to available
        assert!(provider.is_available("my-domain").await.unwrap());
    }

    #[test]
    fn test_subdomain_validation() {
        let provider = SimpleCounterDomainProvider::new();

        // Valid subdomains
        assert!(provider.validate_subdomain("my-app").is_ok());
        assert!(provider.validate_subdomain("api-v2").is_ok());
        assert!(provider.validate_subdomain("tunnel123").is_ok());

        // Invalid subdomains
        assert!(provider.validate_subdomain("").is_err()); // Empty
        assert!(provider.validate_subdomain("ab").is_err()); // Too short
        assert!(provider.validate_subdomain(&"a".repeat(64)).is_err()); // Too long
        assert!(provider.validate_subdomain("-app").is_err()); // Leading hyphen
        assert!(provider.validate_subdomain("app-").is_err()); // Trailing hyphen
        assert!(provider.validate_subdomain("my_app").is_err()); // Underscore
        assert!(provider.validate_subdomain("my.app").is_err()); // Dot
    }
}
