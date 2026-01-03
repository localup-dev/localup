//! Settings management commands

use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde::{Deserialize, Serialize};
use tauri::State;
use tauri_plugin_autostart::ManagerExt;

use crate::db::entities::{setting, Setting};
use crate::state::AppState;

/// Setting keys used in the application
pub mod keys {
    pub const AUTOSTART: &str = "autostart";
    pub const START_MINIMIZED: &str = "start_minimized";
    pub const AUTO_CONNECT_TUNNELS: &str = "auto_connect_tunnels";
    pub const CAPTURE_TRAFFIC: &str = "capture_traffic";
    pub const CLEAR_ON_CLOSE: &str = "clear_on_close";
}

/// All application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Start on login
    pub autostart: bool,
    /// Start minimized to tray
    pub start_minimized: bool,
    /// Auto-connect tunnels marked as auto-start
    pub auto_connect_tunnels: bool,
    /// Capture traffic for inspection
    pub capture_traffic: bool,
    /// Clear traffic data on close
    pub clear_on_close: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            autostart: false,
            start_minimized: false,
            auto_connect_tunnels: true,
            capture_traffic: true,
            clear_on_close: false,
        }
    }
}

/// Get all application settings
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let settings = Setting::find()
        .all(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to load settings: {}", e))?;

    let mut app_settings = AppSettings::default();

    for setting in settings {
        match setting.key.as_str() {
            keys::AUTOSTART => {
                app_settings.autostart = parse_bool(&setting.value);
            }
            keys::START_MINIMIZED => {
                app_settings.start_minimized = parse_bool(&setting.value);
            }
            keys::AUTO_CONNECT_TUNNELS => {
                app_settings.auto_connect_tunnels = parse_bool(&setting.value);
            }
            keys::CAPTURE_TRAFFIC => {
                app_settings.capture_traffic = parse_bool(&setting.value);
            }
            keys::CLEAR_ON_CLOSE => {
                app_settings.clear_on_close = parse_bool(&setting.value);
            }
            _ => {}
        }
    }

    Ok(app_settings)
}

/// Update a single setting
#[tauri::command]
pub async fn update_setting(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: bool,
) -> Result<(), String> {
    // Handle autostart specially - it needs to use the plugin
    if key == keys::AUTOSTART {
        let autostart_manager = app.autolaunch();
        if value {
            autostart_manager
                .enable()
                .map_err(|e| format!("Failed to enable autostart: {}", e))?;
        } else {
            autostart_manager
                .disable()
                .map_err(|e| format!("Failed to disable autostart: {}", e))?;
        }
    }

    // Save to database
    save_setting(&state, &key, value).await?;

    Ok(())
}

/// Get the current autostart status from the system
#[tauri::command]
pub async fn get_autostart_status(app: tauri::AppHandle) -> Result<bool, String> {
    let autostart_manager = app.autolaunch();
    autostart_manager
        .is_enabled()
        .map_err(|e| format!("Failed to get autostart status: {}", e))
}

/// Save a boolean setting to the database
async fn save_setting(state: &State<'_, AppState>, key: &str, value: bool) -> Result<(), String> {
    let value_str = if value { "true" } else { "false" };

    // Check if setting exists
    let existing = Setting::find_by_id(key)
        .one(state.db.as_ref())
        .await
        .map_err(|e| format!("Failed to check setting: {}", e))?;

    if let Some(model) = existing {
        // Update existing
        let mut active: setting::ActiveModel = model.into();
        active.value = Set(value_str.to_string());
        active
            .update(state.db.as_ref())
            .await
            .map_err(|e| format!("Failed to update setting: {}", e))?;
    } else {
        // Insert new
        let setting = setting::ActiveModel {
            key: Set(key.to_string()),
            value: Set(value_str.to_string()),
        };
        setting
            .insert(state.db.as_ref())
            .await
            .map_err(|e| format!("Failed to save setting: {}", e))?;
    }

    Ok(())
}

/// Parse a boolean from a string value
fn parse_bool(value: &str) -> bool {
    matches!(value.to_lowercase().as_str(), "true" | "1" | "yes")
}
