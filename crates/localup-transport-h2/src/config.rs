//! HTTP/2 transport configuration

use localup_transport::{
    TransportConfig, TransportError, TransportResult, TransportSecurityConfig,
};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// HTTP/2-specific configuration
#[derive(Debug, Clone)]
pub struct H2Config {
    /// Security configuration
    security: TransportSecurityConfig,

    /// Server certificate path (for servers)
    pub server_cert_path: Option<String>,

    /// Server private key path (for servers)
    pub server_key_path: Option<String>,

    /// Keep-alive interval (PING frames)
    pub keep_alive_interval: Duration,

    /// Maximum idle timeout
    pub max_idle_timeout: Duration,

    /// Initial window size
    pub initial_window_size: u32,

    /// Maximum concurrent streams
    pub max_concurrent_streams: u32,

    /// Maximum frame size
    pub max_frame_size: u32,
}

impl H2Config {
    /// Create a client configuration with defaults
    pub fn client_default() -> Self {
        Self {
            security: TransportSecurityConfig {
                alpn_protocols: vec!["h2".to_string(), "localup-h2-v1".to_string()],
                ..Default::default()
            },
            server_cert_path: None,
            server_key_path: None,
            keep_alive_interval: Duration::from_secs(30),
            max_idle_timeout: Duration::from_secs(60),
            initial_window_size: 1024 * 1024, // 1MB
            max_concurrent_streams: 100,
            max_frame_size: 16 * 1024, // 16KB (HTTP/2 default)
        }
    }

    /// Create a client configuration for local development (skip cert verification)
    pub fn client_insecure() -> Self {
        Self::client_default().with_insecure_skip_verify()
    }

    /// Create a server configuration with certificate paths
    pub fn server_default(cert_path: &str, key_path: &str) -> TransportResult<Self> {
        Ok(Self {
            security: TransportSecurityConfig {
                alpn_protocols: vec!["h2".to_string(), "localup-h2-v1".to_string()],
                ..Default::default()
            },
            server_cert_path: Some(cert_path.to_string()),
            server_key_path: Some(key_path.to_string()),
            keep_alive_interval: Duration::from_secs(30),
            max_idle_timeout: Duration::from_secs(60),
            initial_window_size: 1024 * 1024,
            max_concurrent_streams: 1000,
            max_frame_size: 16 * 1024,
        })
    }

    /// Create a zero-config server with persistent self-signed certificate
    pub fn server_self_signed() -> TransportResult<Self> {
        use localup_cert::generate_self_signed_cert;

        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| {
                TransportError::ConfigurationError("Cannot determine home directory".to_string())
            })?;

        let localup_dir = Path::new(&home_dir).join(".localup");
        let cert_path = localup_dir.join("localup-h2.crt");
        let key_path = localup_dir.join("localup-h2.key");

        if cert_path.exists() && key_path.exists() && load_certs(&cert_path).is_ok() {
            return Ok(Self {
                security: TransportSecurityConfig {
                    alpn_protocols: vec!["h2".to_string(), "localup-h2-v1".to_string()],
                    ..Default::default()
                },
                server_cert_path: Some(cert_path.to_str().unwrap().to_string()),
                server_key_path: Some(key_path.to_str().unwrap().to_string()),
                keep_alive_interval: Duration::from_secs(30),
                max_idle_timeout: Duration::from_secs(60),
                initial_window_size: 1024 * 1024,
                max_concurrent_streams: 1000,
                max_frame_size: 16 * 1024,
            });
        }

        let cert = generate_self_signed_cert().map_err(|e| {
            TransportError::TlsError(format!("Failed to generate self-signed cert: {}", e))
        })?;

        std::fs::create_dir_all(&localup_dir).map_err(|e| {
            TransportError::ConfigurationError(format!(
                "Failed to create ~/.localup directory: {}",
                e
            ))
        })?;

        cert.save_to_files(cert_path.to_str().unwrap(), key_path.to_str().unwrap())
            .map_err(|e| TransportError::TlsError(format!("Failed to save cert files: {}", e)))?;

        Ok(Self {
            security: TransportSecurityConfig {
                alpn_protocols: vec!["h2".to_string(), "localup-h2-v1".to_string()],
                ..Default::default()
            },
            server_cert_path: Some(cert_path.to_str().unwrap().to_string()),
            server_key_path: Some(key_path.to_str().unwrap().to_string()),
            keep_alive_interval: Duration::from_secs(30),
            max_idle_timeout: Duration::from_secs(60),
            initial_window_size: 1024 * 1024,
            max_concurrent_streams: 1000,
            max_frame_size: 16 * 1024,
        })
    }

    /// Set custom keep-alive interval
    pub fn with_keep_alive(mut self, interval: Duration) -> Self {
        self.keep_alive_interval = interval;
        self
    }

    /// Disable server certificate verification (INSECURE)
    pub fn with_insecure_skip_verify(mut self) -> Self {
        self.security.verify_server_cert = false;
        self
    }

    /// Build rustls TlsConnector for client
    pub(crate) fn build_tls_connector(&self) -> TransportResult<tokio_rustls::TlsConnector> {
        ensure_crypto_provider();

        let mut roots = rustls::RootCertStore::empty();

        if self.security.root_certs.is_empty() {
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        } else {
            for cert_der in &self.security.root_certs {
                roots
                    .add(rustls::pki_types::CertificateDer::from(cert_der.clone()))
                    .map_err(|e| {
                        TransportError::ConfigurationError(format!("Invalid root cert: {}", e))
                    })?;
            }
        }

        let mut client_crypto = if self.security.verify_server_cert {
            rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth()
        } else {
            rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(SkipVerification::new())
                .with_no_client_auth()
        };

        // Set ALPN for HTTP/2
        client_crypto.alpn_protocols = self
            .security
            .alpn_protocols
            .iter()
            .map(|s| s.as_bytes().to_vec())
            .collect();

        Ok(tokio_rustls::TlsConnector::from(Arc::new(client_crypto)))
    }

    /// Build rustls TlsAcceptor for server
    pub(crate) fn build_tls_acceptor(&self) -> TransportResult<tokio_rustls::TlsAcceptor> {
        ensure_crypto_provider();

        let cert_path = self.server_cert_path.as_ref().ok_or_else(|| {
            TransportError::ConfigurationError("Server cert path required".to_string())
        })?;
        let key_path = self.server_key_path.as_ref().ok_or_else(|| {
            TransportError::ConfigurationError("Server key path required".to_string())
        })?;

        let certs = load_certs(Path::new(cert_path))?;
        let key = load_private_key(Path::new(key_path))?;

        let mut server_crypto = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| TransportError::TlsError(format!("Invalid cert/key: {}", e)))?;

        // Set ALPN for HTTP/2
        server_crypto.alpn_protocols = self
            .security
            .alpn_protocols
            .iter()
            .map(|s| s.as_bytes().to_vec())
            .collect();

        Ok(tokio_rustls::TlsAcceptor::from(Arc::new(server_crypto)))
    }
}

impl TransportConfig for H2Config {
    fn security_config(&self) -> &TransportSecurityConfig {
        &self.security
    }

    fn validate(&self) -> TransportResult<()> {
        if self.initial_window_size == 0 {
            return Err(TransportError::ConfigurationError(
                "Initial window size must be > 0".to_string(),
            ));
        }
        Ok(())
    }
}

// Initialize rustls crypto provider
static CRYPTO_PROVIDER_INIT: std::sync::Once = std::sync::Once::new();

fn ensure_crypto_provider() {
    CRYPTO_PROVIDER_INIT.call_once(|| {
        if rustls::crypto::ring::default_provider()
            .install_default()
            .is_err()
        {
            tracing::debug!("Rustls crypto provider already installed");
        }
    });
}

fn load_certs(path: &Path) -> TransportResult<Vec<rustls::pki_types::CertificateDer<'static>>> {
    let file = File::open(path)
        .map_err(|e| TransportError::TlsError(format!("Failed to open cert file: {}", e)))?;
    let mut reader = BufReader::new(file);

    rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TransportError::TlsError(format!("Failed to parse certs: {}", e)))
}

fn load_private_key(path: &Path) -> TransportResult<rustls::pki_types::PrivateKeyDer<'static>> {
    let file = File::open(path)
        .map_err(|e| TransportError::TlsError(format!("Failed to open key file: {}", e)))?;
    let mut reader = BufReader::new(file);

    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| TransportError::TlsError(format!("Failed to parse key: {}", e)))?
        .ok_or_else(|| TransportError::TlsError("No private key found".to_string()))
}

// Certificate verifier that skips verification (INSECURE)
#[derive(Debug)]
struct SkipVerification;

impl SkipVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl rustls::client::danger::ServerCertVerifier for SkipVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        use rustls::SignatureScheme;
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
        let config = H2Config::client_default();
        assert_eq!(config.initial_window_size, 1024 * 1024);
        assert_eq!(config.max_concurrent_streams, 100);
    }

    #[test]
    fn test_config_validation() {
        let config = H2Config::client_default();
        assert!(config.validate().is_ok());
    }
}
