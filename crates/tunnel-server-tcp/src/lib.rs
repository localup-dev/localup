//! TCP tunnel server
//!
//! Handles raw TCP connections and proxies them through QUIC tunnels.

pub mod proxy;
pub mod server;

pub use proxy::TcpProxy;
pub use server::{TcpServer, TcpServerConfig, TcpServerError};
