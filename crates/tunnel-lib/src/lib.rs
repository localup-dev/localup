//! Tunnel Library - Public API for Rust applications using the geo-distributed tunnel system
//!
//! This library re-exports all the tunnel crates, providing a unified entry point
//! for Rust applications that want to integrate tunnel functionality (either as clients or relay servers).
//!
//! # Quick Start - Tunnel Client
//!
//! ```ignore
//! use tunnel_lib::{TunnelClient, TunnelConfig, ExitNodeConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = TunnelConfig {
//!         local_host: "127.0.0.1".to_string(),
//!         exit_node: ExitNodeConfig::Custom("localhost:4443".to_string()),
//!         ..Default::default()
//!     };
//!
//!     let client = TunnelClient::connect(config).await?;
//!
//!     if let Some(url) = client.public_url() {
//!         println!("Tunnel URL: {}", url);
//!     }
//!
//!     client.wait().await?;
//!     Ok(())
//! }
//! ```
//!
//! # Architecture
//!
//! The tunnel system is composed of several focused crates:
//!
//! - **`tunnel-proto`**: Protocol definitions and message types
//! - **`tunnel-transport`**: Transport abstraction (QUIC, TCP, etc.)
//! - **`tunnel-client`**: Tunnel client library
//! - **`tunnel-control`**: Control plane for tunnel management
//! - **`tunnel-auth`**: Authentication and JWT handling
//! - **`tunnel-router`**: Routing logic (TCP port, SNI, HTTP host)
//! - **`tunnel-server-*`**: Protocol-specific servers (TCP, TLS, HTTP, HTTPS)
//! - **`tunnel-cert`**: Certificate management and ACME
//! - **`tunnel-relay-db`**: Database layer for traffic inspection
//!
//! All types from these crates are re-exported here for convenience.

// Re-export protocol types
pub use tunnel_proto::{Endpoint, Protocol, TunnelConfig as ProtoTunnelConfig, TunnelMessage};

// Re-export transport layer
pub use tunnel_transport::{TransportConnection, TransportError, TransportListener};
pub use tunnel_transport_quic::{QuicConfig, QuicConnection, QuicConnector, QuicListener};

// Re-export client types (primary API for tunnel clients)
pub use tunnel_client::{
    ExitNodeConfig, MetricsStore, ProtocolConfig, Region, TunnelClient, TunnelConfig, TunnelError,
};

// Re-export control plane types (for building custom relays)
pub use tunnel_control::{PendingRequests, TunnelConnectionManager, TunnelHandler};

// Re-export server types (for building custom relays)
pub use tunnel_server_https::{HttpsServer, HttpsServerConfig};
pub use tunnel_server_tcp::{TcpServer, TcpServerConfig, TcpServerError};
pub use tunnel_server_tcp_proxy::{TcpProxyServer, TcpProxyServerConfig};
pub use tunnel_server_tls::{TlsServer, TlsServerConfig};

// Re-export router types
pub use tunnel_router::{RouteKey, RouteRegistry, RouteTarget};

// Re-export auth types
pub use tunnel_auth::{JwtClaims, JwtError, JwtValidator};

// Re-export certificate types
pub use tunnel_cert::{Certificate, SelfSignedCertificate};
