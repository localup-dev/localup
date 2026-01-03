//! Application state management

pub mod app_state;
pub mod tunnel_manager;

pub use app_state::AppState;
pub use tunnel_manager::{TunnelManager, TunnelStatus};
