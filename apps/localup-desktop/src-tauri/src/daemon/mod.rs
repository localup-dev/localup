//! Daemon module for running tunnels independently of the Tauri app
//!
//! This module provides a daemon process that manages tunnels and communicates
//! with the Tauri app via IPC using TCP sockets on localhost.

pub mod client;
pub mod protocol;
pub mod service;

pub use client::DaemonClient;
pub use protocol::{DaemonRequest, DaemonResponse, TunnelInfo};
pub use service::DaemonService;

use std::path::PathBuf;

/// Default port for daemon IPC communication
pub const DAEMON_PORT: u16 = 19274;

/// Get the daemon address for IPC communication
pub fn daemon_addr() -> String {
    format!("127.0.0.1:{}", DAEMON_PORT)
}

/// Get the localup data directory (cross-platform)
fn localup_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        // Use LOCALAPPDATA on Windows (e.g., C:\Users\<user>\AppData\Local\localup)
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
            std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".to_string())
        });
        PathBuf::from(local_app_data).join("localup")
    }

    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".localup")
    }
}

/// Get the path to the daemon PID file
pub fn pid_path() -> PathBuf {
    localup_dir().join("daemon.pid")
}

/// Get the path to the daemon log file
pub fn log_path() -> PathBuf {
    localup_dir().join("daemon.log")
}

/// Get the path to the daemon database
pub fn db_path() -> PathBuf {
    localup_dir().join("tunnels.db")
}

/// Ensure the localup data directory exists
pub fn ensure_localup_dir() -> std::io::Result<()> {
    std::fs::create_dir_all(localup_dir())
}
