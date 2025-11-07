//! Connection management using QUIC
//!
//! Provides QUIC-based connection management with native multiplexing via quinn.

pub mod connection;
pub mod reconnect;

pub use connection::{QuicConnection, QuicStream};
pub use reconnect::{ReconnectConfig, ReconnectManager};
