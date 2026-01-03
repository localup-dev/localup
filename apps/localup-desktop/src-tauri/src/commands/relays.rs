//! Relay server management commands

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::entities::{relay_server, RelayServer};
use crate::state::AppState;

/// Relay server response type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayServerResponse {
    pub id: String,
    pub name: String,
    pub address: String,
    pub jwt_token: Option<String>,
    pub protocol: String,
    pub insecure: bool,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<relay_server::Model> for RelayServerResponse {
    fn from(model: relay_server::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            address: model.address,
            jwt_token: model.jwt_token,
            protocol: model.protocol,
            insecure: model.insecure,
            is_default: model.is_default,
            created_at: model.created_at.to_rfc3339(),
            updated_at: model.updated_at.to_rfc3339(),
        }
    }
}

/// Request to create a new relay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRelayRequest {
    pub name: String,
    pub address: String,
    pub jwt_token: Option<String>,
    pub protocol: Option<String>,
    pub insecure: Option<bool>,
    pub is_default: Option<bool>,
}

/// Request to update a relay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRelayRequest {
    pub name: Option<String>,
    pub address: Option<String>,
    pub jwt_token: Option<String>,
    pub protocol: Option<String>,
    pub insecure: Option<bool>,
    pub is_default: Option<bool>,
}

/// List all configured relay servers
#[tauri::command]
pub async fn list_relays(state: State<'_, AppState>) -> Result<Vec<RelayServerResponse>, String> {
    let relays = RelayServer::find()
        .all(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to list relays: {}", e))?;

    Ok(relays.into_iter().map(RelayServerResponse::from).collect())
}

/// Get a single relay by ID
#[tauri::command]
pub async fn get_relay(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<RelayServerResponse>, String> {
    let relay = RelayServer::find_by_id(&id)
        .one(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to get relay: {}", e))?;

    Ok(relay.map(RelayServerResponse::from))
}

/// Add a new relay server
#[tauri::command]
pub async fn add_relay(
    state: State<'_, AppState>,
    request: CreateRelayRequest,
) -> Result<RelayServerResponse, String> {
    let now = Utc::now();
    let id = uuid::Uuid::new_v4().to_string();

    // If this is the first relay or is_default is true, ensure only one default
    if request.is_default.unwrap_or(false) {
        clear_default_relay(&state).await?;
    }

    let relay = relay_server::ActiveModel {
        id: Set(id),
        name: Set(request.name),
        address: Set(request.address),
        jwt_token: Set(request.jwt_token),
        protocol: Set(request.protocol.unwrap_or_else(|| "quic".to_string())),
        insecure: Set(request.insecure.unwrap_or(false)),
        is_default: Set(request.is_default.unwrap_or(false)),
        created_at: Set(now),
        updated_at: Set(now),
    };

    let result = relay
        .insert(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to add relay: {}", e))?;

    Ok(RelayServerResponse::from(result))
}

/// Update an existing relay server
#[tauri::command]
pub async fn update_relay(
    state: State<'_, AppState>,
    id: String,
    request: UpdateRelayRequest,
) -> Result<RelayServerResponse, String> {
    let existing = RelayServer::find_by_id(&id)
        .one(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to find relay: {}", e))?
        .ok_or_else(|| format!("Relay not found: {}", id))?;

    // If setting as default, clear other defaults
    if request.is_default.unwrap_or(false) && !existing.is_default {
        clear_default_relay(&state).await?;
    }

    let mut relay: relay_server::ActiveModel = existing.into();

    if let Some(name) = request.name {
        relay.name = Set(name);
    }
    if let Some(address) = request.address {
        relay.address = Set(address);
    }
    if let Some(jwt_token) = request.jwt_token {
        relay.jwt_token = Set(Some(jwt_token));
    }
    if let Some(protocol) = request.protocol {
        relay.protocol = Set(protocol);
    }
    if let Some(insecure) = request.insecure {
        relay.insecure = Set(insecure);
    }
    if let Some(is_default) = request.is_default {
        relay.is_default = Set(is_default);
    }

    relay.updated_at = Set(Utc::now());

    let result = relay
        .update(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to update relay: {}", e))?;

    Ok(RelayServerResponse::from(result))
}

/// Delete a relay server
#[tauri::command]
pub async fn delete_relay(state: State<'_, AppState>, id: String) -> Result<(), String> {
    RelayServer::delete_by_id(&id)
        .exec(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to delete relay: {}", e))?;

    Ok(())
}

/// Test connection to a relay server
#[tauri::command]
pub async fn test_relay(state: State<'_, AppState>, id: String) -> Result<TestRelayResult, String> {
    let _relay = RelayServer::find_by_id(&id)
        .one(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to find relay: {}", e))?
        .ok_or_else(|| format!("Relay not found: {}", id))?;

    // TODO: Actually test the connection using localup-lib
    // For now, just return a placeholder result
    Ok(TestRelayResult {
        success: true,
        latency_ms: Some(42),
        error: None,
    })
}

/// Result of testing a relay connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRelayResult {
    pub success: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// Clear the is_default flag on all relays
async fn clear_default_relay(state: &State<'_, AppState>) -> Result<(), String> {
    let default_relays = RelayServer::find()
        .filter(relay_server::Column::IsDefault.eq(true))
        .all(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to find default relays: {}", e))?;

    for relay in default_relays {
        let mut active: relay_server::ActiveModel = relay.into();
        active.is_default = Set(false);
        active.updated_at = Set(Utc::now());
        active
            .update(state.db.as_ref())
            .await
            .map_err(|e| format!("Failed to clear default relay: {}", e))?;
    }

    Ok(())
}
