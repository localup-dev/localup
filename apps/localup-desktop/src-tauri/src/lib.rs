//! LocalUp Desktop - Tauri application for tunnel management

use tauri::Manager;

mod commands;
mod db;
mod state;

use state::AppState;

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
            app.manage(app_state);

            // Hide window on close (minimize to tray) instead of quitting
            let window = app.get_webview_window("main").unwrap();
            let window_clone = window.clone();

            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    // Prevent the window from closing, hide it instead
                    api.prevent_close();
                    let _ = window_clone.hide();
                }
            });

            tracing::info!("LocalUp Desktop started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_version,
            commands::list_tunnels,
            commands::list_relays,
            commands::get_relay,
            commands::add_relay,
            commands::update_relay,
            commands::delete_relay,
            commands::test_relay,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
