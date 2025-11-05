//! Control plane for tunnel orchestration
pub mod agent_registry;
pub mod connection;
pub mod handler;
pub mod pending_requests;
pub mod registry;

pub use agent_registry::{AgentRegistry, RegisteredAgent};
pub use connection::{
    AgentConnection, AgentConnectionManager, TcpDataCallback, TunnelConnection,
    TunnelConnectionManager,
};
pub use handler::{PortAllocator, TcpProxySpawner, TunnelHandler};
pub use pending_requests::PendingRequests;
pub use registry::ControlPlane;
