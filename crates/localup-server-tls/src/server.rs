//! TLS server with SNI routing
use std::net::SocketAddr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TlsServerError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct TlsServerConfig {
    pub bind_addr: SocketAddr,
}

impl Default for TlsServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:443".parse().unwrap(),
        }
    }
}

pub struct TlsServer {
    #[allow(dead_code)] // Will be used when TLS server implementation is complete
    config: TlsServerConfig,
}

impl TlsServer {
    pub fn new(config: TlsServerConfig) -> Self {
        Self { config }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_server_config() {
        let config = TlsServerConfig::default();
        assert_eq!(config.bind_addr.port(), 443);
    }
}
