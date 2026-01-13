//! QUIC transport configuration

use localup_transport::{
    TransportConfig, TransportError, TransportResult, TransportSecurityConfig,
};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// QUIC-specific configuration
#[derive(Debug, Clone)]
pub struct QuicConfig {
    /// Security configuration
    security: TransportSecurityConfig,

    /// Server certificate path (for servers)
    pub server_cert_path: Option<String>,

    /// Server private key path (for servers)
    pub server_key_path: Option<String>,

    /// Keep-alive interval
    pub keep_alive_interval: Duration,

    /// Maximum idle timeout
    pub max_idle_timeout: Duration,

    /// Maximum number of concurrent bidirectional streams
    pub max_concurrent_streams: u64,
}

impl QuicConfig {
    /// Create a client configuration with defaults
    ///
    /// Uses system root CAs for certificate verification.
    /// For development/testing with self-signed certs, use `.with_insecure_skip_verify()`.
    pub fn client_default() -> Self {
        Self {
            security: TransportSecurityConfig::default(),
            server_cert_path: None,
            server_key_path: None,
            keep_alive_interval: Duration::from_secs(3), // Faster keep-alive for quicker disconnect detection
            max_idle_timeout: Duration::from_secs(10), // Detect dead connections within 10 seconds
            max_concurrent_streams: 100,
        }
    }

    /// Create a client configuration for local development (skip cert verification)
    ///
    /// **INSECURE**: This skips TLS certificate verification and should ONLY be used
    /// for local development with self-signed certificates.
    ///
    /// # Security Warning
    /// Never use this in production! It makes your connection vulnerable to MITM attacks.
    pub fn client_insecure() -> Self {
        Self::client_default().with_insecure_skip_verify()
    }

    /// Create a server configuration with certificate paths
    pub fn server_default(cert_path: &str, key_path: &str) -> TransportResult<Self> {
        Ok(Self {
            security: TransportSecurityConfig::default(),
            server_cert_path: Some(cert_path.to_string()),
            server_key_path: Some(key_path.to_string()),
            keep_alive_interval: Duration::from_secs(3),
            max_idle_timeout: Duration::from_secs(10),
            max_concurrent_streams: 1000,
        })
    }

    /// Create a zero-config server with persistent self-signed certificate
    ///
    /// Automatically generates a self-signed certificate valid for localhost development.
    /// The certificate is stored in `~/.localup/` and reused across restarts, ensuring
    /// consistent client trust. Valid for 90 days.
    ///
    /// **Certificate Location**: `~/.localup/localup-quic.{crt,key}`
    ///
    /// **Use case**: Local development and testing only.
    ///
    /// # Example
    /// ```no_run
    /// use localup_transport_quic::QuicConfig;
    ///
    /// // Zero-config QUIC server - just works!
    /// let config = QuicConfig::server_self_signed().unwrap();
    /// ```
    ///
    /// # Security Warning
    /// - Self-signed certificates are NOT trusted by default browsers/clients
    /// - Clients must use `client_insecure()` or add the cert to their trust store
    /// - NEVER use in production - use proper CA-signed certs or ACME
    pub fn server_self_signed() -> TransportResult<Self> {
        use localup_cert::generate_self_signed_cert;

        // Use ~/.localup for persistent certificate storage
        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE")) // Windows fallback
            .map_err(|_| {
                TransportError::ConfigurationError("Cannot determine home directory".to_string())
            })?;

        let localup_dir = Path::new(&home_dir).join(".localup");
        let cert_path = localup_dir.join("localup-quic.crt");
        let key_path = localup_dir.join("localup-quic.key");

        // Check if certificate already exists and is valid
        if cert_path.exists() && key_path.exists() {
            // Try to load existing certificate
            match load_certs(&cert_path) {
                Ok(_certs) => {
                    // Certificate exists and is loadable - reuse it
                    return Ok(Self {
                        security: TransportSecurityConfig::default(),
                        server_cert_path: Some(cert_path.to_str().unwrap().to_string()),
                        server_key_path: Some(key_path.to_str().unwrap().to_string()),
                        keep_alive_interval: Duration::from_secs(5),
                        max_idle_timeout: Duration::from_secs(30),
                        max_concurrent_streams: 1000,
                    });
                }
                Err(_) => {
                    // Certificate exists but is invalid - regenerate
                }
            }
        }

        // Generate new certificate
        let cert = generate_self_signed_cert().map_err(|e| {
            TransportError::TlsError(format!("Failed to generate self-signed cert: {}", e))
        })?;

        // Create ~/.localup directory if it doesn't exist
        std::fs::create_dir_all(&localup_dir).map_err(|e| {
            TransportError::ConfigurationError(format!(
                "Failed to create ~/.localup directory: {}",
                e
            ))
        })?;

        // Save certificate to persistent location
        cert.save_to_files(cert_path.to_str().unwrap(), key_path.to_str().unwrap())
            .map_err(|e| TransportError::TlsError(format!("Failed to save cert files: {}", e)))?;

        Ok(Self {
            security: TransportSecurityConfig::default(),
            server_cert_path: Some(cert_path.to_str().unwrap().to_string()),
            server_key_path: Some(key_path.to_str().unwrap().to_string()),
            keep_alive_interval: Duration::from_secs(3),
            max_idle_timeout: Duration::from_secs(10),
            max_concurrent_streams: 1000,
        })
    }

    /// Create a zero-config server with ephemeral self-signed certificate (for tests)
    ///
    /// Generates a unique self-signed certificate in temp directory with UUID to avoid
    /// conflicts when tests run in parallel. Certificate is NOT reused across runs.
    ///
    /// **Use case**: Integration tests only. For development, use `server_self_signed()`.
    ///
    /// # Example
    /// ```no_run
    /// use localup_transport_quic::QuicConfig;
    ///
    /// // Each test gets unique cert - no conflicts in parallel runs
    /// let config = QuicConfig::server_ephemeral().unwrap();
    /// ```
    #[doc(hidden)] // Internal use for tests
    pub fn server_ephemeral() -> TransportResult<Self> {
        use localup_cert::generate_self_signed_cert;

        let cert = generate_self_signed_cert().map_err(|e| {
            TransportError::TlsError(format!("Failed to generate self-signed cert: {}", e))
        })?;

        // Store cert/key in temporary files with UUID to avoid conflicts
        let temp_dir = std::env::temp_dir();
        let unique_id = uuid::Uuid::new_v4();
        let cert_path = temp_dir.join(format!("localup-quic-test-{}.crt", unique_id));
        let key_path = temp_dir.join(format!("localup-quic-test-{}.key", unique_id));

        cert.save_to_files(cert_path.to_str().unwrap(), key_path.to_str().unwrap())
            .map_err(|e| {
                TransportError::TlsError(format!("Failed to save temp cert files: {}", e))
            })?;

        Ok(Self {
            security: TransportSecurityConfig::default(),
            server_cert_path: Some(cert_path.to_str().unwrap().to_string()),
            server_key_path: Some(key_path.to_str().unwrap().to_string()),
            keep_alive_interval: Duration::from_secs(3),
            max_idle_timeout: Duration::from_secs(10),
            max_concurrent_streams: 1000,
        })
    }

    /// Set custom keep-alive interval
    pub fn with_keep_alive(mut self, interval: Duration) -> Self {
        self.keep_alive_interval = interval;
        self
    }

    /// Set custom idle timeout
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.max_idle_timeout = timeout;
        self
    }

    /// Set maximum concurrent streams
    pub fn with_max_streams(mut self, max: u64) -> Self {
        self.max_concurrent_streams = max;
        self
    }

    /// Disable server certificate verification (INSECURE - only for testing!)
    pub fn with_insecure_skip_verify(mut self) -> Self {
        self.security.verify_server_cert = false;
        self
    }

    /// Set custom ALPN protocols
    pub fn with_alpn_protocols(mut self, protocols: Vec<String>) -> Self {
        self.security.alpn_protocols = protocols;
        self
    }

    /// Build quinn ClientConfig
    pub(crate) fn build_client_config(&self) -> TransportResult<quinn::ClientConfig> {
        // Use quinn's re-exported rustls
        let mut roots = quinn::rustls::RootCertStore::empty();

        if self.security.root_certs.is_empty() {
            // Use system root certificates
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        } else {
            // Use custom root certificates
            for cert_der in &self.security.root_certs {
                roots
                    .add(quinn::rustls::pki_types::CertificateDer::from(
                        cert_der.clone(),
                    ))
                    .map_err(|e| {
                        TransportError::ConfigurationError(format!("Invalid root cert: {}", e))
                    })?;
            }
        }

        let mut client_crypto = if self.security.verify_server_cert {
            quinn::rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth()
        } else {
            // INSECURE: Skip certificate verification

            quinn::rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(SkipVerification::new())
                .with_no_client_auth()
        };

        // Set ALPN protocols
        client_crypto.alpn_protocols = self
            .security
            .alpn_protocols
            .iter()
            .map(|s| s.as_bytes().to_vec())
            .collect();

        // Convert to QUIC crypto config (quinn expects owned ClientConfig, not Arc)
        let mut client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
                .map_err(|e| TransportError::TlsError(e.to_string()))?,
        ));

        // Set transport parameters
        let mut transport = quinn::TransportConfig::default();
        transport.keep_alive_interval(Some(self.keep_alive_interval));
        transport.max_idle_timeout(Some(self.max_idle_timeout.try_into().unwrap()));
        transport.max_concurrent_bidi_streams(self.max_concurrent_streams.try_into().unwrap());

        client_config.transport_config(Arc::new(transport));

        Ok(client_config)
    }

    /// Build quinn ServerConfig
    pub(crate) fn build_server_config(&self) -> TransportResult<quinn::ServerConfig> {
        let cert_path = self.server_cert_path.as_ref().ok_or_else(|| {
            TransportError::ConfigurationError("Server cert path required".to_string())
        })?;
        let key_path = self.server_key_path.as_ref().ok_or_else(|| {
            TransportError::ConfigurationError("Server key path required".to_string())
        })?;

        // Load certificates and key
        let certs = load_certs(Path::new(cert_path))?;
        let key = load_private_key(Path::new(key_path))?;

        // Use quinn's re-exported rustls
        let mut server_crypto = quinn::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| TransportError::TlsError(format!("Invalid cert/key: {}", e)))?;

        // Set ALPN protocols
        server_crypto.alpn_protocols = self
            .security
            .alpn_protocols
            .iter()
            .map(|s| s.as_bytes().to_vec())
            .collect();

        // Convert to QUIC crypto config
        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
                .map_err(|e| TransportError::TlsError(e.to_string()))?,
        ));

        // Set transport parameters
        let mut transport = quinn::TransportConfig::default();
        transport.keep_alive_interval(Some(self.keep_alive_interval));
        transport.max_idle_timeout(Some(self.max_idle_timeout.try_into().unwrap()));
        transport.max_concurrent_bidi_streams(self.max_concurrent_streams.try_into().unwrap());

        server_config.transport_config(Arc::new(transport));

        Ok(server_config)
    }
}

impl TransportConfig for QuicConfig {
    fn security_config(&self) -> &TransportSecurityConfig {
        &self.security
    }

    fn validate(&self) -> TransportResult<()> {
        if self.keep_alive_interval.as_secs() == 0 {
            return Err(TransportError::ConfigurationError(
                "Keep-alive interval must be > 0".to_string(),
            ));
        }

        if self.max_idle_timeout < self.keep_alive_interval * 2 {
            return Err(TransportError::ConfigurationError(
                "Idle timeout must be at least 2x keep-alive interval".to_string(),
            ));
        }

        Ok(())
    }
}

// Helper functions for loading certificates

fn load_certs(
    path: &Path,
) -> TransportResult<Vec<quinn::rustls::pki_types::CertificateDer<'static>>> {
    let file = File::open(path)
        .map_err(|e| TransportError::TlsError(format!("Failed to open cert file: {}", e)))?;
    let mut reader = BufReader::new(file);

    rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TransportError::TlsError(format!("Failed to parse certs: {}", e)))
}

fn load_private_key(
    path: &Path,
) -> TransportResult<quinn::rustls::pki_types::PrivateKeyDer<'static>> {
    let file = File::open(path)
        .map_err(|e| TransportError::TlsError(format!("Failed to open key file: {}", e)))?;
    let mut reader = BufReader::new(file);

    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| TransportError::TlsError(format!("Failed to parse key: {}", e)))?
        .ok_or_else(|| TransportError::TlsError("No private key found".to_string()))
}

// Certificate verifier that skips verification (INSECURE - only for testing!)
#[derive(Debug)]
struct SkipVerification;

impl SkipVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl quinn::rustls::client::danger::ServerCertVerifier for SkipVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &quinn::rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[quinn::rustls::pki_types::CertificateDer<'_>],
        _server_name: &quinn::rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: quinn::rustls::pki_types::UnixTime,
    ) -> Result<quinn::rustls::client::danger::ServerCertVerified, quinn::rustls::Error> {
        Ok(quinn::rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &quinn::rustls::pki_types::CertificateDer<'_>,
        _dss: &quinn::rustls::DigitallySignedStruct,
    ) -> Result<quinn::rustls::client::danger::HandshakeSignatureValid, quinn::rustls::Error> {
        Ok(quinn::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &quinn::rustls::pki_types::CertificateDer<'_>,
        _dss: &quinn::rustls::DigitallySignedStruct,
    ) -> Result<quinn::rustls::client::danger::HandshakeSignatureValid, quinn::rustls::Error> {
        Ok(quinn::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<quinn::rustls::SignatureScheme> {
        use quinn::rustls::SignatureScheme;
        // Support all common signature schemes
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_default() {
        let config = QuicConfig::client_default();
        assert_eq!(config.keep_alive_interval, Duration::from_secs(3));
        assert_eq!(config.max_idle_timeout, Duration::from_secs(10));
        assert_eq!(config.max_concurrent_streams, 100);
    }

    #[test]
    fn test_config_validation() {
        let config = QuicConfig::client_default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_config_validation() {
        let config = QuicConfig::client_default().with_idle_timeout(Duration::from_secs(1));

        assert!(config.validate().is_err());
    }
}
