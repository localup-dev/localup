//! Daemon management commands
//!
//! These commands allow the Tauri app to interact with the daemon service.

use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::daemon::{DaemonClient, TunnelInfo};

/// Daemon status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub running: bool,
    pub version: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub tunnel_count: Option<usize>,
}

/// Check if the daemon is running and get its status
#[tauri::command]
pub async fn get_daemon_status() -> Result<DaemonStatus, String> {
    match DaemonClient::connect().await {
        Ok(mut client) => match client.ping().await {
            Ok((version, uptime, tunnel_count)) => Ok(DaemonStatus {
                running: true,
                version: Some(version),
                uptime_seconds: Some(uptime),
                tunnel_count: Some(tunnel_count),
            }),
            Err(e) => {
                error!("Daemon ping failed: {}", e);
                Ok(DaemonStatus {
                    running: false,
                    version: None,
                    uptime_seconds: None,
                    tunnel_count: None,
                })
            }
        },
        Err(_) => Ok(DaemonStatus {
            running: false,
            version: None,
            uptime_seconds: None,
            tunnel_count: None,
        }),
    }
}

/// Start the daemon if not running
#[tauri::command]
pub async fn start_daemon() -> Result<DaemonStatus, String> {
    info!("Starting daemon...");

    match DaemonClient::connect_or_start().await {
        Ok(mut client) => match client.ping().await {
            Ok((version, uptime, tunnel_count)) => {
                info!("Daemon started successfully: v{}", version);
                Ok(DaemonStatus {
                    running: true,
                    version: Some(version),
                    uptime_seconds: Some(uptime),
                    tunnel_count: Some(tunnel_count),
                })
            }
            Err(e) => Err(format!("Daemon started but ping failed: {}", e)),
        },
        Err(e) => Err(format!("Failed to start daemon: {}", e)),
    }
}

/// Stop the daemon
#[tauri::command]
pub async fn stop_daemon() -> Result<(), String> {
    info!("Stopping daemon...");

    DaemonClient::stop_daemon()
        .await
        .map_err(|e| format!("Failed to stop daemon: {}", e))?;

    info!("Daemon stopped");
    Ok(())
}

/// List tunnels from the daemon
#[tauri::command]
pub async fn daemon_list_tunnels() -> Result<Vec<TunnelInfo>, String> {
    let mut client = DaemonClient::connect()
        .await
        .map_err(|e| format!("Failed to connect to daemon: {}", e))?;

    client
        .list_tunnels()
        .await
        .map_err(|e| format!("Failed to list tunnels: {}", e))
}

/// Get a tunnel from the daemon
#[tauri::command]
pub async fn daemon_get_tunnel(id: String) -> Result<TunnelInfo, String> {
    let mut client = DaemonClient::connect()
        .await
        .map_err(|e| format!("Failed to connect to daemon: {}", e))?;

    client
        .get_tunnel(&id)
        .await
        .map_err(|e| format!("Failed to get tunnel: {}", e))
}

/// Start a tunnel via the daemon
#[tauri::command]
pub async fn daemon_start_tunnel(
    id: String,
    name: String,
    relay_address: String,
    auth_token: String,
    local_host: String,
    local_port: u16,
    protocol: String,
    subdomain: Option<String>,
    custom_domain: Option<String>,
) -> Result<TunnelInfo, String> {
    let mut client = DaemonClient::connect_or_start()
        .await
        .map_err(|e| format!("Failed to connect to daemon: {}", e))?;

    client
        .start_tunnel(
            &id,
            &name,
            &relay_address,
            &auth_token,
            &local_host,
            local_port,
            &protocol,
            subdomain.as_deref(),
            custom_domain.as_deref(),
        )
        .await
        .map_err(|e| format!("Failed to start tunnel: {}", e))
}

/// Stop a tunnel via the daemon
#[tauri::command]
pub async fn daemon_stop_tunnel(id: String) -> Result<(), String> {
    let mut client = DaemonClient::connect()
        .await
        .map_err(|e| format!("Failed to connect to daemon: {}", e))?;

    client
        .stop_tunnel(&id)
        .await
        .map_err(|e| format!("Failed to stop tunnel: {}", e))
}

/// Delete a tunnel via the daemon
#[tauri::command]
pub async fn daemon_delete_tunnel(id: String) -> Result<(), String> {
    let mut client = DaemonClient::connect()
        .await
        .map_err(|e| format!("Failed to connect to daemon: {}", e))?;

    client
        .delete_tunnel(&id)
        .await
        .map_err(|e| format!("Failed to delete tunnel: {}", e))
}

/// Get daemon logs (last N lines)
#[tauri::command]
pub async fn get_daemon_logs(lines: Option<usize>) -> Result<String, String> {
    let log_path = crate::daemon::log_path();
    let lines = lines.unwrap_or(100);

    if !log_path.exists() {
        return Ok(String::new());
    }

    // Read the log file
    let content =
        std::fs::read_to_string(&log_path).map_err(|e| format!("Failed to read logs: {}", e))?;

    // Get last N lines
    let log_lines: Vec<&str> = content.lines().collect();
    let start = log_lines.len().saturating_sub(lines);
    let result = log_lines[start..].join("\n");

    // Strip ANSI escape codes for cleaner display
    let ansi_regex = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    Ok(ansi_regex.replace_all(&result, "").to_string())
}
