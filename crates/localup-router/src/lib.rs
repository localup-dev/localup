//! Routing logic for tunnel protocols
//!
//! Handles TCP port-based routing, TLS SNI routing, and HTTP host-based routing.
//! Supports wildcard domain patterns (e.g., `*.example.com`) with fallback matching.

pub mod http;
pub mod registry;
pub mod sni;
pub mod tcp;
pub mod wildcard;

pub use http::{HttpRoute, HttpRouter};
pub use registry::{RouteRegistry, RouteTarget};
pub use sni::{SniRoute, SniRouter};
pub use tcp::{TcpRoute, TcpRouter};
pub use wildcard::{extract_parent_wildcard, WildcardError, WildcardPattern};

/// Route key for identifying connections
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RouteKey {
    /// TCP routing by port
    TcpPort(u16),
    /// TLS routing by SNI hostname
    TlsSni(String),
    /// HTTP routing by host header
    HttpHost(String),
}
