//! TCP Proxy Server
//!
//! This crate implements a TCP proxy server that forwards raw TCP connections through tunnels.
//! Each tunnel gets its own dedicated port on the exit node.

mod server;

pub use server::{TcpProxyServer, TcpProxyServerConfig, TcpProxyServerError};
