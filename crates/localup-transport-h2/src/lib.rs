//! HTTP/2 transport implementation using h2
//!
//! This crate provides an HTTP/2 transport for the tunnel system,
//! designed for environments where QUIC/UDP might be blocked.
//!
//! # Features
//!
//! - **Encryption**: TLS via rustls
//! - **Multiplexing**: Native HTTP/2 stream multiplexing
//! - **Firewall Friendly**: Uses TCP port 443, passes through all firewalls
//! - **Standard Protocol**: HTTP/2 is universally supported
//!
//! # Stream Mapping
//!
//! HTTP/2 streams map naturally to our transport streams:
//! - Each tunnel stream = one HTTP/2 bidirectional stream
//! - Data is sent as DATA frames
//! - Stream close = END_STREAM flag

pub mod config;
pub mod connection;
pub mod listener;
pub mod stream;

pub use config::H2Config;
pub use connection::H2Connection;
pub use listener::{H2Connector, H2Listener};
pub use stream::H2Stream;

use async_trait::async_trait;
use localup_transport::{TransportFactory, TransportResult};
use std::net::SocketAddr;
use std::sync::Arc;

/// HTTP/2 transport factory
#[derive(Debug)]
pub struct H2TransportFactory;

impl H2TransportFactory {
    pub fn new() -> Self {
        Self
    }
}

impl Default for H2TransportFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TransportFactory for H2TransportFactory {
    type Listener = H2Listener;
    type Connector = H2Connector;
    type Config = H2Config;

    fn create_listener(
        &self,
        bind_addr: SocketAddr,
        config: Arc<Self::Config>,
    ) -> TransportResult<Self::Listener> {
        H2Listener::new(bind_addr, config)
    }

    fn create_connector(&self, config: Arc<Self::Config>) -> TransportResult<Self::Connector> {
        H2Connector::new(config)
    }

    fn name(&self) -> &str {
        "HTTP/2"
    }

    fn is_encrypted(&self) -> bool {
        true // Always use TLS
    }
}
