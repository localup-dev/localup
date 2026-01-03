//! Global application state

use sea_orm::DatabaseConnection;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::TunnelManager;

/// Global application state shared across all Tauri commands
#[derive(Clone)]
pub struct AppState {
    /// Database connection
    pub db: Arc<DatabaseConnection>,

    /// Tunnel manager for running tunnels
    pub tunnel_manager: Arc<RwLock<TunnelManager>>,
}

impl AppState {
    /// Create new application state
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db: Arc::new(db),
            tunnel_manager: Arc::new(RwLock::new(TunnelManager::new())),
        }
    }
}
