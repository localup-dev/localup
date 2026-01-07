//! Global application state

use localup_lib::{
    ExitNodeConfig, HttpMetric, MetricsEvent, MetricsStore, ProtocolConfig, TcpMetric,
    TunnelClient, TunnelConfig as ClientTunnelConfig,
};
use sea_orm::{DatabaseConnection, EntityTrait};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use super::{TunnelManager, TunnelStatus};
use crate::db::entities::{RelayServer, TunnelConfig};

/// Handle for a running tunnel task
pub type TunnelHandle = (JoinHandle<()>, oneshot::Sender<()>);

/// Global application state shared across all Tauri commands
#[derive(Clone)]
pub struct AppState {
    /// Database connection
    pub db: Arc<DatabaseConnection>,

    /// Tunnel manager for running tunnels
    pub tunnel_manager: Arc<RwLock<TunnelManager>>,

    /// Handles for running tunnel tasks (for shutdown)
    pub tunnel_handles: Arc<RwLock<HashMap<String, TunnelHandle>>>,

    /// Metrics stores for each tunnel (for querying metrics)
    pub tunnel_metrics: Arc<RwLock<HashMap<String, MetricsStore>>>,

    /// Tauri app handle for emitting events
    pub app_handle: Arc<RwLock<Option<AppHandle>>>,
}

impl AppState {
    /// Create new application state
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db: Arc::new(db),
            tunnel_manager: Arc::new(RwLock::new(TunnelManager::new())),
            tunnel_handles: Arc::new(RwLock::new(HashMap::new())),
            tunnel_metrics: Arc::new(RwLock::new(HashMap::new())),
            app_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the Tauri app handle for event emission
    pub async fn set_app_handle(&self, handle: AppHandle) {
        let mut app_handle = self.app_handle.write().await;
        *app_handle = Some(handle);
    }

    /// Get metrics for a specific tunnel
    pub async fn get_tunnel_metrics(&self, tunnel_id: &str) -> Vec<HttpMetric> {
        let metrics = self.tunnel_metrics.read().await;
        if let Some(store) = metrics.get(tunnel_id) {
            store.get_all().await
        } else {
            Vec::new()
        }
    }

    /// Get metrics for a specific tunnel with pagination
    pub async fn get_tunnel_metrics_paginated(
        &self,
        tunnel_id: &str,
        offset: usize,
        limit: usize,
    ) -> (Vec<HttpMetric>, usize) {
        let metrics = self.tunnel_metrics.read().await;
        if let Some(store) = metrics.get(tunnel_id) {
            let total = store.count().await;
            let items = store.get_paginated(offset, limit).await;
            debug!(
                "get_tunnel_metrics_paginated: tunnel_id={}, total={}, items={}",
                tunnel_id,
                total,
                items.len()
            );
            (items, total)
        } else {
            debug!(
                "get_tunnel_metrics_paginated: no store for tunnel_id={}, available keys: {:?}",
                tunnel_id,
                metrics.keys().collect::<Vec<_>>()
            );
            (Vec::new(), 0)
        }
    }

    /// Get TCP connections for a specific tunnel with pagination
    pub async fn get_tcp_connections_paginated(
        &self,
        tunnel_id: &str,
        offset: usize,
        limit: usize,
    ) -> (Vec<TcpMetric>, usize) {
        let metrics = self.tunnel_metrics.read().await;
        if let Some(store) = metrics.get(tunnel_id) {
            let total = store.tcp_connections_count().await;
            let items = store.get_tcp_connections_paginated(offset, limit).await;
            debug!(
                "get_tcp_connections_paginated: tunnel_id={}, total={}, items={}",
                tunnel_id,
                total,
                items.len()
            );
            (items, total)
        } else {
            debug!(
                "get_tcp_connections_paginated: no store for tunnel_id={}",
                tunnel_id
            );
            (Vec::new(), 0)
        }
    }

    /// Clear metrics for a specific tunnel
    pub async fn clear_tunnel_metrics(&self, tunnel_id: &str) {
        let metrics = self.tunnel_metrics.read().await;
        if let Some(store) = metrics.get(tunnel_id) {
            store.clear().await;
        }
    }

    /// Remove metrics store when tunnel stops
    pub async fn remove_tunnel_metrics(&self, tunnel_id: &str) {
        let mut metrics = self.tunnel_metrics.write().await;
        metrics.remove(tunnel_id);
    }

    /// Start all tunnels marked with auto_start=true
    pub async fn start_auto_start_tunnels(&self) {
        info!("Checking for auto-start tunnels...");

        // Get all tunnels with auto_start=true
        let tunnels = match TunnelConfig::find().all(self.db.as_ref()).await {
            Ok(tunnels) => tunnels,
            Err(e) => {
                error!("Failed to load tunnels for auto-start: {}", e);
                return;
            }
        };

        let auto_start_tunnels: Vec<_> = tunnels
            .into_iter()
            .filter(|t| t.auto_start && t.enabled)
            .collect();

        if auto_start_tunnels.is_empty() {
            info!("No auto-start tunnels configured");
            return;
        }

        info!(
            "Found {} auto-start tunnel(s), starting...",
            auto_start_tunnels.len()
        );

        // Get all relays for lookup
        let relays: HashMap<String, _> = match RelayServer::find().all(self.db.as_ref()).await {
            Ok(relays) => relays.into_iter().map(|r| (r.id.clone(), r)).collect(),
            Err(e) => {
                error!("Failed to load relays for auto-start: {}", e);
                return;
            }
        };

        for tunnel in auto_start_tunnels {
            let relay = match relays.get(&tunnel.relay_server_id) {
                Some(r) => r,
                None => {
                    error!(
                        "Relay {} not found for tunnel {}",
                        tunnel.relay_server_id, tunnel.name
                    );
                    continue;
                }
            };

            info!("Auto-starting tunnel: {}", tunnel.name);

            // Build protocol config
            let protocol_config = match build_protocol_config(&tunnel) {
                Ok(p) => p,
                Err(e) => {
                    error!("Failed to build protocol config for {}: {}", tunnel.name, e);
                    continue;
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
                let mut manager = self.tunnel_manager.write().await;
                manager.update_status(&tunnel.id, TunnelStatus::Connecting, None, None, None);
            }

            // Spawn tunnel task
            let tunnel_manager = self.tunnel_manager.clone();
            let tunnel_handles = self.tunnel_handles.clone();
            let tunnel_metrics = self.tunnel_metrics.clone();
            let app_handle = self.app_handle.clone();
            let config_id = tunnel.id.clone();

            let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

            let handle = tokio::spawn(async move {
                run_tunnel(
                    config_id.clone(),
                    client_config,
                    tunnel_manager,
                    tunnel_metrics,
                    app_handle,
                    shutdown_rx,
                )
                .await;
            });

            // Store handle for later shutdown
            {
                let mut handles = tunnel_handles.write().await;
                handles.insert(tunnel.id.clone(), (handle, shutdown_tx));
            }
        }
    }
}

/// Build protocol config from database model
pub fn build_protocol_config(
    config: &crate::db::entities::tunnel_config::Model,
) -> Result<ProtocolConfig, String> {
    let local_port = config.local_port as u16;

    match config.protocol.as_str() {
        "http" => Ok(ProtocolConfig::Http {
            local_port,
            subdomain: config.subdomain.clone(),
            custom_domain: config.custom_domain.clone(),
        }),
        "https" => Ok(ProtocolConfig::Https {
            local_port,
            subdomain: config.subdomain.clone(),
            custom_domain: config.custom_domain.clone(),
        }),
        "tcp" => Ok(ProtocolConfig::Tcp {
            local_port,
            remote_port: None,
        }),
        "tls" => Ok(ProtocolConfig::Tls {
            local_port,
            sni_hostname: config.custom_domain.clone(),
        }),
        other => Err(format!("Unknown protocol: {}", other)),
    }
}

/// Metrics event payload for Tauri
#[derive(Clone, serde::Serialize)]
pub struct TunnelMetricsPayload {
    pub tunnel_id: String,
    pub event: MetricsEvent,
}

/// Run a tunnel with reconnection logic and metrics forwarding
pub async fn run_tunnel(
    config_id: String,
    config: ClientTunnelConfig,
    tunnel_manager: Arc<RwLock<TunnelManager>>,
    tunnel_metrics: Arc<RwLock<HashMap<String, MetricsStore>>>,
    app_handle: Arc<RwLock<Option<AppHandle>>>,
    mut shutdown_rx: oneshot::Receiver<()>,
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
                config_id, backoff_seconds
            );

            tokio::time::sleep(tokio::time::Duration::from_secs(backoff_seconds)).await;
        }

        // Check for shutdown
        if shutdown_rx.try_recv().is_ok() {
            info!("[{}] Tunnel stopped by request", config_id);
            let mut manager = tunnel_manager.write().await;
            manager.update_status(&config_id, TunnelStatus::Disconnected, None, None, None);
            // Clean up metrics store
            tunnel_metrics.write().await.remove(&config_id);
            break;
        }

        info!(
            "[{}] Connecting... (attempt {})",
            config_id,
            reconnect_attempt + 1
        );

        match TunnelClient::connect(config.clone()).await {
            Ok(client) => {
                reconnect_attempt = 0;

                info!("[{}] Connected successfully!", config_id);

                let public_url = client.public_url().map(|s| s.to_string());

                if let Some(url) = &public_url {
                    info!("[{}] Public URL: {}", config_id, url);
                }

                // Store the metrics store for this tunnel
                let metrics_store = client.metrics().clone();
                {
                    let mut metrics_map = tunnel_metrics.write().await;
                    metrics_map.insert(config_id.clone(), metrics_store.clone());
                }

                // Subscribe to metrics events and forward to Tauri
                let metrics_rx = metrics_store.subscribe();
                let config_id_for_metrics = config_id.clone();
                let app_handle_for_metrics = app_handle.clone();

                let metrics_task = tokio::spawn(async move {
                    let mut rx = metrics_rx;
                    loop {
                        match rx.recv().await {
                            Ok(event) => {
                                // Emit event to frontend
                                if let Some(handle) = app_handle_for_metrics.read().await.as_ref() {
                                    let payload = TunnelMetricsPayload {
                                        tunnel_id: config_id_for_metrics.clone(),
                                        event,
                                    };
                                    if let Err(e) = handle.emit("tunnel-metrics", &payload) {
                                        warn!("Failed to emit metrics event: {}", e);
                                    }
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                debug!(
                                    "Metrics channel closed for tunnel {}",
                                    config_id_for_metrics
                                );
                                break;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                warn!("Metrics receiver lagged {} messages", n);
                            }
                        }
                    }
                });

                // Update status to connected
                {
                    let mut manager = tunnel_manager.write().await;
                    manager.update_status(
                        &config_id,
                        TunnelStatus::Connected,
                        public_url.clone(),
                        None,
                        None,
                    );
                }

                // Wait for tunnel to close or shutdown signal
                tokio::select! {
                    result = client.wait() => {
                        match result {
                            Ok(_) => {
                                info!("[{}] Tunnel closed gracefully", config_id);
                            }
                            Err(e) => {
                                error!("[{}] Tunnel error: {}", config_id, e);
                            }
                        }
                    }
                    _ = &mut shutdown_rx => {
                        info!("[{}] Shutdown requested", config_id);
                        let mut manager = tunnel_manager.write().await;
                        manager.update_status(&config_id, TunnelStatus::Disconnected, None, None, None);
                        // Clean up metrics
                        tunnel_metrics.write().await.remove(&config_id);
                        metrics_task.abort();
                        break;
                    }
                }

                // Abort metrics task when connection ends
                metrics_task.abort();

                info!(
                    "[{}] Connection lost, attempting to reconnect...",
                    config_id
                );
            }
            Err(e) => {
                error!("[{}] Failed to connect: {}", config_id, e);

                // Update status to error
                {
                    let mut manager = tunnel_manager.write().await;
                    manager.update_status(
                        &config_id,
                        TunnelStatus::Error,
                        None,
                        None,
                        Some(e.to_string()),
                    );
                }

                // Check if non-recoverable
                if e.is_non_recoverable() {
                    error!("[{}] Non-recoverable error, stopping tunnel", config_id);
                    // Clean up metrics store
                    tunnel_metrics.write().await.remove(&config_id);
                    break;
                }

                reconnect_attempt += 1;

                // Check for shutdown
                if shutdown_rx.try_recv().is_ok() {
                    info!("[{}] Tunnel stopped by request", config_id);
                    let mut manager = tunnel_manager.write().await;
                    manager.update_status(&config_id, TunnelStatus::Disconnected, None, None, None);
                    // Clean up metrics store
                    tunnel_metrics.write().await.remove(&config_id);
                    break;
                }
            }
        }
    }
}
