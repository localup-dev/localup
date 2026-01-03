//! Daemon mode for managing multiple tunnels
//!
//! Runs multiple tunnel connections concurrently and manages their lifecycle.
//! Includes an IPC server for CLI communication.

use anyhow::Result;
use localup_client::{ProtocolConfig, TunnelClient};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::ipc::{IpcRequest, IpcResponse, IpcServer, TunnelStatusDisplay, TunnelStatusInfo};
use crate::localup_store::{StoredTunnel, TunnelStore};

/// Daemon status for a single tunnel
#[derive(Debug, Clone)]
pub enum TunnelStatus {
    Starting,
    Connected { public_url: Option<String> },
    Reconnecting { attempt: u32 },
    Failed { error: String },
    Stopped,
}

/// Daemon command
pub enum DaemonCommand {
    /// Start a tunnel by name
    StartTunnel(String),
    /// Stop a tunnel by name
    StopTunnel(String),
    /// Get status of all tunnels
    GetStatus(mpsc::Sender<HashMap<String, TunnelStatus>>),
    /// Reload tunnel configurations from disk
    Reload,
    /// Shutdown the daemon
    Shutdown,
}

/// Daemon for managing multiple tunnels
pub struct Daemon {
    store: TunnelStore,
    tunnels: Arc<RwLock<HashMap<String, TunnelHandle>>>,
}

/// Handle for a running tunnel
struct TunnelHandle {
    status: TunnelStatus,
    cancel_tx: mpsc::Sender<()>,
    task: JoinHandle<()>,
    /// Protocol type for display
    protocol: String,
    /// Local port for display
    local_port: u16,
    /// When the tunnel connected (for uptime)
    connected_at: Option<Instant>,
}

impl Daemon {
    /// Create a new daemon
    pub fn new() -> Result<Self> {
        let store = TunnelStore::new()?;
        Ok(Self {
            store,
            tunnels: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Run the daemon
    pub async fn run(self, mut command_rx: mpsc::Receiver<DaemonCommand>) -> Result<()> {
        info!("🚀 Daemon starting...");

        // Start IPC server
        let ipc_server = match IpcServer::bind().await {
            Ok(server) => {
                info!("IPC server listening at {:?}", server.path());
                Some(server)
            }
            Err(e) => {
                warn!(
                    "Failed to start IPC server: {}. Status queries will not work.",
                    e
                );
                None
            }
        };

        // Spawn IPC handler if server is available
        if let Some(server) = ipc_server {
            let tunnels_for_ipc = self.tunnels.clone();
            let store_for_ipc = TunnelStore::new().ok();
            tokio::spawn(Self::run_ipc_server(server, tunnels_for_ipc, store_for_ipc));
        }

        // Load and start all enabled tunnels
        match self.store.list_enabled() {
            Ok(enabled_tunnels) => {
                info!("Found {} enabled tunnel(s)", enabled_tunnels.len());

                for stored_tunnel in enabled_tunnels {
                    let name = stored_tunnel.name.clone();
                    info!("Starting tunnel: {}", name);
                    if let Err(e) = self.start_tunnel(stored_tunnel).await {
                        error!("Failed to start tunnel '{}': {}", name, e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to load tunnel configurations: {}. Daemon will still run but no tunnels started.", e);
            }
        }

        info!("✅ Daemon ready");

        // Main command loop
        while let Some(command) = command_rx.recv().await {
            match command {
                DaemonCommand::StartTunnel(name) => match self.store.load(&name) {
                    Ok(stored_tunnel) => {
                        if let Err(e) = self.start_tunnel(stored_tunnel).await {
                            error!("Failed to start tunnel '{}': {}", name, e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to load tunnel '{}': {}", name, e);
                    }
                },
                DaemonCommand::StopTunnel(name) => {
                    if let Err(e) = self.stop_tunnel(&name).await {
                        error!("Failed to stop tunnel '{}': {}", name, e);
                    }
                }
                DaemonCommand::GetStatus(response_tx) => {
                    let status = self.get_status().await;
                    let _ = response_tx.send(status).await;
                }
                DaemonCommand::Reload => {
                    info!("Reloading tunnel configurations...");
                    // TODO: Implement reload logic
                }
                DaemonCommand::Shutdown => {
                    info!("Shutting down daemon...");
                    break;
                }
            }
        }

        // Stop all tunnels
        self.stop_all().await;

        info!("✅ Daemon stopped");
        Ok(())
    }

    /// Run the IPC server to handle CLI requests
    async fn run_ipc_server(
        server: IpcServer,
        tunnels: Arc<RwLock<HashMap<String, TunnelHandle>>>,
        store: Option<TunnelStore>,
    ) {
        loop {
            match server.accept().await {
                Ok(mut conn) => {
                    let tunnels = tunnels.clone();
                    let store_ref = store.as_ref();

                    // Handle request
                    let response = match conn.recv().await {
                        Ok(request) => Self::handle_ipc_request(request, &tunnels, store_ref).await,
                        Err(e) => {
                            // Connection closed or error
                            if !e.to_string().contains("Connection closed") {
                                warn!("IPC recv error: {}", e);
                            }
                            continue;
                        }
                    };

                    if let Err(e) = conn.send(&response).await {
                        warn!("IPC send error: {}", e);
                    }
                }
                Err(e) => {
                    error!("IPC accept error: {}", e);
                    // Brief pause before retrying
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Handle an IPC request
    async fn handle_ipc_request(
        request: IpcRequest,
        tunnels: &Arc<RwLock<HashMap<String, TunnelHandle>>>,
        _store: Option<&TunnelStore>,
    ) -> IpcResponse {
        match request {
            IpcRequest::Ping => IpcResponse::Pong,

            IpcRequest::GetStatus => {
                let tunnels = tunnels.read().await;
                let mut status_map = HashMap::new();

                for (name, handle) in tunnels.iter() {
                    let (status, public_url, last_error) = match &handle.status {
                        TunnelStatus::Starting => (TunnelStatusDisplay::Starting, None, None),
                        TunnelStatus::Connected { public_url } => {
                            (TunnelStatusDisplay::Connected, public_url.clone(), None)
                        }
                        TunnelStatus::Reconnecting { attempt } => (
                            TunnelStatusDisplay::Reconnecting { attempt: *attempt },
                            None,
                            None,
                        ),
                        TunnelStatus::Failed { error } => {
                            (TunnelStatusDisplay::Failed, None, Some(error.clone()))
                        }
                        TunnelStatus::Stopped => (TunnelStatusDisplay::Stopped, None, None),
                    };

                    let uptime_seconds = handle.connected_at.map(|t| t.elapsed().as_secs());

                    status_map.insert(
                        name.clone(),
                        TunnelStatusInfo {
                            name: name.clone(),
                            protocol: handle.protocol.clone(),
                            local_port: handle.local_port,
                            public_url,
                            status,
                            uptime_seconds,
                            last_error,
                        },
                    );
                }

                IpcResponse::Status {
                    tunnels: status_map,
                }
            }

            IpcRequest::StartTunnel { name } => {
                // Note: Actually starting requires access to the daemon command channel
                // For now, return an error suggesting to use daemon commands
                IpcResponse::Error {
                    message: format!(
                        "Starting tunnel '{}' via IPC not yet implemented. Use 'localup daemon start' instead.",
                        name
                    ),
                }
            }

            IpcRequest::StopTunnel { name } => {
                // Similar limitation
                IpcResponse::Error {
                    message: format!(
                        "Stopping tunnel '{}' via IPC not yet implemented. Use 'localup daemon stop' instead.",
                        name
                    ),
                }
            }

            IpcRequest::Reload => {
                // Would need access to daemon command channel
                IpcResponse::Error {
                    message: "Reload via IPC not yet implemented.".to_string(),
                }
            }
        }
    }

    /// Start a tunnel
    async fn start_tunnel(&self, stored_tunnel: StoredTunnel) -> Result<()> {
        let name = stored_tunnel.name.clone();

        // Check if already running
        {
            let tunnels = self.tunnels.read().await;
            if tunnels.contains_key(&name) {
                warn!("Tunnel '{}' is already running", name);
                return Ok(());
            }
        }

        // Extract protocol and port for status display
        let (protocol, local_port) = stored_tunnel
            .config
            .protocols
            .first()
            .map(|p| match p {
                ProtocolConfig::Http { local_port, .. } => ("http".to_string(), *local_port),
                ProtocolConfig::Https { local_port, .. } => ("https".to_string(), *local_port),
                ProtocolConfig::Tcp { local_port, .. } => ("tcp".to_string(), *local_port),
                ProtocolConfig::Tls { local_port, .. } => ("tls".to_string(), *local_port),
            })
            .unwrap_or(("unknown".to_string(), 0));

        let (cancel_tx, cancel_rx) = mpsc::channel::<()>(1);

        // Update status to Starting
        let tunnels_clone = self.tunnels.clone();
        {
            let mut tunnels = tunnels_clone.write().await;
            tunnels.insert(
                name.clone(),
                TunnelHandle {
                    status: TunnelStatus::Starting,
                    cancel_tx: cancel_tx.clone(),
                    task: tokio::spawn(async {}), // Placeholder, will be replaced
                    protocol,
                    local_port,
                    connected_at: None,
                },
            );
        }

        // Spawn tunnel task
        let task = tokio::spawn(Self::run_tunnel(
            name.clone(),
            stored_tunnel.config,
            tunnels_clone.clone(),
            cancel_rx,
        ));

        // Update with real task handle
        {
            let mut tunnels = tunnels_clone.write().await;
            if let Some(handle) = tunnels.get_mut(&name) {
                handle.task = task;
            }
        }

        Ok(())
    }

    /// Stop a tunnel
    async fn stop_tunnel(&self, name: &str) -> Result<()> {
        let mut tunnels = self.tunnels.write().await;

        if let Some(handle) = tunnels.remove(name) {
            info!("Stopping tunnel: {}", name);
            let _ = handle.cancel_tx.send(()).await;
            handle.task.abort();
            Ok(())
        } else {
            anyhow::bail!("Tunnel '{}' is not running", name);
        }
    }

    /// Stop all tunnels
    async fn stop_all(&self) {
        let mut tunnels = self.tunnels.write().await;
        for (name, handle) in tunnels.drain() {
            info!("Stopping tunnel: {}", name);
            let _ = handle.cancel_tx.send(()).await;
            handle.task.abort();
        }
    }

    /// Get status of all tunnels
    async fn get_status(&self) -> HashMap<String, TunnelStatus> {
        let tunnels = self.tunnels.read().await;
        tunnels
            .iter()
            .map(|(name, handle)| (name.clone(), handle.status.clone()))
            .collect()
    }

    /// Run a single tunnel with reconnection logic
    async fn run_tunnel(
        name: String,
        config: localup_client::TunnelConfig,
        tunnels: Arc<RwLock<HashMap<String, TunnelHandle>>>,
        mut cancel_rx: mpsc::Receiver<()>,
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
                    name, backoff_seconds
                );

                // Update status to Reconnecting
                Self::update_status(
                    &tunnels,
                    &name,
                    TunnelStatus::Reconnecting {
                        attempt: reconnect_attempt,
                    },
                )
                .await;

                tokio::time::sleep(tokio::time::Duration::from_secs(backoff_seconds)).await;
            }

            // Check for cancellation
            if cancel_rx.try_recv().is_ok() {
                info!("[{}] Tunnel stopped by request", name);
                Self::update_status(&tunnels, &name, TunnelStatus::Stopped).await;
                break;
            }

            info!(
                "[{}] Connecting... (attempt {})",
                name,
                reconnect_attempt + 1
            );

            match TunnelClient::connect(config.clone()).await {
                Ok(client) => {
                    reconnect_attempt = 0; // Reset on successful connection

                    info!("[{}] ✅ Connected successfully!", name);

                    let public_url = client.public_url().map(|s| s.to_string());

                    if let Some(url) = &public_url {
                        info!("[{}] 🌐 Public URL: {}", name, url);
                    }

                    // Update status to Connected
                    Self::update_status(
                        &tunnels,
                        &name,
                        TunnelStatus::Connected {
                            public_url: public_url.clone(),
                        },
                    )
                    .await;

                    // Get disconnect handle
                    let disconnect_future = client.disconnect_handle();

                    // Spawn wait task
                    let mut wait_task = tokio::spawn(client.wait());

                    // Wait for cancellation or tunnel close
                    tokio::select! {
                        wait_result = &mut wait_task => {
                            match wait_result {
                                Ok(Ok(_)) => {
                                    info!("[{}] Tunnel closed gracefully", name);
                                }
                                Ok(Err(e)) => {
                                    error!("[{}] Tunnel error: {}", name, e);
                                }
                                Err(e) => {
                                    error!("[{}] Tunnel task panicked: {}", name, e);
                                }
                            }
                        }
                        _ = cancel_rx.recv() => {
                            info!("[{}] Shutdown requested, sending disconnect...", name);

                            // Send graceful disconnect
                            if let Err(e) = disconnect_future.await {
                                error!("[{}] Failed to trigger disconnect: {}", name, e);
                            }

                            // Wait for graceful shutdown
                            match tokio::time::timeout(
                                tokio::time::Duration::from_secs(5),
                                wait_task
                            ).await {
                                Ok(Ok(Ok(_))) => {
                                    info!("[{}] ✅ Closed gracefully", name);
                                }
                                Ok(Ok(Err(e))) => {
                                    error!("[{}] Error during shutdown: {}", name, e);
                                }
                                Ok(Err(e)) => {
                                    error!("[{}] Task panicked during shutdown: {}", name, e);
                                }
                                Err(_) => {
                                    warn!("[{}] Graceful shutdown timed out", name);
                                }
                            }

                            Self::update_status(&tunnels, &name, TunnelStatus::Stopped).await;
                            break;
                        }
                    }

                    info!("[{}] 🔄 Connection lost, attempting to reconnect...", name);
                }
                Err(e) => {
                    error!("[{}] ❌ Failed to connect: {}", name, e);

                    // Update status to Failed
                    Self::update_status(
                        &tunnels,
                        &name,
                        TunnelStatus::Failed {
                            error: e.to_string(),
                        },
                    )
                    .await;

                    // Check if non-recoverable
                    if e.is_non_recoverable() {
                        error!("[{}] 🚫 Non-recoverable error, stopping tunnel", name);
                        break;
                    }

                    reconnect_attempt += 1;

                    // Check for cancellation
                    if cancel_rx.try_recv().is_ok() {
                        info!("[{}] Tunnel stopped by request", name);
                        Self::update_status(&tunnels, &name, TunnelStatus::Stopped).await;
                        break;
                    }
                }
            }
        }
    }

    /// Update tunnel status
    async fn update_status(
        tunnels: &Arc<RwLock<HashMap<String, TunnelHandle>>>,
        name: &str,
        status: TunnelStatus,
    ) {
        let mut tunnels = tunnels.write().await;
        if let Some(handle) = tunnels.get_mut(name) {
            // Track connected_at for uptime calculation
            if matches!(status, TunnelStatus::Connected { .. }) {
                handle.connected_at = Some(Instant::now());
            } else if !matches!(status, TunnelStatus::Reconnecting { .. }) {
                // Reset connected_at when not connected or reconnecting
                handle.connected_at = None;
            }
            handle.status = status;
        }
    }
}

impl Default for Daemon {
    fn default() -> Self {
        Self::new().expect("Failed to create daemon")
    }
}
