//! Self-signed certificate generation for development and testing
//!
//! Provides zero-config TLS certificates for QUIC connections in development mode.

use rcgen::{CertificateParams, DistinguishedName};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::time::SystemTime;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SelfSignedError {
    #[error("Certificate generation failed: {0}")]
    GenerationFailed(String),

    #[error("Key generation failed: {0}")]
    KeyGenerationFailed(String),
}

/// Generate a self-signed certificate for development/testing
///
/// This creates an ephemeral certificate valid for localhost and common development domains.
/// **DO NOT use in production** - use proper CA-signed certificates or ACME instead.
///
/// # Features
/// - Valid for 90 days (typical development cycle)
/// - Includes localhost, 127.0.0.1, ::1 as SANs
/// - RSA 2048-bit key (fast generation, adequate for development)
/// - Random serial number to avoid collisions
///
/// # Example
/// ```no_run
/// use tunnel_cert::self_signed::generate_self_signed_cert;
///
/// let cert = generate_self_signed_cert().unwrap();
/// // Use cert.cert_der and cert.key_der with rustls/quinn
/// ```
pub fn generate_self_signed_cert() -> Result<SelfSignedCertificate, SelfSignedError> {
    let mut params = CertificateParams::default();

    // Set subject
    let mut dn = DistinguishedName::new();
    dn.push(rcgen::DnType::CommonName, "Tunnel Development Certificate");
    dn.push(rcgen::DnType::OrganizationName, "Tunnel Dev");
    params.distinguished_name = dn;

    // Set Subject Alternative Names (SANs) for local development
    params.subject_alt_names = vec![
        rcgen::SanType::DnsName(rcgen::Ia5String::try_from("localhost").unwrap()),
        rcgen::SanType::DnsName(rcgen::Ia5String::try_from("*.localhost").unwrap()),
        rcgen::SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))),
        rcgen::SanType::IpAddress(std::net::IpAddr::V6(std::net::Ipv6Addr::new(
            0, 0, 0, 0, 0, 0, 0, 1,
        ))),
    ];

    // Validity: 90 days from now
    let not_before = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    params.not_before = time::OffsetDateTime::from_unix_timestamp(not_before.as_secs() as i64)
        .map_err(|e| SelfSignedError::GenerationFailed(e.to_string()))?;

    let not_after = not_before + std::time::Duration::from_secs(90 * 24 * 60 * 60); // 90 days
    params.not_after = time::OffsetDateTime::from_unix_timestamp(not_after.as_secs() as i64)
        .map_err(|e| SelfSignedError::GenerationFailed(e.to_string()))?;

    // Generate random serial number
    params.serial_number = Some(rcgen::SerialNumber::from(rand::random::<u64>()));

    // Generate a key pair
    let key_pair =
        rcgen::KeyPair::generate().map_err(|e| SelfSignedError::GenerationFailed(e.to_string()))?;

    // Generate the certificate with our key pair
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| SelfSignedError::GenerationFailed(e.to_string()))?;

    // Serialize to PEM format (for file storage)
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    // Serialize to DER format (for direct quinn/rustls use)
    let cert_der = cert.der().to_vec();
    let key_der = key_pair.serialize_der();

    Ok(SelfSignedCertificate {
        cert_der: CertificateDer::from(cert_der),
        key_der: PrivateKeyDer::try_from(key_der)
            .map_err(|e| SelfSignedError::KeyGenerationFailed(format!("{:?}", e)))?,
        pem_cert: cert_pem,
        pem_key: key_pem,
    })
}

/// A self-signed certificate with its private key
pub struct SelfSignedCertificate {
    /// Certificate in DER format (binary)
    pub cert_der: CertificateDer<'static>,

    /// Private key in DER format (binary)
    pub key_der: PrivateKeyDer<'static>,

    /// Certificate in PEM format (text)
    pub pem_cert: String,

    /// Private key in PEM format (text)
    pub pem_key: String,
}

impl SelfSignedCertificate {
    /// Save certificate and key to PEM files
    pub fn save_to_files(&self, cert_path: &str, key_path: &str) -> std::io::Result<()> {
        std::fs::write(cert_path, &self.pem_cert)?;
        std::fs::write(key_path, &self.pem_key)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_self_signed_cert() {
        let cert = generate_self_signed_cert().unwrap();

        // Verify we got valid DER data
        assert!(!cert.cert_der.is_empty());
        assert!(!cert.pem_cert.is_empty());
        assert!(cert.pem_cert.contains("BEGIN CERTIFICATE"));
        assert!(cert.pem_key.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_cert_can_be_used_with_rustls() {
        let cert = generate_self_signed_cert().unwrap();

        // Verify rustls can parse it
        let certs = vec![cert.cert_der];
        let key = cert.key_der;

        // This would fail if the cert/key format is invalid
        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key);

        assert!(server_config.is_ok());
    }
}
