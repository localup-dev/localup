//! Transport trait for tunnel connections

use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;

/// Transport errors
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("Connection closed")]
    ConnectionClosed,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("Timeout")]
    Timeout,

    #[error("Protocol error: {0}")]
    ProtocolError(String),
}

/// Transport trait for tunnel connections
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send data through the transport
    async fn send(&mut self, data: Bytes) -> Result<(), TransportError>;

    /// Receive data from the transport
    async fn recv(&mut self) -> Result<Option<Bytes>, TransportError>;

    /// Close the transport
    async fn close(&mut self) -> Result<(), TransportError>;

    /// Check if transport is connected
    fn is_connected(&self) -> bool;
}
