//! Control plane for tunnel orchestration
pub mod agent_registry;
pub mod connection;
pub mod domain_provider;
pub mod handler;
pub mod pending_requests;
pub mod registry;
pub mod task_tracker;

pub use agent_registry::{AgentRegistry, RegisteredAgent};
pub use connection::{
    AgentConnection, AgentConnectionManager, TcpDataCallback, TunnelConnection,
    TunnelConnectionManager,
};
pub use domain_provider::{
    DomainContext, DomainProvider, DomainProviderError, RestrictedDomainProvider,
    SimpleCounterDomainProvider,
};
pub use handler::{PortAllocator, TcpProxySpawner, TunnelHandler};
pub use pending_requests::PendingRequests;
pub use registry::ControlPlane;
pub use task_tracker::TaskTracker;
