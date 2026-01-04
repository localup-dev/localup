//! Daemon module for running tunnels independently of the Tauri app
//!
//! This module provides a daemon process that manages tunnels and communicates
//! with the Tauri app via IPC (Unix socket on macOS/Linux, named pipe on Windows).

pub mod client;
pub mod protocol;
pub mod service;

pub use client::DaemonClient;
pub use protocol::{DaemonRequest, DaemonResponse, TunnelInfo};
pub use service::DaemonService;

use std::path::PathBuf;

/// Get the path to the daemon socket
pub fn socket_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(r"\\.\pipe\localup-daemon")
    }

    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".localup").join("daemon.sock")
    }
}

/// Get the path to the daemon PID file
pub fn pid_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".localup").join("daemon.pid")
}

/// Get the path to the daemon log file
pub fn log_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".localup").join("daemon.log")
}

/// Get the path to the daemon database
pub fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".localup").join("tunnels.db")
}
