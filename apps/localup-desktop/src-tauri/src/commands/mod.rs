//! Tauri IPC commands for LocalUp Desktop

pub mod daemon;
pub mod relays;
pub mod settings;
pub mod tunnels;

// Re-export commands
pub use daemon::*;
pub use relays::*;
pub use settings::*;
pub use tunnels::*;
