//! LocalUp Desktop - Tauri application for tunnel management

use tauri::Manager;

mod commands;
pub mod daemon;
mod db;
mod state;
mod tray;

use daemon::DaemonClient;
use state::AppState;

#[cfg(target_os = "macos")]
use tauri::ActivationPolicy;

/// Get application version
#[tauri::command]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("localup_desktop=debug".parse().unwrap()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            // Get app data directory for database
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");

            // Create directory if it doesn't exist
            std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data directory");

            // Initialize database
            let database_url = db::get_database_url(&app_data_dir);
            tracing::info!("Database URL: {}", database_url);

            // Run database setup in a blocking context
            let db = tauri::async_runtime::block_on(async {
                let db = db::connect(&database_url)
                    .await
                    .expect("Failed to connect to database");
                db::migrate(&db).await.expect("Failed to run migrations");
                db
            });

            // Create app state and manage it
            let app_state = AppState::new(db);

            // Set app handle for metrics event emission
            let app_handle_for_state = app.handle().clone();
            tauri::async_runtime::block_on(async {
                app_state.set_app_handle(app_handle_for_state).await;
            });

            app.manage(app_state.clone());

            // Setup system tray
            let app_handle = app.handle().clone();
            if let Err(e) = tray::setup_tray(&app_handle) {
                tracing::error!("Failed to setup system tray: {}", e);
            }

            // Clone app handle for the spawn block
            let app_handle_for_tunnels = app.handle().clone();

            // Start the daemon automatically (ensures tunnels can run independently)
            tauri::async_runtime::spawn(async move {
                tracing::info!("Checking daemon status...");
                match DaemonClient::connect_or_start().await {
                    Ok(mut client) => match client.ping().await {
                        Ok((version, uptime, count)) => {
                            tracing::info!(
                                "Daemon running: v{}, uptime {}s, {} tunnels",
                                version,
                                uptime,
                                count
                            );
                            // Sync tunnel states from daemon to local state
                            if count > 0 {
                                tracing::info!("Syncing {} tunnel(s) from daemon...", count);
                                if let Ok(tunnels) = client.list_tunnels().await {
                                    let mut manager = app_state.tunnel_manager.write().await;
                                    for tunnel in tunnels {
                                        let status = match tunnel.status.as_str() {
                                            "connected" => state::tunnel_manager::TunnelStatus::Connected,
                                            "connecting" => state::tunnel_manager::TunnelStatus::Connecting,
                                            "error" => state::tunnel_manager::TunnelStatus::Error,
                                            _ => state::tunnel_manager::TunnelStatus::Disconnected,
                                        };
                                        manager.update_status(
                                            &tunnel.id,
                                            status,
                                            tunnel.public_url.clone(),
                                            tunnel.localup_id.clone(),
                                            tunnel.error_message.clone(),
                                        );
                                        tracing::info!(
                                            "Synced tunnel {} ({}) - status: {}",
                                            tunnel.id,
                                            tunnel.name,
                                            tunnel.status
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Daemon ping failed: {}", e);
                        }
                    },
                    Err(e) => {
                        tracing::error!("Failed to start daemon: {}", e);
                        // Fall back to in-process tunnel management
                        tracing::info!("Falling back to in-process tunnel management");
                        app_state.start_auto_start_tunnels().await;
                    }
                }
                // Update tray after tunnels start
                tray::update_tray_menu(&app_handle_for_tunnels).await;
            });

            // Hide window on close (minimize to tray) instead of quitting
            let window = app.get_webview_window("main").unwrap();
            let window_clone = window.clone();
            #[cfg(target_os = "macos")]
            let app_handle_for_policy = app.handle().clone();

            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    // Prevent the window from closing, hide it instead
                    api.prevent_close();
                    let _ = window_clone.hide();

                    // On macOS, hide from dock when window is hidden
                    #[cfg(target_os = "macos")]
                    {
                        let _ = app_handle_for_policy
                            .set_activation_policy(ActivationPolicy::Accessory);
                    }
                }
            });

            tracing::info!("LocalUp Desktop started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_version,
            // Tunnel commands
            commands::list_tunnels,
            commands::get_tunnel,
            commands::create_tunnel,
            commands::update_tunnel,
            commands::delete_tunnel,
            commands::start_tunnel,
            commands::stop_tunnel,
            commands::get_tunnel_metrics,
            commands::clear_tunnel_metrics,
            commands::get_captured_requests,
            commands::replay_request,
            commands::subscribe_daemon_metrics,
            // Relay commands
            commands::list_relays,
            commands::get_relay,
            commands::add_relay,
            commands::update_relay,
            commands::delete_relay,
            commands::test_relay,
            // Settings commands
            commands::get_settings,
            commands::update_setting,
            commands::get_autostart_status,
            // Daemon commands
            commands::get_daemon_status,
            commands::start_daemon,
            commands::stop_daemon,
            commands::daemon_list_tunnels,
            commands::daemon_get_tunnel,
            commands::daemon_start_tunnel,
            commands::daemon_stop_tunnel,
            commands::daemon_delete_tunnel,
            commands::get_daemon_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
