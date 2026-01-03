//! Tunnel management commands
//!
//! Handles tunnel CRUD operations and start/stop functionality.
//! Tunnels run directly in the Tauri app process (no external daemon needed).
//! This approach is cross-platform (Windows/macOS/Linux).

use chrono::Utc;
use localup_lib::{
    ExitNodeConfig, ProtocolConfig, TunnelClient, TunnelConfig as ClientTunnelConfig,
};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tokio::sync::oneshot;
use tracing::{error, info};

use crate::db::entities::{tunnel_config, RelayServer, TunnelConfig};
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
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
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

    let manager = state.tunnel_manager.read().await;

    let result = configs
        .into_iter()
        .map(|config| {
            let running = manager.get(&config.id);
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
                status: running
                    .map(|t| t.status.as_str().to_string())
                    .unwrap_or_else(|| "disconnected".to_string()),
                public_url: running.and_then(|t| t.public_url.clone()),
                error_message: running.and_then(|t| t.error_message.clone()),
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

    let manager = state.tunnel_manager.read().await;
    let running = manager.get(&config.id);

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
        status: running
            .map(|t| t.status.as_str().to_string())
            .unwrap_or_else(|| "disconnected".to_string()),
        public_url: running.and_then(|t| t.public_url.clone()),
        error_message: running.and_then(|t| t.error_message.clone()),
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

/// Start a tunnel
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

    // Update status to connecting
    {
        let mut manager = state.tunnel_manager.write().await;
        manager.update_status(&id, TunnelStatus::Connecting, None, None, None);
    }

    // Build client config
    let protocol_config = build_protocol_config(&config)?;

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
    let config_id = id.clone();

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        run_tunnel(
            config_id.clone(),
            client_config,
            tunnel_manager,
            shutdown_rx,
        )
        .await;
    });

    // Store handle for later shutdown
    {
        let mut handles = tunnel_handles.write().await;
        handles.insert(id.clone(), (handle, shutdown_tx));
    }

    // Return current state (will be updated by the spawned task)
    get_tunnel(state, id)
        .await?
        .ok_or_else(|| "Tunnel not found".to_string())
}

/// Stop a tunnel
#[tauri::command]
pub async fn stop_tunnel(state: State<'_, AppState>, id: String) -> Result<TunnelResponse, String> {
    stop_tunnel_internal(&state, &id).await;

    get_tunnel(state, id)
        .await?
        .ok_or_else(|| "Tunnel not found".to_string())
}

/// Internal function to stop a tunnel
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
}

/// Run a tunnel with reconnection logic
async fn run_tunnel(
    config_id: String,
    config: ClientTunnelConfig,
    tunnel_manager: Arc<tokio::sync::RwLock<crate::state::TunnelManager>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut reconnect_attempt = 0u32;

    loop {
        // Calculate backoff delay
        let backoff_seconds = if reconnect_attempt == 0 {
            0
        } else {
            std::cmp::min(2u64.pow(reconnect_attempt - 1), 30)
        };

        if backoff_seconds > 0 {
            info!(
                "[{}] Waiting {} seconds before reconnecting...",
                config_id, backoff_seconds
            );

            tokio::time::sleep(tokio::time::Duration::from_secs(backoff_seconds)).await;
        }

        // Check for shutdown
        if shutdown_rx.try_recv().is_ok() {
            info!("[{}] Tunnel stopped by request", config_id);
            let mut manager = tunnel_manager.write().await;
            manager.update_status(&config_id, TunnelStatus::Disconnected, None, None, None);
            break;
        }

        info!(
            "[{}] Connecting... (attempt {})",
            config_id,
            reconnect_attempt + 1
        );

        match TunnelClient::connect(config.clone()).await {
            Ok(client) => {
                reconnect_attempt = 0;

                info!("[{}] Connected successfully!", config_id);

                let public_url = client.public_url().map(|s| s.to_string());

                if let Some(url) = &public_url {
                    info!("[{}] Public URL: {}", config_id, url);
                }

                // Update status to connected
                {
                    let mut manager = tunnel_manager.write().await;
                    manager.update_status(
                        &config_id,
                        TunnelStatus::Connected,
                        public_url.clone(),
                        None,
                        None,
                    );
                }

                // Wait for tunnel to close or shutdown signal
                tokio::select! {
                    result = client.wait() => {
                        match result {
                            Ok(_) => {
                                info!("[{}] Tunnel closed gracefully", config_id);
                            }
                            Err(e) => {
                                error!("[{}] Tunnel error: {}", config_id, e);
                            }
                        }
                    }
                    _ = &mut shutdown_rx => {
                        info!("[{}] Shutdown requested", config_id);
                        let mut manager = tunnel_manager.write().await;
                        manager.update_status(&config_id, TunnelStatus::Disconnected, None, None, None);
                        break;
                    }
                }

                info!(
                    "[{}] Connection lost, attempting to reconnect...",
                    config_id
                );
            }
            Err(e) => {
                error!("[{}] Failed to connect: {}", config_id, e);

                // Update status to error
                {
                    let mut manager = tunnel_manager.write().await;
                    manager.update_status(
                        &config_id,
                        TunnelStatus::Error,
                        None,
                        None,
                        Some(e.to_string()),
                    );
                }

                // Check if non-recoverable
                if e.is_non_recoverable() {
                    error!("[{}] Non-recoverable error, stopping tunnel", config_id);
                    break;
                }

                reconnect_attempt += 1;

                // Check for shutdown
                if shutdown_rx.try_recv().is_ok() {
                    info!("[{}] Tunnel stopped by request", config_id);
                    let mut manager = tunnel_manager.write().await;
                    manager.update_status(&config_id, TunnelStatus::Disconnected, None, None, None);
                    break;
                }
            }
        }
    }
}

/// Build protocol config from database model
fn build_protocol_config(config: &tunnel_config::Model) -> Result<ProtocolConfig, String> {
    let local_port = config.local_port as u16;

    match config.protocol.as_str() {
        "http" => Ok(ProtocolConfig::Http {
            local_port,
            subdomain: config.subdomain.clone(),
            custom_domain: config.custom_domain.clone(),
        }),
        "https" => Ok(ProtocolConfig::Https {
            local_port,
            subdomain: config.subdomain.clone(),
            custom_domain: config.custom_domain.clone(),
        }),
        "tcp" => Ok(ProtocolConfig::Tcp {
            local_port,
            remote_port: None,
        }),
        "tls" => Ok(ProtocolConfig::Tls {
            local_port,
            sni_hostname: config.custom_domain.clone(),
        }),
        other => Err(format!("Unknown protocol: {}", other)),
    }
}
