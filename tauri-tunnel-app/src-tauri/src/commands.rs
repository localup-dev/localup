//! Tauri IPC Commands
//!
//! This module defines the commands that can be invoked from the frontend.

use crate::db::DatabaseService;
use crate::tunnel_manager::{TunnelConfigDto, TunnelInfo, TunnelManager};
use std::sync::Arc;
use tauri::State;
use tunnel_lib::{HttpMetric, MetricsStats, TcpMetric};

/// Application state passed to commands
pub struct AppState {
    pub tunnel_manager: Arc<TunnelManager>,
    pub db: Arc<DatabaseService>,
}

/// Create a new tunnel
#[tauri::command]
pub async fn create_tunnel(
    config: TunnelConfigDto,
    state: State<'_, AppState>,
) -> Result<TunnelInfo, String> {
    state.tunnel_manager.create_tunnel(config).await
}

/// Stop a tunnel
#[tauri::command]
pub async fn stop_tunnel(tunnel_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.tunnel_manager.stop_tunnel(&tunnel_id).await
}

/// Get all tunnels
#[tauri::command]
pub async fn list_tunnels(state: State<'_, AppState>) -> Result<Vec<TunnelInfo>, String> {
    Ok(state.tunnel_manager.list_tunnels().await)
}

/// Get a specific tunnel
#[tauri::command]
pub async fn get_tunnel(
    tunnel_id: String,
    state: State<'_, AppState>,
) -> Result<Option<TunnelInfo>, String> {
    Ok(state.tunnel_manager.get_tunnel(&tunnel_id).await)
}

/// Get metrics for a tunnel
#[tauri::command]
pub async fn get_tunnel_metrics(
    tunnel_id: String,
    state: State<'_, AppState>,
) -> Result<Option<MetricsStats>, String> {
    if let Some(metrics) = state.tunnel_manager.get_metrics(&tunnel_id).await {
        Ok(Some(metrics.get_stats().await))
    } else {
        Ok(None)
    }
}

/// Get HTTP requests for a tunnel
#[tauri::command]
pub async fn get_tunnel_requests(
    tunnel_id: String,
    offset: usize,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<HttpMetric>, String> {
    if let Some(metrics) = state.tunnel_manager.get_metrics(&tunnel_id).await {
        Ok(metrics.get_paginated(offset, limit).await)
    } else {
        Ok(Vec::new())
    }
}

/// Get TCP connections for a tunnel
#[tauri::command]
pub async fn get_tunnel_tcp_connections(
    tunnel_id: String,
    offset: usize,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<TcpMetric>, String> {
    if let Some(metrics) = state.tunnel_manager.get_metrics(&tunnel_id).await {
        Ok(metrics.get_tcp_connections_paginated(offset, limit).await)
    } else {
        Ok(Vec::new())
    }
}

/// Clear metrics for a tunnel
#[tauri::command]
pub async fn clear_tunnel_metrics(
    tunnel_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(metrics) = state.tunnel_manager.get_metrics(&tunnel_id).await {
        metrics.clear().await;
        Ok(())
    } else {
        Err("Tunnel not found".to_string())
    }
}

/// Stop all tunnels
#[tauri::command]
pub async fn stop_all_tunnels(state: State<'_, AppState>) -> Result<(), String> {
    state.tunnel_manager.stop_all_tunnels().await;
    Ok(())
}

/// Health check
#[tauri::command]
pub async fn health_check() -> Result<String, String> {
    Ok("OK".to_string())
}

// ========== Database Commands ==========

use crate::db::models::{Protocol, Relay, Tunnel};

/// Create a new relay
#[tauri::command]
pub async fn db_create_relay(
    name: String,
    address: String,
    region: String,
    description: Option<String>,
    state: State<'_, AppState>,
) -> Result<Relay, String> {
    state
        .db
        .create_relay(name, address, region, description)
        .await
        .map_err(|e| e.to_string())
}

/// Get a relay by ID
#[tauri::command]
pub async fn db_get_relay(id: String, state: State<'_, AppState>) -> Result<Option<Relay>, String> {
    state.db.get_relay(id).await.map_err(|e| e.to_string())
}

/// List all relays
#[tauri::command]
pub async fn db_list_relays(state: State<'_, AppState>) -> Result<Vec<Relay>, String> {
    state.db.list_relays().await.map_err(|e| e.to_string())
}

/// Update a relay
#[tauri::command]
pub async fn db_update_relay(
    id: String,
    name: Option<String>,
    address: Option<String>,
    region: Option<String>,
    description: Option<String>,
    status: Option<String>,
    state: State<'_, AppState>,
) -> Result<Relay, String> {
    state
        .db
        .update_relay(id, name, address, region, description, status)
        .await
        .map_err(|e| e.to_string())
}

/// Delete a relay
#[tauri::command]
pub async fn db_delete_relay(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.db.delete_relay(id).await.map_err(|e| e.to_string())
}

/// Create a new tunnel
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn db_create_tunnel(
    name: String,
    local_host: String,
    auth_token: String,
    exit_node_config: String,
    description: Option<String>,
    failover: bool,
    connection_timeout: i32,
    state: State<'_, AppState>,
) -> Result<Tunnel, String> {
    state
        .db
        .create_tunnel(
            name,
            local_host,
            auth_token,
            exit_node_config,
            description,
            failover,
            connection_timeout,
        )
        .await
        .map_err(|e| e.to_string())
}

/// Get a tunnel by ID
#[tauri::command]
pub async fn db_get_tunnel(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<Tunnel>, String> {
    state.db.get_tunnel(id).await.map_err(|e| e.to_string())
}

/// List all tunnels
#[tauri::command]
pub async fn db_list_tunnels(state: State<'_, AppState>) -> Result<Vec<Tunnel>, String> {
    state.db.list_tunnels().await.map_err(|e| e.to_string())
}

/// Update tunnel status
#[tauri::command]
pub async fn db_update_tunnel_status(
    id: String,
    status: String,
    last_connected_at: Option<String>,
    state: State<'_, AppState>,
) -> Result<Tunnel, String> {
    let last_connected = last_connected_at
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    state
        .db
        .update_tunnel_status(id, status, last_connected)
        .await
        .map_err(|e| e.to_string())
}

/// Delete a tunnel
#[tauri::command]
pub async fn db_delete_tunnel(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.db.delete_tunnel(id).await.map_err(|e| e.to_string())
}

/// Create a protocol for a tunnel
#[tauri::command]
pub async fn db_create_protocol(
    tunnel_id: String,
    protocol_type: String,
    local_port: i32,
    remote_port: Option<i32>,
    subdomain: Option<String>,
    custom_domain: Option<String>,
    state: State<'_, AppState>,
) -> Result<Protocol, String> {
    state
        .db
        .create_protocol(
            tunnel_id,
            protocol_type,
            local_port,
            remote_port,
            subdomain,
            custom_domain,
        )
        .await
        .map_err(|e| e.to_string())
}

/// List protocols for a tunnel
#[tauri::command]
pub async fn db_list_protocols_for_tunnel(
    tunnel_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Protocol>, String> {
    state
        .db
        .list_protocols_for_tunnel(tunnel_id)
        .await
        .map_err(|e| e.to_string())
}

/// Delete a protocol
#[tauri::command]
pub async fn db_delete_protocol(id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .db
        .delete_protocol(id)
        .await
        .map_err(|e| e.to_string())
}

/// Verify relay connectivity
#[tauri::command]
pub async fn verify_relay(address: String) -> Result<bool, String> {
    use tokio::net::TcpStream;
    use tokio::time::{timeout, Duration};

    // First try parsing as direct SocketAddr (e.g., "127.0.0.1:4443")
    let socket_addr = if let Ok(addr) = address.parse::<std::net::SocketAddr>() {
        addr
    } else {
        // Not a direct IP:port, try resolving as hostname:port
        let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host(&address)
            .await
            .map_err(|e| {
                format!(
                    "Failed to resolve hostname '{}': {}. Expected format: 'host:port'",
                    address, e
                )
            })?
            .collect();

        // Use the first resolved address (prefer IPv4)
        addrs
            .iter()
            .find(|addr| addr.is_ipv4())
            .or_else(|| addrs.first())
            .copied()
            .ok_or_else(|| format!("No addresses found for hostname '{}'", address))?
    };

    // Try to connect with 5 second timeout
    match timeout(Duration::from_secs(5), TcpStream::connect(socket_addr)).await {
        Ok(Ok(_stream)) => Ok(true),
        Ok(Err(e)) => Err(format!("Connection failed: {}", e)),
        Err(_) => Err("Connection timeout (5 seconds)".to_string()),
    }
}
