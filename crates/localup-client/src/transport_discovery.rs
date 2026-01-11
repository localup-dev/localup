//! Transport protocol discovery for automatic protocol selection
//!
//! This module fetches available transport protocols from a relay server
//! and selects the best one based on priority and availability.

use localup_proto::{ProtocolDiscoveryResponse, TransportProtocol, WELL_KNOWN_PATH};
use std::net::SocketAddr;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Transport discovery errors
#[derive(Debug, Error)]
pub enum TransportDiscoveryError {
    #[error("Failed to connect to relay: {0}")]
    ConnectionFailed(String),

    #[error("Failed to fetch protocol discovery: {0}")]
    FetchFailed(String),

    #[error("Invalid response from relay: {0}")]
    InvalidResponse(String),

    #[error("No transports available")]
    NoTransports,

    #[error("Timeout fetching protocols")]
    Timeout,
}

/// Result of transport discovery
#[derive(Debug, Clone)]
pub struct DiscoveredTransport {
    /// Selected transport protocol
    pub protocol: TransportProtocol,
    /// Address to connect to (may differ by protocol)
    pub address: SocketAddr,
    /// Path for WebSocket (if applicable)
    pub path: Option<String>,
    /// Full discovery response (if available)
    pub full_response: Option<ProtocolDiscoveryResponse>,
}

/// Transport discoverer - fetches and selects transport protocols
pub struct TransportDiscoverer {
    /// Timeout for HTTP requests
    timeout: Duration,
    /// Whether to skip TLS verification (for development)
    insecure: bool,
}

impl TransportDiscoverer {
    /// Create a new transport discoverer
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            insecure: false,
        }
    }

    /// Set request timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Enable insecure mode (skip TLS verification)
    pub fn with_insecure(mut self, insecure: bool) -> Self {
        self.insecure = insecure;
        self
    }

    /// Discover available transports from a relay
    ///
    /// This fetches the well-known endpoint and returns the discovery response.
    /// Falls back to QUIC-only if discovery fails.
    pub async fn discover(
        &self,
        host: &str,
        port: u16,
    ) -> Result<ProtocolDiscoveryResponse, TransportDiscoveryError> {
        // Try multiple ports to find the API server with the well-known endpoint
        // The QUIC control port might be different from the HTTPS/HTTP API port
        let ports_to_try = [
            port,  // Try the specified port first (for multi-protocol servers)
            3080,  // Common development HTTP port
            8080,  // Common HTTP port
            18080, // Alternate HTTP port
            80,    // Standard HTTP port
            443,   // Standard HTTPS port
            8443,  // Common HTTPS port
            18443, // Alternate HTTPS port
        ];

        for try_port in ports_to_try {
            // Try HTTPS first
            let url = format!("https://{}:{}{}", host, try_port, WELL_KNOWN_PATH);
            debug!("Trying protocol discovery from {}", url);

            if let Ok(response) = self.fetch_discovery(&url).await {
                info!(
                    "✅ Discovered {} transport(s) from relay on port {}",
                    response.transports.len(),
                    try_port
                );
                return Ok(response);
            }

            // If HTTPS fails, try HTTP (for development)
            let url_http = format!("http://{}:{}{}", host, try_port, WELL_KNOWN_PATH);
            debug!(
                "Trying protocol discovery from {} (HTTP fallback)",
                url_http
            );

            if let Ok(response) = self.fetch_discovery_http(&url_http).await {
                info!(
                    "✅ Discovered {} transport(s) from relay on port {} (HTTP)",
                    response.transports.len(),
                    try_port
                );
                return Ok(response);
            }
        }

        warn!("Protocol discovery failed on all ports, falling back to QUIC-only");
        // Return default QUIC-only response
        Ok(ProtocolDiscoveryResponse::quic_only(port))
    }

    /// Fetch discovery response from URL
    async fn fetch_discovery(
        &self,
        url: &str,
    ) -> Result<ProtocolDiscoveryResponse, TransportDiscoveryError> {
        // Use reqwest or a simple HTTP client
        // For now, we'll use a simple TCP + TLS + HTTP/1.1 implementation
        // to avoid adding heavy dependencies

        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::TcpStream;

        // Parse URL
        let url_parsed = url::Url::parse(url)
            .map_err(|e| TransportDiscoveryError::InvalidResponse(e.to_string()))?;

        let host = url_parsed
            .host_str()
            .ok_or_else(|| TransportDiscoveryError::InvalidResponse("No host".to_string()))?;
        let port = url_parsed.port().unwrap_or(443);
        let path = url_parsed.path();

        // Connect with timeout
        let addr = format!("{}:{}", host, port);
        let stream = tokio::time::timeout(self.timeout, TcpStream::connect(&addr))
            .await
            .map_err(|_| TransportDiscoveryError::Timeout)?
            .map_err(|e| TransportDiscoveryError::ConnectionFailed(e.to_string()))?;

        // TLS handshake
        let connector = if self.insecure {
            build_insecure_tls_connector()?
        } else {
            build_tls_connector()?
        };

        let dns_name = rustls::pki_types::ServerName::try_from(host.to_string())
            .map_err(|e| TransportDiscoveryError::ConnectionFailed(e.to_string()))?;

        let tls_stream = connector
            .connect(dns_name, stream)
            .await
            .map_err(|e| TransportDiscoveryError::ConnectionFailed(e.to_string()))?;

        // Send HTTP request
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/json\r\n\r\n",
            path, host
        );

        let (read_half, mut write_half) = tokio::io::split(tls_stream);

        write_half
            .write_all(request.as_bytes())
            .await
            .map_err(|e| TransportDiscoveryError::FetchFailed(e.to_string()))?;

        // Read response
        let mut reader = BufReader::new(read_half);
        let mut headers = String::new();
        let mut content_length: Option<usize> = None;

        // Read headers
        loop {
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .await
                .map_err(|e| TransportDiscoveryError::FetchFailed(e.to_string()))?;

            if line == "\r\n" || line.is_empty() {
                break;
            }

            // Parse content-length
            if line.to_lowercase().starts_with("content-length:") {
                if let Some(len_str) = line.split(':').nth(1) {
                    content_length = len_str.trim().parse().ok();
                }
            }

            headers.push_str(&line);
        }

        // Check for success status
        if !headers.starts_with("HTTP/1.1 200") && !headers.starts_with("HTTP/1.0 200") {
            return Err(TransportDiscoveryError::FetchFailed(format!(
                "HTTP error: {}",
                headers.lines().next().unwrap_or("unknown")
            )));
        }

        // Read body
        let body = if let Some(len) = content_length {
            let mut buf = vec![0u8; len];
            tokio::io::AsyncReadExt::read_exact(&mut reader, &mut buf)
                .await
                .map_err(|e| TransportDiscoveryError::FetchFailed(e.to_string()))?;
            String::from_utf8(buf)
                .map_err(|e| TransportDiscoveryError::InvalidResponse(e.to_string()))?
        } else {
            // Read until EOF
            let mut body = String::new();
            tokio::io::AsyncReadExt::read_to_string(&mut reader, &mut body)
                .await
                .map_err(|e| TransportDiscoveryError::FetchFailed(e.to_string()))?;
            body
        };

        // Parse JSON
        serde_json::from_str(&body)
            .map_err(|e| TransportDiscoveryError::InvalidResponse(e.to_string()))
    }

    /// Fetch discovery response from HTTP URL (no TLS)
    async fn fetch_discovery_http(
        &self,
        url: &str,
    ) -> Result<ProtocolDiscoveryResponse, TransportDiscoveryError> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::TcpStream;

        // Parse URL
        let url_parsed = url::Url::parse(url)
            .map_err(|e| TransportDiscoveryError::InvalidResponse(e.to_string()))?;

        let host = url_parsed
            .host_str()
            .ok_or_else(|| TransportDiscoveryError::InvalidResponse("No host".to_string()))?;
        let port = url_parsed.port().unwrap_or(80);
        let path = url_parsed.path();

        // Connect with timeout
        let addr = format!("{}:{}", host, port);
        let stream = tokio::time::timeout(self.timeout, TcpStream::connect(&addr))
            .await
            .map_err(|_| TransportDiscoveryError::Timeout)?
            .map_err(|e| TransportDiscoveryError::ConnectionFailed(e.to_string()))?;

        // Send HTTP request (no TLS)
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/json\r\n\r\n",
            path, host
        );

        let (read_half, mut write_half) = tokio::io::split(stream);

        write_half
            .write_all(request.as_bytes())
            .await
            .map_err(|e| TransportDiscoveryError::FetchFailed(e.to_string()))?;

        // Read response
        let mut reader = BufReader::new(read_half);
        let mut headers = String::new();
        let mut content_length: Option<usize> = None;

        // Read headers
        loop {
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .await
                .map_err(|e| TransportDiscoveryError::FetchFailed(e.to_string()))?;

            if line == "\r\n" || line.is_empty() {
                break;
            }

            // Parse content-length
            if line.to_lowercase().starts_with("content-length:") {
                if let Some(len_str) = line.split(':').nth(1) {
                    content_length = len_str.trim().parse().ok();
                }
            }

            headers.push_str(&line);
        }

        // Check for success status
        if !headers.starts_with("HTTP/1.1 200") && !headers.starts_with("HTTP/1.0 200") {
            return Err(TransportDiscoveryError::FetchFailed(format!(
                "HTTP error: {}",
                headers.lines().next().unwrap_or("unknown")
            )));
        }

        // Read body
        let body = if let Some(len) = content_length {
            let mut buf = vec![0u8; len];
            tokio::io::AsyncReadExt::read_exact(&mut reader, &mut buf)
                .await
                .map_err(|e| TransportDiscoveryError::FetchFailed(e.to_string()))?;
            String::from_utf8(buf)
                .map_err(|e| TransportDiscoveryError::InvalidResponse(e.to_string()))?
        } else {
            // Read until EOF
            let mut body = String::new();
            tokio::io::AsyncReadExt::read_to_string(&mut reader, &mut body)
                .await
                .map_err(|e| TransportDiscoveryError::FetchFailed(e.to_string()))?;
            body
        };

        // Parse JSON
        serde_json::from_str(&body)
            .map_err(|e| TransportDiscoveryError::InvalidResponse(e.to_string()))
    }

    /// Select the best transport from a discovery response
    pub fn select_best(
        &self,
        response: &ProtocolDiscoveryResponse,
        base_addr: SocketAddr,
        preferred: Option<TransportProtocol>,
    ) -> Result<DiscoveredTransport, TransportDiscoveryError> {
        // If preferred protocol specified, try to find it
        if let Some(pref) = preferred {
            if let Some(endpoint) = response.find_transport(pref) {
                let addr = SocketAddr::new(base_addr.ip(), endpoint.port);
                return Ok(DiscoveredTransport {
                    protocol: pref,
                    address: addr,
                    path: endpoint.path.clone(),
                    full_response: Some(response.clone()),
                });
            }
            warn!(
                "Preferred protocol {:?} not available, selecting best available",
                pref
            );
        }

        // Select best available
        let best = response
            .best_transport()
            .ok_or(TransportDiscoveryError::NoTransports)?;

        let addr = SocketAddr::new(base_addr.ip(), best.port);

        Ok(DiscoveredTransport {
            protocol: best.protocol,
            address: addr,
            path: best.path.clone(),
            full_response: Some(response.clone()),
        })
    }

    /// Discover and select the best transport in one call
    pub async fn discover_and_select(
        &self,
        host: &str,
        port: u16,
        base_addr: SocketAddr,
        preferred: Option<TransportProtocol>,
    ) -> Result<DiscoveredTransport, TransportDiscoveryError> {
        let response = self.discover(host, port).await?;
        self.select_best(&response, base_addr, preferred)
    }
}

impl Default for TransportDiscoverer {
    fn default() -> Self {
        Self::new()
    }
}

// Helper functions for TLS

fn build_tls_connector() -> Result<tokio_rustls::TlsConnector, TransportDiscoveryError> {
    ensure_crypto_provider();

    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    Ok(tokio_rustls::TlsConnector::from(std::sync::Arc::new(
        config,
    )))
}

fn build_insecure_tls_connector() -> Result<tokio_rustls::TlsConnector, TransportDiscoveryError> {
    ensure_crypto_provider();

    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(SkipVerification::new())
        .with_no_client_auth();

    Ok(tokio_rustls::TlsConnector::from(std::sync::Arc::new(
        config,
    )))
}

static CRYPTO_PROVIDER_INIT: std::sync::Once = std::sync::Once::new();

fn ensure_crypto_provider() {
    CRYPTO_PROVIDER_INIT.call_once(|| {
        if rustls::crypto::ring::default_provider()
            .install_default()
            .is_err()
        {
            // Already installed
        }
    });
}

// Insecure TLS verifier for development
#[derive(Debug)]
struct SkipVerification;

impl SkipVerification {
    fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self)
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
    fn test_discoverer_creation() {
        let discoverer = TransportDiscoverer::new();
        assert_eq!(discoverer.timeout, Duration::from_secs(5));
        assert!(!discoverer.insecure);
    }

    #[test]
    fn test_select_best_quic() {
        let discoverer = TransportDiscoverer::new();
        let response = ProtocolDiscoveryResponse::default()
            .with_quic(4443)
            .with_websocket(443, "/localup")
            .with_h2(443);

        let base_addr: SocketAddr = "127.0.0.1:443".parse().unwrap();
        let result = discoverer.select_best(&response, base_addr, None).unwrap();

        // QUIC should be selected as it has highest priority
        assert_eq!(result.protocol, TransportProtocol::Quic);
        assert_eq!(result.address.port(), 4443);
    }

    #[test]
    fn test_select_preferred() {
        let discoverer = TransportDiscoverer::new();
        let response = ProtocolDiscoveryResponse::default()
            .with_quic(4443)
            .with_websocket(443, "/localup");

        let base_addr: SocketAddr = "127.0.0.1:443".parse().unwrap();
        let result = discoverer
            .select_best(&response, base_addr, Some(TransportProtocol::WebSocket))
            .unwrap();

        // WebSocket should be selected when preferred
        assert_eq!(result.protocol, TransportProtocol::WebSocket);
        assert_eq!(result.address.port(), 443);
        assert_eq!(result.path, Some("/localup".to_string()));
    }
}
