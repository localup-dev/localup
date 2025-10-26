//! QUIC listener and connector implementations

use async_trait::async_trait;
use quinn::Endpoint;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, error, info};
use tunnel_transport::{
    TransportConfig, TransportConnector, TransportError, TransportListener, TransportResult,
};

use crate::config::QuicConfig;
use crate::connection::QuicConnection;

/// QUIC listener for accepting incoming connections
#[derive(Debug)]
pub struct QuicListener {
    endpoint: Endpoint,
    // Kept for potential future revalidation or reconfiguration
    _config: Arc<QuicConfig>,
}

impl QuicListener {
    pub fn new(bind_addr: SocketAddr, config: Arc<QuicConfig>) -> TransportResult<Self> {
        TransportConfig::validate(&*config)?;

        let server_config = config.build_server_config()?;

        let endpoint =
            Endpoint::server(server_config, bind_addr).map_err(TransportError::IoError)?;

        let local_addr = endpoint.local_addr().map_err(TransportError::IoError)?;

        info!("QUIC listener bound to {}", local_addr);

        Ok(Self {
            endpoint,
            _config: config,
        })
    }
}

#[async_trait]
impl TransportListener for QuicListener {
    type Connection = QuicConnection;

    async fn accept(&self) -> TransportResult<(Self::Connection, SocketAddr)> {
        loop {
            match self.endpoint.accept().await {
                Some(connecting) => {
                    let remote = connecting.remote_address();

                    debug!("Incoming QUIC connection from {}", remote);

                    match connecting.await {
                        Ok(connection) => {
                            info!("QUIC connection established from {}", remote);
                            return Ok((QuicConnection::new(connection), remote));
                        }
                        Err(e) => {
                            error!("Failed to establish QUIC connection from {}: {}", remote, e);
                            // Continue to accept next connection
                            continue;
                        }
                    }
                }
                None => {
                    // Endpoint is closed
                    return Err(TransportError::ConnectionError(
                        "QUIC endpoint closed".to_string(),
                    ));
                }
            }
        }
    }

    fn local_addr(&self) -> TransportResult<SocketAddr> {
        self.endpoint.local_addr().map_err(TransportError::IoError)
    }

    async fn close(&self) {
        self.endpoint.close(0u32.into(), b"Listener closed");
        info!("QUIC listener closed");
    }
}

/// QUIC connector for establishing outgoing connections
#[derive(Debug)]
pub struct QuicConnector {
    endpoint: Endpoint,
    // Kept for potential future reconfiguration
    _config: Arc<QuicConfig>,
}

impl QuicConnector {
    pub fn new(config: Arc<QuicConfig>) -> TransportResult<Self> {
        TransportConfig::validate(&*config)?;

        let client_config = config.build_client_config()?;

        let mut endpoint =
            Endpoint::client("0.0.0.0:0".parse().unwrap()).map_err(TransportError::IoError)?;

        endpoint.set_default_client_config(client_config);

        debug!("QUIC connector created");

        Ok(Self {
            endpoint,
            _config: config,
        })
    }
}

#[async_trait]
impl TransportConnector for QuicConnector {
    type Connection = QuicConnection;

    async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
    ) -> TransportResult<Self::Connection> {
        debug!("Connecting to QUIC server: {} ({})", server_name, addr);

        let connecting = self
            .endpoint
            .connect(addr, server_name)
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;

        let connection = connecting
            .await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;

        info!("QUIC connection established to {} ({})", server_name, addr);

        Ok(QuicConnection::new(connection))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quic_config() {
        let config = QuicConfig::client_default();
        assert!(config.validate().is_ok());
    }

    // Note: Full integration tests require actual QUIC handshakes
    // and are better suited for the integration test suite
}
