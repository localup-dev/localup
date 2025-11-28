//! WebSocket transport implementation using tokio-tungstenite
//!
//! This crate provides a WebSocket transport for the tunnel system,
//! designed for environments where QUIC/UDP might be blocked.
//!
//! # Features
//!
//! - **Encryption**: TLS via rustls (wss://)
//! - **Multiplexing**: Stream multiplexing over single WebSocket connection
//! - **Firewall Friendly**: Uses TCP port 443, passes through most firewalls
//! - **HTTP Compatible**: Can coexist with HTTP servers on same port
//!
//! # Stream Multiplexing
//!
//! Since WebSocket doesn't have native stream multiplexing like QUIC,
//! we implement a simple multiplexing protocol:
//!
//! Each WebSocket message is prefixed with:
//! - 4 bytes: stream ID (big-endian u32)
//! - 1 byte: message type (0=data, 1=open, 2=close)
//! - Rest: payload

pub mod config;
pub mod connection;
pub mod listener;
pub mod stream;

pub use config::WebSocketConfig;
pub use connection::WebSocketConnection;
pub use listener::{WebSocketConnector, WebSocketListener};
pub use stream::WebSocketStream;

use async_trait::async_trait;
use localup_transport::{TransportFactory, TransportResult};
use std::net::SocketAddr;
use std::sync::Arc;

/// WebSocket transport factory
#[derive(Debug)]
pub struct WebSocketTransportFactory;

impl WebSocketTransportFactory {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebSocketTransportFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TransportFactory for WebSocketTransportFactory {
    type Listener = WebSocketListener;
    type Connector = WebSocketConnector;
    type Config = WebSocketConfig;

    fn create_listener(
        &self,
        bind_addr: SocketAddr,
        config: Arc<Self::Config>,
    ) -> TransportResult<Self::Listener> {
        WebSocketListener::new(bind_addr, config)
    }

    fn create_connector(&self, config: Arc<Self::Config>) -> TransportResult<Self::Connector> {
        WebSocketConnector::new(config)
    }

    fn name(&self) -> &str {
        "WebSocket"
    }

    fn is_encrypted(&self) -> bool {
        true // Always use wss://
    }
}
