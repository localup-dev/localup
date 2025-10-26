//! Transport abstraction layer for tunnel connections
//!
//! This crate provides transport-agnostic traits that allow the tunnel system
//! to work with different underlying protocols (QUIC, WebSocket, TCP+TLS, etc.)
//! without coupling to any specific implementation.
//!
//! # Design Principles
//!
//! 1. **Transport Independence**: Core tunnel logic should not depend on specific transports
//! 2. **Easy Migration**: Switching transports should only require changing configuration
//! 3. **Security by Default**: All transports must provide encryption
//! 4. **Performance**: Traits should allow zero-cost abstractions where possible
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                  Tunnel Application                      │
//! │         (exit-node, client, control plane)               │
//! └─────────────────────────────────────────────────────────┘
//!                           │
//!                           │ Uses traits
//!                           ↓
//! ┌─────────────────────────────────────────────────────────┐
//! │            tunnel-transport (this crate)                 │
//! │  - TransportListener    - TransportConnection            │
//! │  - TransportStream      - TransportConnector             │
//! └─────────────────────────────────────────────────────────┘
//!                           │
//!                           │ Implemented by
//!                           ↓
//! ┌──────────────┬──────────────┬──────────────┬────────────┐
//! │ tunnel-      │ tunnel-      │ tunnel-      │  Future    │
//! │ transport-   │ transport-   │ transport-   │ transports │
//! │ quic         │ websocket    │ tcp-tls      │            │
//! └──────────────┴──────────────┴──────────────┴────────────┘
//! ```

use async_trait::async_trait;
use bytes::Bytes;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tunnel_proto::TunnelMessage;

/// Transport-level errors
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Stream closed")]
    StreamClosed,

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Timeout")]
    Timeout,

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("TLS error: {0}")]
    TlsError(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),
}

/// Result type for transport operations
pub type TransportResult<T> = Result<T, TransportError>;

/// A bidirectional stream over a transport connection
///
/// Represents a single logical stream that can send and receive messages.
/// In QUIC, this maps to a bidirectional stream. In WebSocket, this might
/// be multiplexed over a single WebSocket connection with stream IDs.
#[async_trait]
pub trait TransportStream: Send + Sync + Debug {
    /// Send a tunnel message on this stream
    async fn send_message(&mut self, message: &TunnelMessage) -> TransportResult<()>;

    /// Receive a tunnel message from this stream
    ///
    /// Returns `None` if the stream has been closed gracefully by the remote peer.
    async fn recv_message(&mut self) -> TransportResult<Option<TunnelMessage>>;

    /// Send raw bytes (for protocol-level data that doesn't use TunnelMessage)
    async fn send_bytes(&mut self, data: &[u8]) -> TransportResult<()>;

    /// Receive raw bytes (up to max_size)
    ///
    /// Returns empty bytes if stream is closed.
    async fn recv_bytes(&mut self, max_size: usize) -> TransportResult<Bytes>;

    /// Close the sending side of the stream
    async fn finish(&mut self) -> TransportResult<()>;

    /// Get the stream ID (unique within this connection)
    fn stream_id(&self) -> u64;

    /// Check if the stream is closed
    fn is_closed(&self) -> bool;
}

/// A transport connection that can create multiple streams
///
/// Represents a single connection to a remote peer. Connections can be
/// multiplexed (QUIC) or require external multiplexing (TCP).
#[async_trait]
pub trait TransportConnection: Send + Sync + Debug {
    /// The stream type created by this connection
    type Stream: TransportStream;

    /// Open a new bidirectional stream
    async fn open_stream(&self) -> TransportResult<Self::Stream>;

    /// Accept an incoming bidirectional stream
    ///
    /// Returns `None` when the connection is closed and no more streams will arrive.
    async fn accept_stream(&self) -> TransportResult<Option<Self::Stream>>;

    /// Close the connection gracefully
    ///
    /// # Arguments
    /// * `error_code` - Application-specific error code (0 for normal closure)
    /// * `reason` - Human-readable reason for closure
    async fn close(&self, error_code: u32, reason: &str);

    /// Check if the connection is closed
    fn is_closed(&self) -> bool;

    /// Get the remote peer address
    fn remote_address(&self) -> SocketAddr;

    /// Get connection statistics
    fn stats(&self) -> ConnectionStats;

    /// Get a unique stable identifier for this connection
    ///
    /// This ID should remain stable across the lifetime of the connection
    /// and can be used for logging, metrics, and correlation.
    fn connection_id(&self) -> String;
}

/// Statistics about a connection
#[derive(Debug, Clone, Default)]
pub struct ConnectionStats {
    /// Number of bytes sent
    pub bytes_sent: u64,

    /// Number of bytes received
    pub bytes_received: u64,

    /// Number of active streams
    pub active_streams: usize,

    /// Round-trip time estimate (milliseconds)
    pub rtt_ms: Option<u32>,

    /// Connection uptime (seconds)
    pub uptime_secs: u64,
}

/// Server-side: Listens for incoming transport connections
///
/// This is used by exit nodes to accept incoming tunnel connections from clients.
#[async_trait]
pub trait TransportListener: Send + Sync + Debug {
    /// The connection type accepted by this listener
    type Connection: TransportConnection;

    /// Accept an incoming connection
    ///
    /// Returns the connection and the remote address of the connecting peer.
    async fn accept(&self) -> TransportResult<(Self::Connection, SocketAddr)>;

    /// Get the local address this listener is bound to
    fn local_addr(&self) -> TransportResult<SocketAddr>;

    /// Close the listener (stop accepting new connections)
    async fn close(&self);
}

/// Client-side: Establishes outgoing transport connections
///
/// This is used by tunnel clients to connect to exit nodes.
#[async_trait]
pub trait TransportConnector: Send + Sync + Debug {
    /// The connection type created by this connector
    type Connection: TransportConnection;

    /// Connect to a remote server
    ///
    /// # Arguments
    /// * `addr` - The socket address to connect to
    /// * `server_name` - The server name for TLS verification (e.g., "tunnel.example.com")
    async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
    ) -> TransportResult<Self::Connection>;
}

/// Configuration for transport security
#[derive(Debug, Clone)]
pub struct TransportSecurityConfig {
    /// Whether to verify the server's TLS certificate
    pub verify_server_cert: bool,

    /// Optional client certificate for mutual TLS
    pub client_cert: Option<ClientCertificate>,

    /// Custom root CA certificates (if not using system roots)
    pub root_certs: Vec<Vec<u8>>,

    /// Application-Layer Protocol Negotiation (ALPN) protocols
    pub alpn_protocols: Vec<String>,
}

impl Default for TransportSecurityConfig {
    fn default() -> Self {
        Self {
            verify_server_cert: true,
            client_cert: None,
            root_certs: Vec::new(),
            alpn_protocols: vec!["tunnel-v1".to_string()],
        }
    }
}

/// Client certificate for mutual TLS
#[derive(Debug, Clone)]
pub struct ClientCertificate {
    /// Certificate chain (PEM or DER encoded)
    pub cert_chain: Vec<u8>,

    /// Private key (PEM or DER encoded)
    pub private_key: Vec<u8>,
}

/// Transport-specific configuration
///
/// Each transport implementation can define its own configuration type
/// that implements this trait.
pub trait TransportConfig: Send + Sync + Debug {
    /// Get the security configuration
    fn security_config(&self) -> &TransportSecurityConfig;

    /// Validate the configuration
    fn validate(&self) -> TransportResult<()>;
}

/// Factory for creating transport listeners and connectors
///
/// This allows dynamically selecting transport implementations at runtime.
pub trait TransportFactory: Send + Sync + Debug {
    /// The listener type created by this factory
    type Listener: TransportListener;

    /// The connector type created by this factory
    type Connector: TransportConnector;

    /// The configuration type for this transport
    type Config: TransportConfig;

    /// Create a new listener bound to the given address
    fn create_listener(
        &self,
        bind_addr: SocketAddr,
        config: Arc<Self::Config>,
    ) -> TransportResult<Self::Listener>;

    /// Create a new connector
    fn create_connector(&self, config: Arc<Self::Config>) -> TransportResult<Self::Connector>;

    /// Get a human-readable name for this transport (e.g., "QUIC", "WebSocket+TLS")
    fn name(&self) -> &str;

    /// Check if this transport provides encryption
    fn is_encrypted(&self) -> bool;
}

// Test module with mock implementations
#[cfg(test)]
pub mod tests;

#[cfg(test)]
mod simple_tests {
    use super::*;

    #[test]
    fn test_transport_security_config_default() {
        let config = TransportSecurityConfig::default();
        assert!(config.verify_server_cert);
        assert!(config.client_cert.is_none());
        assert_eq!(config.alpn_protocols, vec!["tunnel-v1"]);
    }

    #[test]
    fn test_connection_stats_default() {
        let stats = ConnectionStats::default();
        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.bytes_received, 0);
        assert_eq!(stats.active_streams, 0);
    }
}
