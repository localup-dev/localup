// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod db;
mod tunnel_manager;

use commands::AppState;
use db::DatabaseService;
use std::sync::Arc;
use tauri::Manager;
use tunnel_manager::TunnelManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Get app data directory
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");

            // Create directory if it doesn't exist
            std::fs::create_dir_all(&app_dir).expect("Failed to create app data directory");

            // Database path
            let db_path = app_dir.join("tunnels.db");

            // Create database service
            let db_service = Arc::new(DatabaseService::new());

            // Initialize database synchronously before app starts
            let db_service_clone = db_service.clone();
            tauri::async_runtime::block_on(async move {
                if let Err(e) = db_service_clone.init(db_path).await {
                    tracing::error!("Failed to initialize database: {:?}", e);
                    panic!("Database initialization failed: {:?}", e);
                }
            });

            // Create tunnel manager
            let tunnel_manager = Arc::new(TunnelManager::new());

            // Create app state
            let app_state = AppState {
                tunnel_manager,
                db: db_service,
            };

            app.manage(app_state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::create_tunnel,
            commands::stop_tunnel,
            commands::list_tunnels,
            commands::get_tunnel,
            commands::get_tunnel_metrics,
            commands::get_tunnel_requests,
            commands::get_tunnel_tcp_connections,
            commands::clear_tunnel_metrics,
            commands::stop_all_tunnels,
            // Database commands
            commands::db_create_relay,
            commands::db_get_relay,
            commands::db_list_relays,
            commands::db_update_relay,
            commands::db_delete_relay,
            commands::db_create_tunnel,
            commands::db_get_tunnel,
            commands::db_list_tunnels,
            commands::db_update_tunnel_status,
            commands::db_delete_tunnel,
            commands::db_create_protocol,
            commands::db_list_protocols_for_tunnel,
            commands::db_delete_protocol,
            commands::verify_relay,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
