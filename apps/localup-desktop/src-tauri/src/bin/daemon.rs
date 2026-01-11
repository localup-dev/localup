//! LocalUp Daemon - Background tunnel service
//!
//! This daemon runs independently of the Tauri app and manages tunnels.
//! It can be installed as a system service (launchd on macOS, systemd on Linux).

use localup_desktop_lib::daemon::DaemonService;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("localup_daemon=debug".parse().unwrap())
                .add_directive("localup_desktop_lib=debug".parse().unwrap()),
        )
        .init();

    info!("LocalUp Daemon starting...");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));

    // Create and run daemon service
    let service = DaemonService::new();
    service.run().await?;

    Ok(())
}
