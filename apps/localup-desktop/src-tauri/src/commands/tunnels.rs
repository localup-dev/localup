//! Tunnel management commands
//!
//! Handles tunnel CRUD operations and start/stop functionality.
//! Tunnels prefer to run via the daemon for persistence, but fall back to
//! in-process management if daemon is not available.

use chrono::Utc;
use localup_lib::{ExitNodeConfig, HttpMetric, TunnelConfig as ClientTunnelConfig};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::oneshot;
use tracing::{info, warn};

use crate::daemon::DaemonClient;
use crate::db::entities::{tunnel_config, RelayServer, TunnelConfig};
use crate::state::app_state::run_tunnel;
use crate::state::tunnel_manager::TunnelStatus;
use crate::state::AppState;

/// Tunnel response with current status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelResponse {
    pub id: String,
    pub name: String,
    pub relay_id: String,
    pub relay_name: Option<String>,
    pub local_host: String,
    pub local_port: u16,
    pub protocol: String,
    pub subdomain: Option<String>,
    pub custom_domain: Option<String>,
    pub auto_start: bool,
    pub enabled: bool,
    pub status: String,
    pub public_url: Option<String>,
    pub localup_id: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Captured request response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedRequestResponse {
    pub id: String,
    pub tunnel_session_id: String,
    pub localup_id: String,
    pub method: String,
    pub path: String,
    pub host: Option<String>,
    pub headers: String,
    pub body: Option<String>,
    pub status: Option<i32>,
    pub response_headers: Option<String>,
    pub response_body: Option<String>,
    pub created_at: String,
    pub latency_ms: Option<i32>,
}

/// Request to create a new tunnel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTunnelRequest {
    pub name: String,
    pub relay_id: String,
    pub local_host: Option<String>,
    pub local_port: u16,
    pub protocol: String,
    pub subdomain: Option<String>,
    pub custom_domain: Option<String>,
    pub auto_start: Option<bool>,
}

/// Request to update a tunnel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTunnelRequest {
    pub name: Option<String>,
    pub relay_id: Option<String>,
    pub local_host: Option<String>,
    pub local_port: Option<u16>,
    pub protocol: Option<String>,
    pub subdomain: Option<String>,
    pub custom_domain: Option<String>,
    pub auto_start: Option<bool>,
    pub enabled: Option<bool>,
}

/// List all tunnel configurations with their current status
#[tauri::command]
pub async fn list_tunnels(state: State<'_, AppState>) -> Result<Vec<TunnelResponse>, String> {
    let configs = TunnelConfig::find()
        .all(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to list tunnels: {}", e))?;

    // Get all relay servers for names
    let relays: std::collections::HashMap<String, String> = RelayServer::find()
        .all(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to list relays: {}", e))?
        .into_iter()
        .map(|r| (r.id, r.name))
        .collect();

    // Try to get tunnel statuses from daemon (with quick timeout)
    let daemon_tunnels: std::collections::HashMap<String, crate::daemon::protocol::TunnelInfo> =
        match tokio::time::timeout(std::time::Duration::from_millis(500), async {
            if let Ok(mut client) = DaemonClient::connect().await {
                if let Ok(tunnels) = client.list_tunnels().await {
                    return tunnels.into_iter().map(|t| (t.id.clone(), t)).collect();
                }
            }
            std::collections::HashMap::new()
        })
        .await
        {
            Ok(map) => map,
            Err(_) => std::collections::HashMap::new(),
        };

    let manager = state.tunnel_manager.read().await;

    let result = configs
        .into_iter()
        .map(|config| {
            // First check daemon status, then local manager
            let daemon_info = daemon_tunnels.get(&config.id);
            let local_running = manager.get(&config.id);

            // Prefer daemon status over local status
            let (status, public_url, localup_id, error_message) = if let Some(dt) = daemon_info {
                (
                    dt.status.clone(),
                    dt.public_url.clone(),
                    dt.localup_id.clone(),
                    dt.error_message.clone(),
                )
            } else if let Some(lt) = local_running {
                (
                    lt.status.as_str().to_string(),
                    lt.public_url.clone(),
                    lt.localup_id.clone(),
                    lt.error_message.clone(),
                )
            } else {
                ("disconnected".to_string(), None, None, None)
            };

            TunnelResponse {
                id: config.id.clone(),
                name: config.name,
                relay_id: config.relay_server_id.clone(),
                relay_name: relays.get(&config.relay_server_id).cloned(),
                local_host: config.local_host,
                local_port: config.local_port as u16,
                protocol: config.protocol,
                subdomain: config.subdomain,
                custom_domain: config.custom_domain,
                auto_start: config.auto_start,
                enabled: config.enabled,
                status,
                public_url,
                localup_id,
                error_message,
                created_at: config.created_at.to_rfc3339(),
                updated_at: config.updated_at.to_rfc3339(),
            }
        })
        .collect();

    Ok(result)
}

/// Get a single tunnel by ID
#[tauri::command]
pub async fn get_tunnel(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<TunnelResponse>, String> {
    let config = TunnelConfig::find_by_id(&id)
        .one(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to get tunnel: {}", e))?;

    let Some(config) = config else {
        return Ok(None);
    };

    // Get relay name
    let relay = RelayServer::find_by_id(&config.relay_server_id)
        .one(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to get relay: {}", e))?;

    // Try to get tunnel status from daemon (with quick timeout)
    let daemon_info: Option<crate::daemon::protocol::TunnelInfo> =
        tokio::time::timeout(std::time::Duration::from_millis(500), async {
            if let Ok(mut client) = DaemonClient::connect().await {
                if let Ok(info) = client.get_tunnel(&id).await {
                    return Some(info);
                }
            }
            None
        })
        .await
        .unwrap_or(None);

    let manager = state.tunnel_manager.read().await;
    let local_running = manager.get(&config.id);

    // Prefer daemon status over local status
    let (status, public_url, localup_id, error_message) = if let Some(dt) = daemon_info {
        (
            dt.status.clone(),
            dt.public_url.clone(),
            dt.localup_id.clone(),
            dt.error_message.clone(),
        )
    } else if let Some(lt) = local_running {
        (
            lt.status.as_str().to_string(),
            lt.public_url.clone(),
            lt.localup_id.clone(),
            lt.error_message.clone(),
        )
    } else {
        ("disconnected".to_string(), None, None, None)
    };

    Ok(Some(TunnelResponse {
        id: config.id.clone(),
        name: config.name,
        relay_id: config.relay_server_id,
        relay_name: relay.map(|r| r.name),
        local_host: config.local_host,
        local_port: config.local_port as u16,
        protocol: config.protocol,
        subdomain: config.subdomain,
        custom_domain: config.custom_domain,
        auto_start: config.auto_start,
        enabled: config.enabled,
        status,
        public_url,
        localup_id,
        error_message,
        created_at: config.created_at.to_rfc3339(),
        updated_at: config.updated_at.to_rfc3339(),
    }))
}

/// Create a new tunnel configuration
#[tauri::command]
pub async fn create_tunnel(
    state: State<'_, AppState>,
    request: CreateTunnelRequest,
) -> Result<TunnelResponse, String> {
    // Verify relay exists
    let relay = RelayServer::find_by_id(&request.relay_id)
        .one(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to find relay: {}", e))?
        .ok_or_else(|| format!("Relay not found: {}", request.relay_id))?;

    let now = Utc::now();
    let id = uuid::Uuid::new_v4().to_string();

    let tunnel = tunnel_config::ActiveModel {
        id: Set(id.clone()),
        name: Set(request.name.clone()),
        relay_server_id: Set(request.relay_id.clone()),
        local_host: Set(request
            .local_host
            .unwrap_or_else(|| "localhost".to_string())),
        local_port: Set(request.local_port as i32),
        protocol: Set(request.protocol.clone()),
        subdomain: Set(request.subdomain.clone()),
        custom_domain: Set(request.custom_domain.clone()),
        auto_start: Set(request.auto_start.unwrap_or(false)),
        enabled: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    };

    let result = tunnel
        .insert(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to create tunnel: {}", e))?;

    Ok(TunnelResponse {
        id: result.id,
        name: result.name,
        relay_id: result.relay_server_id,
        relay_name: Some(relay.name),
        local_host: result.local_host,
        local_port: result.local_port as u16,
        protocol: result.protocol,
        subdomain: result.subdomain,
        custom_domain: result.custom_domain,
        auto_start: result.auto_start,
        enabled: result.enabled,
        status: "disconnected".to_string(),
        public_url: None,
        localup_id: None,
        error_message: None,
        created_at: result.created_at.to_rfc3339(),
        updated_at: result.updated_at.to_rfc3339(),
    })
}

/// Update an existing tunnel configuration
#[tauri::command]
pub async fn update_tunnel(
    state: State<'_, AppState>,
    id: String,
    request: UpdateTunnelRequest,
) -> Result<TunnelResponse, String> {
    let existing = TunnelConfig::find_by_id(&id)
        .one(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to find tunnel: {}", e))?
        .ok_or_else(|| format!("Tunnel not found: {}", id))?;

    let mut tunnel: tunnel_config::ActiveModel = existing.into();

    if let Some(name) = request.name {
        tunnel.name = Set(name);
    }
    if let Some(relay_id) = request.relay_id {
        // Verify relay exists
        RelayServer::find_by_id(&relay_id)
            .one(state.db.as_ref())
            .await
            .map_err(|e| format!("Failed to find relay: {}", e))?
            .ok_or_else(|| format!("Relay not found: {}", relay_id))?;
        tunnel.relay_server_id = Set(relay_id);
    }
    if let Some(local_host) = request.local_host {
        tunnel.local_host = Set(local_host);
    }
    if let Some(local_port) = request.local_port {
        tunnel.local_port = Set(local_port as i32);
    }
    if let Some(protocol) = request.protocol {
        tunnel.protocol = Set(protocol);
    }
    if let Some(subdomain) = request.subdomain {
        tunnel.subdomain = Set(Some(subdomain));
    }
    if let Some(custom_domain) = request.custom_domain {
        tunnel.custom_domain = Set(Some(custom_domain));
    }
    if let Some(auto_start) = request.auto_start {
        tunnel.auto_start = Set(auto_start);
    }
    if let Some(enabled) = request.enabled {
        tunnel.enabled = Set(enabled);
    }

    tunnel.updated_at = Set(Utc::now());

    let result = tunnel
        .update(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to update tunnel: {}", e))?;

    // Get relay name
    let relay = RelayServer::find_by_id(&result.relay_server_id)
        .one(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to get relay: {}", e))?;

    let manager = state.tunnel_manager.read().await;
    let running = manager.get(&result.id);

    Ok(TunnelResponse {
        id: result.id.clone(),
        name: result.name,
        relay_id: result.relay_server_id,
        relay_name: relay.map(|r| r.name),
        local_host: result.local_host,
        local_port: result.local_port as u16,
        protocol: result.protocol,
        subdomain: result.subdomain,
        custom_domain: result.custom_domain,
        auto_start: result.auto_start,
        enabled: result.enabled,
        status: running
            .map(|t| t.status.as_str().to_string())
            .unwrap_or_else(|| "disconnected".to_string()),
        public_url: running.and_then(|t| t.public_url.clone()),
        localup_id: running.and_then(|t| t.localup_id.clone()),
        error_message: running.and_then(|t| t.error_message.clone()),
        created_at: result.created_at.to_rfc3339(),
        updated_at: result.updated_at.to_rfc3339(),
    })
}

/// Delete a tunnel configuration
#[tauri::command]
pub async fn delete_tunnel(state: State<'_, AppState>, id: String) -> Result<(), String> {
    // Stop tunnel if running
    stop_tunnel_internal(&state, &id).await;

    TunnelConfig::delete_by_id(&id)
        .exec(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to delete tunnel: {}", e))?;

    Ok(())
}

/// Start a tunnel - tries daemon first, falls back to in-process
#[tauri::command]
pub async fn start_tunnel(
    state: State<'_, AppState>,
    id: String,
) -> Result<TunnelResponse, String> {
    // Get tunnel config
    let config = TunnelConfig::find_by_id(&id)
        .one(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to find tunnel: {}", e))?
        .ok_or_else(|| format!("Tunnel not found: {}", id))?;

    // Get relay config
    let relay = RelayServer::find_by_id(&config.relay_server_id)
        .one(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to find relay: {}", e))?
        .ok_or_else(|| format!("Relay not found: {}", config.relay_server_id))?;

    // Check if already running
    {
        let manager = state.tunnel_manager.read().await;
        if let Some(running) = manager.get(&id) {
            if running.status == TunnelStatus::Connected
                || running.status == TunnelStatus::Connecting
            {
                return Err("Tunnel is already running".to_string());
            }
        }
    }

    // Try to start via daemon first (with quick timeout check)
    let use_daemon = match tokio::time::timeout(
        std::time::Duration::from_secs(2),
        DaemonClient::connect(),
    )
    .await
    {
        Ok(Ok(mut client)) => {
            // Verify daemon is responsive with a quick ping
            if client.ping().await.is_err() {
                warn!("Daemon not responsive, using in-process tunnel management");
                false
            } else {
                info!("Starting tunnel {} via daemon", id);
                match client
                    .start_tunnel(
                        &id,
                        &config.name,
                        &relay.address,
                        relay.jwt_token.as_deref().unwrap_or(""),
                        &config.local_host,
                        config.local_port as u16,
                        &config.protocol,
                        config.subdomain.as_deref(),
                        config.custom_domain.as_deref(),
                    )
                    .await
                {
                    Ok(tunnel_info) => {
                        // Update local state from daemon response
                        let mut manager = state.tunnel_manager.write().await;
                        let status = match tunnel_info.status.as_str() {
                            "connected" => TunnelStatus::Connected,
                            "connecting" => TunnelStatus::Connecting,
                            "error" => TunnelStatus::Error,
                            _ => TunnelStatus::Disconnected,
                        };
                        manager.update_status(
                            &id,
                            status,
                            tunnel_info.public_url.clone(),
                            tunnel_info.localup_id.clone(),
                            tunnel_info.error_message.clone(),
                        );
                        drop(manager);

                        return get_tunnel(state, id)
                            .await?
                            .ok_or_else(|| "Tunnel not found".to_string());
                    }
                    Err(e) => {
                        // Check if tunnel is already running in daemon
                        if e.to_string().contains("already running") {
                            info!("Tunnel {} already running in daemon, syncing state", id);
                            // Get current tunnel info from daemon
                            if let Ok(tunnel_info) = client.get_tunnel(&id).await {
                                let mut manager = state.tunnel_manager.write().await;
                                let status = match tunnel_info.status.as_str() {
                                    "connected" => TunnelStatus::Connected,
                                    "connecting" => TunnelStatus::Connecting,
                                    "error" => TunnelStatus::Error,
                                    _ => TunnelStatus::Disconnected,
                                };
                                manager.update_status(
                                    &id,
                                    status,
                                    tunnel_info.public_url.clone(),
                                    tunnel_info.localup_id.clone(),
                                    tunnel_info.error_message.clone(),
                                );
                                drop(manager);

                                return get_tunnel(state, id)
                                    .await?
                                    .ok_or_else(|| "Tunnel not found".to_string());
                            }
                        }
                        warn!("Daemon start failed, falling back to in-process: {}", e);
                        false
                    }
                }
            }
        }
        Ok(Err(e)) => {
            info!(
                "Daemon not available ({}), using in-process tunnel management",
                e
            );
            false
        }
        Err(_) => {
            warn!("Daemon connection timed out, using in-process tunnel management");
            false
        }
    };

    if use_daemon {
        return get_tunnel(state, id)
            .await?
            .ok_or_else(|| "Tunnel not found".to_string());
    }

    // Fall back to in-process tunnel management
    start_tunnel_in_process(&state, &id, &config, &relay).await?;

    get_tunnel(state, id)
        .await?
        .ok_or_else(|| "Tunnel not found".to_string())
}

/// Start a tunnel in-process (fallback when daemon is not available)
async fn start_tunnel_in_process(
    state: &State<'_, AppState>,
    id: &str,
    config: &tunnel_config::Model,
    relay: &crate::db::entities::relay_server::Model,
) -> Result<(), String> {
    // Update status to connecting
    {
        let mut manager = state.tunnel_manager.write().await;
        manager.update_status(id, TunnelStatus::Connecting, None, None, None);
    }

    // Build client config
    let protocol_config = build_protocol_config(config)?;

    let client_config = ClientTunnelConfig {
        local_host: config.local_host.clone(),
        protocols: vec![protocol_config],
        auth_token: relay.jwt_token.clone().unwrap_or_default(),
        exit_node: ExitNodeConfig::Custom(relay.address.clone()),
        ..Default::default()
    };

    // Spawn tunnel task
    let tunnel_manager = state.tunnel_manager.clone();
    let tunnel_handles = state.tunnel_handles.clone();
    let tunnel_metrics = state.tunnel_metrics.clone();
    let app_handle = state.app_handle.clone();
    let config_id = id.to_string();

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        run_tunnel(
            config_id.clone(),
            client_config,
            tunnel_manager,
            tunnel_metrics,
            app_handle,
            shutdown_rx,
        )
        .await;
    });

    // Store handle for later shutdown
    {
        let mut handles = tunnel_handles.write().await;
        handles.insert(id.to_string(), (handle, shutdown_tx));
    }

    Ok(())
}

/// Stop a tunnel - tries both daemon and in-process
#[tauri::command]
pub async fn stop_tunnel(state: State<'_, AppState>, id: String) -> Result<TunnelResponse, String> {
    // Try to stop via daemon (with quick timeout)
    if let Ok(Ok(mut client)) =
        tokio::time::timeout(std::time::Duration::from_secs(2), DaemonClient::connect()).await
    {
        info!("Stopping tunnel {} via daemon", id);
        match client.stop_tunnel(&id).await {
            Ok(()) => {
                info!("Tunnel {} stopped via daemon", id);
            }
            Err(e) => {
                // "Tunnel not found" is fine - might only exist in-process
                if !e.to_string().contains("not found") {
                    warn!("Daemon stop failed: {}", e);
                }
            }
        }
    }

    // Always also try to stop in-process (in case tunnel is running both places)
    stop_tunnel_internal(&state, &id).await;

    get_tunnel(state, id)
        .await?
        .ok_or_else(|| "Tunnel not found".to_string())
}

/// Internal function to stop a tunnel (in-process)
async fn stop_tunnel_internal(state: &State<'_, AppState>, id: &str) {
    // Send shutdown signal
    {
        let mut handles = state.tunnel_handles.write().await;
        if let Some((handle, shutdown_tx)) = handles.remove(id) {
            let _ = shutdown_tx.send(());
            // Give it a moment to shut down gracefully
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            handle.abort();
        }
    }

    // Update status
    {
        let mut manager = state.tunnel_manager.write().await;
        manager.update_status(id, TunnelStatus::Disconnected, None, None, None);
    }

    // Clean up metrics
    state.remove_tunnel_metrics(id).await;
}

// Use build_protocol_config from app_state
use crate::state::app_state::build_protocol_config;

/// Paginated metrics response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedMetricsResponse {
    pub items: Vec<HttpMetric>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

/// Get real-time metrics for a tunnel with pagination (from daemon or in-memory MetricsStore)
#[tauri::command]
pub async fn get_tunnel_metrics(
    state: State<'_, AppState>,
    tunnel_id: String,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<PaginatedMetricsResponse, String> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(50).min(100); // Default 50, max 100

    // Try to get metrics from daemon first (with timeout)
    let daemon_result = tokio::time::timeout(std::time::Duration::from_millis(500), async {
        if let Ok(mut client) = DaemonClient::connect().await {
            if let Ok((items, total)) = client
                .get_tunnel_metrics(&tunnel_id, Some(offset), Some(limit))
                .await
            {
                return Some((items, total));
            }
        }
        None
    })
    .await;

    // Use daemon metrics if available, otherwise fall back to in-process
    let (items, total) = match daemon_result {
        Ok(Some((items, total))) if !items.is_empty() || total > 0 => (items, total),
        _ => {
            // Fall back to in-process metrics store
            state
                .get_tunnel_metrics_paginated(&tunnel_id, offset, limit)
                .await
        }
    };

    Ok(PaginatedMetricsResponse {
        items,
        total,
        offset,
        limit,
    })
}

/// Clear metrics for a tunnel
#[tauri::command]
pub async fn clear_tunnel_metrics(
    state: State<'_, AppState>,
    tunnel_id: String,
) -> Result<(), String> {
    // Try to clear metrics in daemon (with timeout)
    let _ = tokio::time::timeout(std::time::Duration::from_millis(500), async {
        if let Ok(mut client) = DaemonClient::connect().await {
            let _ = client.clear_tunnel_metrics(&tunnel_id).await;
        }
    })
    .await;

    // Also clear in-process metrics
    state.clear_tunnel_metrics(&tunnel_id).await;
    Ok(())
}

/// Replay request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayRequestParams {
    pub method: String,
    pub uri: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

/// Replay response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub duration_ms: u64,
}

/// Replay a captured HTTP request to the local service
#[tauri::command]
pub async fn replay_request(
    state: State<'_, AppState>,
    tunnel_id: String,
    request: ReplayRequestParams,
) -> Result<ReplayResponse, String> {
    use std::time::Instant;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    // Get tunnel config to find local port
    let config = TunnelConfig::find_by_id(&tunnel_id)
        .one(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to find tunnel: {}", e))?
        .ok_or_else(|| format!("Tunnel not found: {}", tunnel_id))?;

    let local_addr = format!("{}:{}", config.local_host, config.local_port);
    let start_time = Instant::now();

    // Connect to local service
    let mut socket = TcpStream::connect(&local_addr)
        .await
        .map_err(|e| format!("Failed to connect to {}: {}", local_addr, e))?;

    // Build HTTP request
    let mut http_request = format!("{} {} HTTP/1.1\r\n", request.method, request.uri);

    // Add headers
    let mut has_host = false;
    let mut has_content_length = false;
    for (name, value) in &request.headers {
        if name.to_lowercase() == "host" {
            has_host = true;
        }
        if name.to_lowercase() == "content-length" {
            has_content_length = true;
        }
        http_request.push_str(&format!("{}: {}\r\n", name, value));
    }

    // Add Host header if missing
    if !has_host {
        http_request.push_str(&format!("Host: {}\r\n", local_addr));
    }

    // Add Content-Length if body present and not already set
    if let Some(ref body) = request.body {
        if !has_content_length {
            http_request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
    }

    http_request.push_str("\r\n");

    // Write request
    socket
        .write_all(http_request.as_bytes())
        .await
        .map_err(|e| format!("Failed to write request: {}", e))?;

    // Write body if present
    if let Some(ref body) = request.body {
        socket
            .write_all(body.as_bytes())
            .await
            .map_err(|e| format!("Failed to write body: {}", e))?;
    }

    // Read response
    let mut response_data = Vec::new();
    let mut buffer = [0u8; 8192];

    // Read with timeout
    let read_future = async {
        loop {
            match socket.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => response_data.extend_from_slice(&buffer[..n]),
                Err(e) => return Err(format!("Read error: {}", e)),
            }
            // If we have enough data and it looks complete, break
            if response_data.len() > 0 {
                let response_str = String::from_utf8_lossy(&response_data);
                // Check if we have a complete response (has \r\n\r\n and content-length matches or chunked encoding ended)
                if let Some(header_end) = response_str.find("\r\n\r\n") {
                    let headers_part = &response_str[..header_end];
                    if let Some(cl_line) = headers_part
                        .lines()
                        .find(|l| l.to_lowercase().starts_with("content-length:"))
                    {
                        if let Ok(content_length) = cl_line
                            .split(':')
                            .nth(1)
                            .unwrap_or("0")
                            .trim()
                            .parse::<usize>()
                        {
                            let body_start = header_end + 4;
                            if response_data.len() >= body_start + content_length {
                                break;
                            }
                        }
                    } else if headers_part
                        .to_lowercase()
                        .contains("transfer-encoding: chunked")
                    {
                        // For chunked, check if we have 0\r\n\r\n
                        if response_str.contains("\r\n0\r\n") {
                            break;
                        }
                    } else {
                        // No content-length, assume complete after small delay
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        break;
                    }
                }
            }
        }
        Ok(())
    };

    tokio::time::timeout(tokio::time::Duration::from_secs(30), read_future)
        .await
        .map_err(|_| "Request timed out".to_string())?
        .map_err(|e| e)?;

    let duration_ms = start_time.elapsed().as_millis() as u64;

    // Parse response
    let response_str = String::from_utf8_lossy(&response_data);
    let mut lines = response_str.lines();

    // Parse status line
    let status = if let Some(status_line) = lines.next() {
        let parts: Vec<&str> = status_line.split_whitespace().collect();
        if parts.len() >= 2 {
            parts[1].parse().unwrap_or(0)
        } else {
            0
        }
    } else {
        0
    };

    // Parse headers
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }

    // Extract body (everything after \r\n\r\n)
    let body = if let Some(pos) = response_str.find("\r\n\r\n") {
        let body_str = &response_str[pos + 4..];
        if !body_str.is_empty() {
            Some(body_str.to_string())
        } else {
            None
        }
    } else {
        None
    };

    Ok(ReplayResponse {
        status,
        headers,
        body,
        duration_ms,
    })
}

/// Get captured requests for a tunnel (from database - historical)
#[tauri::command]
pub async fn get_captured_requests(
    state: State<'_, AppState>,
    tunnel_id: String,
) -> Result<Vec<CapturedRequestResponse>, String> {
    use crate::db::entities::CapturedRequest;
    use sea_orm::{ColumnTrait, QueryFilter, QueryOrder, QuerySelect};

    // Get the tunnel's current localup_id from the manager
    let localup_id = {
        let manager = state.tunnel_manager.read().await;
        manager.get(&tunnel_id).and_then(|t| t.localup_id.clone())
    };

    // If tunnel is not connected, return empty list
    let Some(localup_id) = localup_id else {
        return Ok(vec![]);
    };

    // Query captured requests by localup_id
    let requests = CapturedRequest::find()
        .filter(crate::db::entities::captured_request::Column::LocalupId.eq(&localup_id))
        .order_by_desc(crate::db::entities::captured_request::Column::CreatedAt)
        .limit(100)
        .all(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to get captured requests: {}", e))?;

    Ok(requests
        .into_iter()
        .map(|r| CapturedRequestResponse {
            id: r.id,
            tunnel_session_id: r.tunnel_session_id,
            localup_id: r.localup_id,
            method: r.method,
            path: r.path,
            host: r.host,
            headers: r.headers,
            body: r.body,
            status: r.status,
            response_headers: r.response_headers,
            response_body: r.response_body,
            created_at: r.created_at.to_rfc3339(),
            latency_ms: r.latency_ms,
        })
        .collect())
}

/// Subscribe to real-time metrics for a tunnel via daemon
/// This starts a background task that emits `tunnel-metrics` events
/// When the subscription ends, it emits a `tunnel-metrics-subscription-ended` event
#[tauri::command]
pub async fn subscribe_daemon_metrics(
    app_handle: tauri::AppHandle,
    tunnel_id: String,
) -> Result<(), String> {
    use localup_lib::MetricsEvent;
    use tauri::Emitter;

    info!("Subscribing to daemon metrics for tunnel: {}", tunnel_id);

    // Connect to daemon
    let client = DaemonClient::connect()
        .await
        .map_err(|e| format!("Failed to connect to daemon: {}", e))?;

    // Subscribe to metrics
    let mut subscription = client
        .subscribe_metrics(&tunnel_id)
        .await
        .map_err(|e| format!("Failed to subscribe to metrics: {}", e))?;

    info!(
        "Subscription started for tunnel: {}",
        subscription.tunnel_id()
    );

    // Spawn a background task to receive and emit events
    let tunnel_id_clone = tunnel_id.clone();
    tokio::spawn(async move {
        info!(
            "[{}] Starting metrics event loop from daemon",
            tunnel_id_clone
        );
        while let Some(event) = subscription.recv().await {
            // Emit the event to the frontend
            match &event {
                MetricsEvent::Request { metric } => {
                    info!(
                        "[{}] Daemon -> Frontend: {} {} (pending)",
                        tunnel_id_clone, metric.method, metric.uri
                    );
                }
                MetricsEvent::Response { id, status, .. } => {
                    info!(
                        "[{}] Daemon -> Frontend: {} -> {}",
                        tunnel_id_clone, id, status
                    );
                }
                MetricsEvent::Error { id, error, .. } => {
                    info!(
                        "[{}] Daemon -> Frontend: {} (error: {})",
                        tunnel_id_clone, id, error
                    );
                }
                _ => {
                    // Other events (TCP connection, etc.)
                    info!("[{}] Daemon -> Frontend: other event", tunnel_id_clone);
                }
            }

            let payload = serde_json::json!({
                "tunnel_id": tunnel_id_clone,
                "event": event,
            });

            if let Err(e) = app_handle.emit("tunnel-metrics", payload) {
                warn!("[{}] Failed to emit metrics event: {}", tunnel_id_clone, e);
                break;
            }
        }
        info!("[{}] Metrics event loop ended", tunnel_id_clone);

        // Notify frontend that subscription ended so it can retry
        let end_payload = serde_json::json!({
            "tunnel_id": tunnel_id_clone,
        });
        let _ = app_handle.emit("tunnel-metrics-subscription-ended", end_payload);
    });

    Ok(())
}
