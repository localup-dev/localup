//! LocalUp Agent-Server
//!
//! Standalone agent-server that combines relay and agent functionality.
//! Perfect for VPN scenarios where you want to expose internal services
//! without running a separate relay.

pub mod access_control;
pub mod server;

pub use access_control::{AccessControl, PortRange};
pub use server::{AgentServer, AgentServerConfig};
