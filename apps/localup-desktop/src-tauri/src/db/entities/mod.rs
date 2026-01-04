//! SeaORM entities for LocalUp Desktop

pub mod captured_request;
pub mod relay_server;
pub mod setting;
pub mod tunnel_config;
pub mod tunnel_session;

pub use captured_request::Entity as CapturedRequest;
pub use relay_server::Entity as RelayServer;
pub use setting::Entity as Setting;
pub use tunnel_config::Entity as TunnelConfig;
#[allow(unused_imports)]
pub use tunnel_session::Entity as TunnelSession;
