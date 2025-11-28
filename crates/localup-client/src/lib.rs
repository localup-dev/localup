//! Tunnel client library - Public API
//!
//! This is the main library that developers integrate into their applications.

pub mod client;
pub mod config;
pub mod metrics;
pub mod metrics_db;
pub mod metrics_server;
pub mod metrics_service;
pub mod relay_discovery;

pub use client::{TunnelClient, TunnelError};
pub use config::{ProtocolConfig, TunnelConfig};
pub use metrics::{
    BodyContent, BodyData, HttpMetric, MetricsStats, MetricsStore, TcpConnectionState, TcpMetric,
};
pub use metrics_server::MetricsServer;
pub use relay_discovery::{RelayDiscovery, RelayEndpoint, RelayError, RelayInfo};

pub use localup_proto::{Endpoint, ExitNodeConfig, Protocol, Region};
#[cfg(feature = "db-metrics")]
pub use metrics_db::DbMetricsStore;

pub mod localup;
pub use localup::{TunnelConnection, TunnelConnector};

pub mod transport_discovery;
pub use transport_discovery::{DiscoveredTransport, TransportDiscoverer, TransportDiscoveryError};

pub mod reverse_tunnel;
pub use reverse_tunnel::{ReverseTunnelClient, ReverseTunnelConfig, ReverseTunnelError};
