//! Tauri IPC commands for LocalUp Desktop

pub mod relays;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

// Re-export relay commands
pub use relays::*;

/// Tunnel configuration with status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelWithStatus {
    pub id: String,
    pub name: String,
    pub relay_id: String,
    pub local_port: u16,
    pub protocol: String,
    pub subdomain: Option<String>,
    pub status: String,
    pub public_url: Option<String>,
}

/// List all tunnel configurations with their current status
#[tauri::command]
pub async fn list_tunnels(state: State<'_, AppState>) -> Result<Vec<TunnelWithStatus>, String> {
    use crate::db::entities::TunnelConfig;
    use sea_orm::EntityTrait;

    let configs = TunnelConfig::find()
        .all(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to list tunnels: {}", e))?;

    let manager = state.tunnel_manager.read().await;

    let result = configs
        .into_iter()
        .map(|config| {
            let running = manager.get(&config.id);
            TunnelWithStatus {
                id: config.id.clone(),
                name: config.name,
                relay_id: config.relay_server_id,
                local_port: config.local_port as u16,
                protocol: config.protocol,
                subdomain: config.subdomain,
                status: running
                    .map(|t| t.status.as_str().to_string())
                    .unwrap_or_else(|| "disconnected".to_string()),
                public_url: running.and_then(|t| t.public_url.clone()),
            }
        })
        .collect();

    Ok(result)
}
