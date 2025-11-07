//! QUIC transport implementation using quinn
//!
//! This crate provides a production-ready QUIC transport for the tunnel system,
//! leveraging quinn for high-performance multiplexed connections with built-in
//! TLS 1.3 encryption.
//!
//! # Features
//!
//! - **Encryption**: Mandatory TLS 1.3 (required by QUIC protocol)
//! - **Multiplexing**: Native support for multiple streams over single connection
//! - **0-RTT**: Fast reconnection with 0-RTT handshake support
//! - **Flow Control**: Per-stream and per-connection flow control
//! - **Congestion Control**: Built-in congestion control algorithms
//!
//! # Example
//!
//! ```no_run
//! use localup_transport_quic::{QuicTransportFactory, QuicConfig};
//! use localup_transport::TransportFactory;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Server side
//! let factory = QuicTransportFactory::new();
//! let config = Arc::new(QuicConfig::server_default("cert.pem", "key.pem")?);
//! let listener = factory.create_listener("0.0.0.0:4433".parse()?, config)?;
//!
//! // Client side
//! let config = Arc::new(QuicConfig::client_default());
//! let connector = factory.create_connector(config)?;
//! # Ok(())
//! # }
//! ```

// Initialize rustls crypto provider once globally
// This MUST be called before any rustls/QUIC operations
static CRYPTO_PROVIDER_INIT: std::sync::Once = std::sync::Once::new();

fn ensure_crypto_provider() {
    CRYPTO_PROVIDER_INIT.call_once(|| {
        if rustls::crypto::ring::default_provider()
            .install_default()
            .is_err()
        {
            // Provider already installed by another crate, this is fine
            tracing::debug!("Rustls crypto provider already installed");
        }
    });
}

pub mod config;
pub mod connection;
pub mod listener;
pub mod stream;

pub use config::QuicConfig;
pub use connection::QuicConnection;
pub use listener::{QuicConnector, QuicListener};
pub use stream::{QuicRecvHalf, QuicSendHalf, QuicStream};

use async_trait::async_trait;
use localup_transport::{TransportFactory, TransportResult};
use std::net::SocketAddr;
use std::sync::Arc;

/// QUIC transport factory
///
/// Creates QUIC listeners and connectors with quinn.
#[derive(Debug)]
pub struct QuicTransportFactory;

impl QuicTransportFactory {
    pub fn new() -> Self {
        Self
    }
}

impl Default for QuicTransportFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TransportFactory for QuicTransportFactory {
    type Listener = QuicListener;
    type Connector = QuicConnector;
    type Config = QuicConfig;

    fn create_listener(
        &self,
        bind_addr: SocketAddr,
        config: Arc<Self::Config>,
    ) -> TransportResult<Self::Listener> {
        QuicListener::new(bind_addr, config)
    }

    fn create_connector(&self, config: Arc<Self::Config>) -> TransportResult<Self::Connector> {
        QuicConnector::new(config)
    }

    fn name(&self) -> &str {
        "QUIC"
    }

    fn is_encrypted(&self) -> bool {
        true // QUIC always uses TLS 1.3
    }
}
