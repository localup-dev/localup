//! System tray implementation for LocalUp Desktop
//!
//! Provides a menu bar icon (macOS) / system tray (Windows/Linux) with:
//! - Status indicator (connected/disconnected)
//! - Quick tunnel start/stop
//! - Show/hide window
//! - Quit application

use sea_orm::EntityTrait;
use tauri::{
    image::Image,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Manager, Wry,
};

#[cfg(target_os = "macos")]
use tauri::ActivationPolicy;
use tracing::{error, info};

use crate::db::entities::{RelayServer, TunnelConfig};
use crate::state::AppState;

// Embedded tray icon (22x22 PNG for macOS menu bar)
// This is the "l" logo as a template icon
const TRAY_ICON: &[u8] = include_bytes!("../icons/tray-iconTemplate.png");

/// Create and setup the system tray
pub fn setup_tray(app: &AppHandle) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let menu = build_tray_menu(app)?;

    // Load the embedded tray icon
    let icon = load_tray_icon()?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true) // macOS: use as template (adapts to light/dark mode)
        .menu(&menu)
        .show_menu_on_left_click(true) // Show menu on left click (standard macOS behavior)
        .tooltip("LocalUp - No tunnels active")
        .on_menu_event(handle_menu_event)
        .build(app)?;

    Ok(tray)
}

/// Load the tray icon from embedded PNG data
fn load_tray_icon() -> Result<Image<'static>, Box<dyn std::error::Error>> {
    // Decode the PNG manually since Tauri's Image expects raw RGBA
    let decoder = png::Decoder::new(TRAY_ICON);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;

    // Convert grayscale+alpha to RGBA (our template icon format)
    let rgba = match info.color_type {
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity(buf.len() * 2);
            for chunk in buf.chunks(2) {
                let gray = chunk[0];
                let alpha = chunk[1];
                rgba.extend_from_slice(&[gray, gray, gray, alpha]);
            }
            rgba
        }
        png::ColorType::Rgba => buf,
        _ => return Err("Unsupported PNG color type for tray icon".into()),
    };

    // Flip image horizontally (the source icon is mirrored)
    let width = info.width as usize;
    let height = info.height as usize;
    let bytes_per_pixel = 4; // RGBA
    let mut flipped = vec![0u8; rgba.len()];

    for y in 0..height {
        for x in 0..width {
            let src_idx = (y * width + x) * bytes_per_pixel;
            let dst_idx = (y * width + (width - 1 - x)) * bytes_per_pixel;
            flipped[dst_idx..dst_idx + bytes_per_pixel]
                .copy_from_slice(&rgba[src_idx..src_idx + bytes_per_pixel]);
        }
    }

    Ok(Image::new_owned(flipped, info.width, info.height))
}

/// Build the tray menu
fn build_tray_menu(app: &AppHandle) -> Result<Menu<Wry>, Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show LocalUp", true, None::<&str>)?;
    let separator1 = PredefinedMenuItem::separator(app)?;

    // Tunnels submenu (will be populated dynamically)
    let no_tunnels = MenuItem::with_id(
        app,
        "no_tunnels",
        "No tunnels configured",
        false,
        None::<&str>,
    )?;
    let tunnels_submenu =
        Submenu::with_id_and_items(app, "tunnels", "Tunnels", true, &[&no_tunnels])?;

    let separator2 = PredefinedMenuItem::separator(app)?;
    let start_all = MenuItem::with_id(app, "start_all", "Start All Tunnels", true, None::<&str>)?;
    let stop_all = MenuItem::with_id(app, "stop_all", "Stop All Tunnels", true, None::<&str>)?;

    let separator3 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit LocalUp", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &separator1,
            &tunnels_submenu,
            &separator2,
            &start_all,
            &stop_all,
            &separator3,
            &quit,
        ],
    )?;

    Ok(menu)
}

/// Handle tray menu events
fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id.as_ref() {
        "show" => {
            if let Some(window) = app.get_webview_window("main") {
                // On macOS, restore dock icon when showing window
                #[cfg(target_os = "macos")]
                {
                    let _ = app.set_activation_policy(ActivationPolicy::Regular);
                }
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "start_all" => {
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                start_all_tunnels(&app_handle).await;
            });
        }
        "stop_all" => {
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                stop_all_tunnels(&app_handle).await;
            });
        }
        "quit" => {
            info!("Quit requested from tray");
            app.exit(0);
        }
        id if id.starts_with("tunnel_start_") => {
            let tunnel_id = id.strip_prefix("tunnel_start_").unwrap().to_string();
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                start_tunnel(&app_handle, &tunnel_id).await;
            });
        }
        id if id.starts_with("tunnel_stop_") => {
            let tunnel_id = id.strip_prefix("tunnel_stop_").unwrap().to_string();
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                stop_tunnel(&app_handle, &tunnel_id).await;
            });
        }
        _ => {}
    }
}

/// Update the tray menu with current tunnel status
pub async fn update_tray_menu(app: &AppHandle) {
    let state = match app.try_state::<AppState>() {
        Some(s) => s,
        None => return,
    };

    // Get tunnels from database
    let tunnels = match TunnelConfig::find().all(state.db.as_ref()).await {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to load tunnels for tray: {}", e);
            return;
        }
    };

    // Get running status
    let manager = state.tunnel_manager.read().await;

    // Count active tunnels
    let active_count = tunnels
        .iter()
        .filter(|t| {
            manager
                .get(&t.id)
                .map(|r| r.status == crate::state::TunnelStatus::Connected)
                .unwrap_or(false)
        })
        .count();

    // Update tooltip
    if let Some(tray) = app.tray_by_id("main") {
        let tooltip = if active_count == 0 {
            "LocalUp - No tunnels active".to_string()
        } else if active_count == 1 {
            "LocalUp - 1 tunnel active".to_string()
        } else {
            format!("LocalUp - {} tunnels active", active_count)
        };
        let _ = tray.set_tooltip(Some(&tooltip));
    }

    // Rebuild the menu with updated tunnel status
    let Ok(menu) = rebuild_tray_menu_with_tunnels(app, &tunnels, &manager).await else {
        return;
    };

    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Rebuild the tray menu with current tunnel data
async fn rebuild_tray_menu_with_tunnels(
    app: &AppHandle,
    tunnels: &[crate::db::entities::tunnel_config::Model],
    manager: &crate::state::TunnelManager,
) -> Result<Menu<Wry>, Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show LocalUp", true, None::<&str>)?;
    let separator1 = PredefinedMenuItem::separator(app)?;

    // Build tunnel items
    let mut tunnel_items: Vec<Box<dyn tauri::menu::IsMenuItem<Wry>>> = Vec::new();

    if tunnels.is_empty() {
        let no_tunnels = MenuItem::with_id(
            app,
            "no_tunnels",
            "No tunnels configured",
            false,
            None::<&str>,
        )?;
        tunnel_items.push(Box::new(no_tunnels));
    } else {
        for tunnel in tunnels {
            let status = manager.get(&tunnel.id);
            let is_connected = status
                .map(|s| s.status == crate::state::TunnelStatus::Connected)
                .unwrap_or(false);
            let is_connecting = status
                .map(|s| s.status == crate::state::TunnelStatus::Connecting)
                .unwrap_or(false);

            let status_indicator = if is_connected {
                "●"
            } else if is_connecting {
                "◐"
            } else {
                "○"
            };

            let label = format!("{} {}", status_indicator, tunnel.name);

            if is_connected || is_connecting {
                let item = MenuItem::with_id(
                    app,
                    format!("tunnel_stop_{}", tunnel.id),
                    format!("{} (Stop)", label),
                    true,
                    None::<&str>,
                )?;
                tunnel_items.push(Box::new(item));
            } else {
                let item = MenuItem::with_id(
                    app,
                    format!("tunnel_start_{}", tunnel.id),
                    format!("{} (Start)", label),
                    true,
                    None::<&str>,
                )?;
                tunnel_items.push(Box::new(item));
            }
        }
    }

    // Convert to references for the submenu
    let tunnel_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> =
        tunnel_items.iter().map(|b| b.as_ref()).collect();

    let tunnels_submenu = Submenu::with_items(app, "Tunnels", true, &tunnel_refs)?;

    let separator2 = PredefinedMenuItem::separator(app)?;
    let start_all = MenuItem::with_id(
        app,
        "start_all",
        "Start All Tunnels",
        !tunnels.is_empty(),
        None::<&str>,
    )?;
    let stop_all = MenuItem::with_id(
        app,
        "stop_all",
        "Stop All Tunnels",
        !tunnels.is_empty(),
        None::<&str>,
    )?;

    let separator3 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit LocalUp", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &separator1,
            &tunnels_submenu,
            &separator2,
            &start_all,
            &stop_all,
            &separator3,
            &quit,
        ],
    )?;

    Ok(menu)
}

/// Start a tunnel from tray
async fn start_tunnel(app: &AppHandle, tunnel_id: &str) {
    let state = match app.try_state::<AppState>() {
        Some(s) => s,
        None => return,
    };

    // Get tunnel config
    let tunnel = match TunnelConfig::find_by_id(tunnel_id)
        .one(state.db.as_ref())
        .await
    {
        Ok(Some(t)) => t,
        _ => {
            error!("Tunnel {} not found", tunnel_id);
            return;
        }
    };

    // Get relay config
    let relay = match RelayServer::find_by_id(&tunnel.relay_server_id)
        .one(state.db.as_ref())
        .await
    {
        Ok(Some(r)) => r,
        _ => {
            error!("Relay {} not found", tunnel.relay_server_id);
            return;
        }
    };

    info!("Starting tunnel {} from tray", tunnel.name);

    // Use the start logic from app_state
    use crate::state::TunnelStatus;
    use localup_lib::{ExitNodeConfig, TunnelConfig as ClientTunnelConfig};
    use tokio::sync::oneshot;

    // Build protocol config
    let protocol_config = match crate::state::app_state::build_protocol_config(&tunnel) {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to build protocol config: {}", e);
            return;
        }
    };

    let client_config = ClientTunnelConfig {
        local_host: tunnel.local_host.clone(),
        protocols: vec![protocol_config],
        auth_token: relay.jwt_token.clone().unwrap_or_default(),
        exit_node: ExitNodeConfig::Custom(relay.address.clone()),
        ..Default::default()
    };

    // Update status to connecting
    {
        let mut manager = state.tunnel_manager.write().await;
        manager.update_status(tunnel_id, TunnelStatus::Connecting, None, None, None);
    }

    // Update tray
    update_tray_menu(app).await;

    // Spawn tunnel task
    let tunnel_manager = state.tunnel_manager.clone();
    let tunnel_handles = state.tunnel_handles.clone();
    let tunnel_metrics = state.tunnel_metrics.clone();
    let app_handle = state.app_handle.clone();
    let config_id = tunnel_id.to_string();
    let app_clone = app.clone();

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        crate::state::app_state::run_tunnel(
            config_id.clone(),
            client_config,
            tunnel_manager,
            tunnel_metrics,
            app_handle,
            shutdown_rx,
        )
        .await;
        // Update tray when tunnel stops
        update_tray_menu(&app_clone).await;
    });

    // Store handle
    {
        let mut handles = tunnel_handles.write().await;
        handles.insert(tunnel_id.to_string(), (handle, shutdown_tx));
    }

    // Update tray after a short delay to show connected status
    let app_clone = app.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        update_tray_menu(&app_clone).await;
    });
}

/// Stop a tunnel from tray
async fn stop_tunnel(app: &AppHandle, tunnel_id: &str) {
    let state = match app.try_state::<AppState>() {
        Some(s) => s,
        None => return,
    };

    info!("Stopping tunnel {} from tray", tunnel_id);

    // Send shutdown signal
    {
        let mut handles = state.tunnel_handles.write().await;
        if let Some((handle, shutdown_tx)) = handles.remove(tunnel_id) {
            let _ = shutdown_tx.send(());
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            handle.abort();
        }
    }

    // Update status
    {
        let mut manager = state.tunnel_manager.write().await;
        manager.update_status(
            tunnel_id,
            crate::state::TunnelStatus::Disconnected,
            None,
            None,
            None,
        );
    }

    // Update tray
    update_tray_menu(app).await;
}

/// Start all tunnels
async fn start_all_tunnels(app: &AppHandle) {
    let state = match app.try_state::<AppState>() {
        Some(s) => s,
        None => return,
    };

    let tunnels = match TunnelConfig::find().all(state.db.as_ref()).await {
        Ok(t) => t,
        Err(_) => return,
    };

    // Collect tunnel IDs to start
    let tunnels_to_start: Vec<String> = {
        let manager = state.tunnel_manager.read().await;
        tunnels
            .iter()
            .filter(|tunnel| {
                let is_running = manager
                    .get(&tunnel.id)
                    .map(|s| {
                        s.status == crate::state::TunnelStatus::Connected
                            || s.status == crate::state::TunnelStatus::Connecting
                    })
                    .unwrap_or(false);
                !is_running && tunnel.enabled
            })
            .map(|t| t.id.clone())
            .collect()
    };

    // Start each tunnel
    for tunnel_id in tunnels_to_start {
        start_tunnel(app, &tunnel_id).await;
    }
}

/// Stop all tunnels
async fn stop_all_tunnels(app: &AppHandle) {
    let state = match app.try_state::<AppState>() {
        Some(s) => s,
        None => return,
    };

    let tunnels = match TunnelConfig::find().all(state.db.as_ref()).await {
        Ok(t) => t,
        Err(_) => return,
    };

    for tunnel in tunnels {
        stop_tunnel(app, &tunnel.id).await;
    }
}
