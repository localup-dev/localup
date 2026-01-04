//! Daemon service implementation
//!
//! The daemon runs as a separate process and manages tunnels independently.

use localup_lib::{ExitNodeConfig, ProtocolConfig, TunnelClient, TunnelConfig};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::{oneshot, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info};

use super::protocol::{DaemonRequest, DaemonResponse, TunnelInfo};
use super::socket_path;

/// Running tunnel state
struct RunningTunnel {
    info: TunnelInfo,
    handle: JoinHandle<()>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

/// Daemon service that manages tunnels
pub struct DaemonService {
    /// Running tunnels
    tunnels: Arc<RwLock<HashMap<String, RunningTunnel>>>,
    /// Start time
    start_time: Instant,
    /// Version
    version: String,
}

impl DaemonService {
    /// Create a new daemon service
    pub fn new() -> Self {
        Self {
            tunnels: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Run the daemon service
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let socket_path = socket_path();

        // Create parent directory if needed
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove existing socket file
        let _ = std::fs::remove_file(&socket_path);

        // Bind to Unix socket
        let listener = UnixListener::bind(&socket_path)?;
        info!("Daemon listening on {:?}", socket_path);

        // Write PID file
        let pid = std::process::id();
        std::fs::write(super::pid_path(), pid.to_string())?;
        info!("Daemon started with PID {}", pid);

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let tunnels = self.tunnels.clone();
                    let version = self.version.clone();
                    let uptime = self.start_time.elapsed().as_secs();

                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, tunnels, version, uptime).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }

    /// Start a tunnel
    pub async fn start_tunnel(&self, request: DaemonRequest) -> DaemonResponse {
        if let DaemonRequest::StartTunnel {
            id,
            name,
            relay_address,
            auth_token,
            local_host,
            local_port,
            protocol,
            subdomain,
            custom_domain,
        } = request
        {
            // Check if already running
            {
                let tunnels = self.tunnels.read().await;
                if let Some(t) = tunnels.get(&id) {
                    if t.info.is_connected() || t.info.is_connecting() {
                        return DaemonResponse::Error {
                            message: "Tunnel is already running".to_string(),
                        };
                    }
                }
            }

            // Build protocol config
            let protocol_config = match protocol.as_str() {
                "http" => ProtocolConfig::Http {
                    local_port,
                    subdomain: subdomain.clone(),
                    custom_domain: custom_domain.clone(),
                },
                "https" => ProtocolConfig::Https {
                    local_port,
                    subdomain: subdomain.clone(),
                    custom_domain: custom_domain.clone(),
                },
                "tcp" => ProtocolConfig::Tcp {
                    local_port,
                    remote_port: None,
                },
                "tls" => ProtocolConfig::Tls {
                    local_port,
                    sni_hostname: custom_domain.clone(),
                },
                other => {
                    return DaemonResponse::Error {
                        message: format!("Unknown protocol: {}", other),
                    };
                }
            };

            let config = TunnelConfig {
                local_host: local_host.clone(),
                protocols: vec![protocol_config],
                auth_token,
                exit_node: ExitNodeConfig::Custom(relay_address.clone()),
                ..Default::default()
            };

            // Create tunnel info
            let info = TunnelInfo {
                id: id.clone(),
                name: name.clone(),
                relay_address: relay_address.clone(),
                local_host: local_host.clone(),
                local_port,
                protocol: protocol.clone(),
                subdomain: subdomain.clone(),
                custom_domain: custom_domain.clone(),
                status: "connecting".to_string(),
                public_url: None,
                localup_id: None,
                error_message: None,
                started_at: Some(chrono::Utc::now().to_rfc3339()),
            };

            // Spawn tunnel task
            let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
            let tunnels = self.tunnels.clone();
            let tunnel_id = id.clone();

            let handle = tokio::spawn(async move {
                run_tunnel_task(tunnel_id, config, tunnels, shutdown_rx).await;
            });

            // Store running tunnel
            {
                let mut tunnels = self.tunnels.write().await;
                tunnels.insert(
                    id.clone(),
                    RunningTunnel {
                        info,
                        handle,
                        shutdown_tx: Some(shutdown_tx),
                    },
                );
            }

            // Return current info
            let tunnels = self.tunnels.read().await;
            if let Some(t) = tunnels.get(&id) {
                DaemonResponse::Tunnel(t.info.clone())
            } else {
                DaemonResponse::Error {
                    message: "Failed to start tunnel".to_string(),
                }
            }
        } else {
            DaemonResponse::Error {
                message: "Invalid request".to_string(),
            }
        }
    }
}

impl Default for DaemonService {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle a client connection
async fn handle_connection(
    mut stream: tokio::net::UnixStream,
    tunnels: Arc<RwLock<HashMap<String, RunningTunnel>>>,
    version: String,
    uptime: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    loop {
        // Read message length (4 bytes, big-endian)
        let mut len_buf = [0u8; 4];
        match stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Client disconnected
                break;
            }
            Err(e) => return Err(e.into()),
        }
        let len = u32::from_be_bytes(len_buf) as usize;

        // Read message
        let mut msg_buf = vec![0u8; len];
        stream.read_exact(&mut msg_buf).await?;

        // Parse request
        let request: DaemonRequest = serde_json::from_slice(&msg_buf)?;

        // Handle request
        let response = handle_request(request, &tunnels, &version, uptime).await;

        // Send response
        let response_bytes = serde_json::to_vec(&response)?;
        let response_len = (response_bytes.len() as u32).to_be_bytes();
        stream.write_all(&response_len).await?;
        stream.write_all(&response_bytes).await?;
    }

    Ok(())
}

/// Handle a single request
async fn handle_request(
    request: DaemonRequest,
    tunnels: &Arc<RwLock<HashMap<String, RunningTunnel>>>,
    version: &str,
    uptime: u64,
) -> DaemonResponse {
    match request {
        DaemonRequest::Ping => {
            let tunnel_count = tunnels.read().await.len();
            DaemonResponse::Pong {
                version: version.to_string(),
                uptime_seconds: uptime,
                tunnel_count,
            }
        }

        DaemonRequest::ListTunnels => {
            let tunnels = tunnels.read().await;
            let list: Vec<TunnelInfo> = tunnels.values().map(|t| t.info.clone()).collect();
            DaemonResponse::Tunnels(list)
        }

        DaemonRequest::GetTunnel { id } => {
            let tunnels = tunnels.read().await;
            match tunnels.get(&id) {
                Some(t) => DaemonResponse::Tunnel(t.info.clone()),
                None => DaemonResponse::Error {
                    message: format!("Tunnel not found: {}", id),
                },
            }
        }

        DaemonRequest::StartTunnel {
            id,
            name,
            relay_address,
            auth_token,
            local_host,
            local_port,
            protocol,
            subdomain,
            custom_domain,
        } => {
            // Check if already running
            {
                let tunnels_read = tunnels.read().await;
                if let Some(t) = tunnels_read.get(&id) {
                    if t.info.is_connected() || t.info.is_connecting() {
                        return DaemonResponse::Error {
                            message: "Tunnel is already running".to_string(),
                        };
                    }
                }
            }

            // Build protocol config
            let protocol_config = match protocol.as_str() {
                "http" => ProtocolConfig::Http {
                    local_port,
                    subdomain: subdomain.clone(),
                    custom_domain: custom_domain.clone(),
                },
                "https" => ProtocolConfig::Https {
                    local_port,
                    subdomain: subdomain.clone(),
                    custom_domain: custom_domain.clone(),
                },
                "tcp" => ProtocolConfig::Tcp {
                    local_port,
                    remote_port: None,
                },
                "tls" => ProtocolConfig::Tls {
                    local_port,
                    sni_hostname: custom_domain.clone(),
                },
                other => {
                    return DaemonResponse::Error {
                        message: format!("Unknown protocol: {}", other),
                    };
                }
            };

            let config = TunnelConfig {
                local_host: local_host.clone(),
                protocols: vec![protocol_config],
                auth_token,
                exit_node: ExitNodeConfig::Custom(relay_address.clone()),
                ..Default::default()
            };

            // Create tunnel info
            let info = TunnelInfo {
                id: id.clone(),
                name: name.clone(),
                relay_address: relay_address.clone(),
                local_host: local_host.clone(),
                local_port,
                protocol: protocol.clone(),
                subdomain: subdomain.clone(),
                custom_domain: custom_domain.clone(),
                status: "connecting".to_string(),
                public_url: None,
                localup_id: None,
                error_message: None,
                started_at: Some(chrono::Utc::now().to_rfc3339()),
            };

            // Spawn tunnel task
            let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
            let tunnels_clone = tunnels.clone();
            let tunnel_id = id.clone();

            let handle = tokio::spawn(async move {
                run_tunnel_task(tunnel_id, config, tunnels_clone, shutdown_rx).await;
            });

            // Store running tunnel
            {
                let mut tunnels_write = tunnels.write().await;
                tunnels_write.insert(
                    id.clone(),
                    RunningTunnel {
                        info,
                        handle,
                        shutdown_tx: Some(shutdown_tx),
                    },
                );
            }

            // Return current info
            let tunnels_read = tunnels.read().await;
            if let Some(t) = tunnels_read.get(&id) {
                DaemonResponse::Tunnel(t.info.clone())
            } else {
                DaemonResponse::Error {
                    message: "Failed to start tunnel".to_string(),
                }
            }
        }

        DaemonRequest::StopTunnel { id } => {
            let mut tunnels_write = tunnels.write().await;
            if let Some(mut tunnel) = tunnels_write.remove(&id) {
                // Send shutdown signal
                if let Some(tx) = tunnel.shutdown_tx.take() {
                    let _ = tx.send(());
                }
                // Abort the task
                tunnel.handle.abort();

                DaemonResponse::Ok
            } else {
                DaemonResponse::Error {
                    message: format!("Tunnel not found: {}", id),
                }
            }
        }

        DaemonRequest::UpdateTunnel { id, .. } => {
            // For now, just return an error - would need to stop and restart
            let tunnels_read = tunnels.read().await;
            if tunnels_read.contains_key(&id) {
                DaemonResponse::Error {
                    message: "Update requires stopping and restarting the tunnel".to_string(),
                }
            } else {
                DaemonResponse::Error {
                    message: format!("Tunnel not found: {}", id),
                }
            }
        }

        DaemonRequest::DeleteTunnel { id } => {
            let mut tunnels_write = tunnels.write().await;
            if let Some(mut tunnel) = tunnels_write.remove(&id) {
                // Send shutdown signal
                if let Some(tx) = tunnel.shutdown_tx.take() {
                    let _ = tx.send(());
                }
                // Abort the task
                tunnel.handle.abort();

                DaemonResponse::Ok
            } else {
                // Tunnel wasn't running, that's fine
                DaemonResponse::Ok
            }
        }

        DaemonRequest::Shutdown => {
            info!("Shutdown requested, stopping all tunnels...");

            // Stop all tunnels
            let mut tunnels_write = tunnels.write().await;
            for (id, mut tunnel) in tunnels_write.drain() {
                info!("Stopping tunnel: {}", id);
                if let Some(tx) = tunnel.shutdown_tx.take() {
                    let _ = tx.send(());
                }
                tunnel.handle.abort();
            }

            // Schedule process exit
            tokio::spawn(async {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                std::process::exit(0);
            });

            DaemonResponse::Ok
        }
    }
}

/// Run a tunnel task with reconnection
async fn run_tunnel_task(
    tunnel_id: String,
    config: TunnelConfig,
    tunnels: Arc<RwLock<HashMap<String, RunningTunnel>>>,
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
                tunnel_id, backoff_seconds
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(backoff_seconds)).await;
        }

        // Check for shutdown
        if shutdown_rx.try_recv().is_ok() {
            info!("[{}] Tunnel stopped by request", tunnel_id);
            update_tunnel_status(&tunnels, &tunnel_id, "disconnected", None, None).await;
            break;
        }

        info!(
            "[{}] Connecting... (attempt {})",
            tunnel_id,
            reconnect_attempt + 1
        );

        match TunnelClient::connect(config.clone()).await {
            Ok(client) => {
                reconnect_attempt = 0;
                info!("[{}] Connected successfully!", tunnel_id);

                let public_url = client.public_url().map(|s| s.to_string());
                if let Some(url) = &public_url {
                    info!("[{}] Public URL: {}", tunnel_id, url);
                }

                // Update status to connected
                update_tunnel_status(&tunnels, &tunnel_id, "connected", public_url.clone(), None)
                    .await;

                // Wait for tunnel to close or shutdown signal
                tokio::select! {
                    result = client.wait() => {
                        match result {
                            Ok(_) => {
                                info!("[{}] Tunnel closed gracefully", tunnel_id);
                            }
                            Err(e) => {
                                error!("[{}] Tunnel error: {}", tunnel_id, e);
                            }
                        }
                    }
                    _ = &mut shutdown_rx => {
                        info!("[{}] Shutdown requested", tunnel_id);
                        update_tunnel_status(&tunnels, &tunnel_id, "disconnected", None, None).await;
                        break;
                    }
                }

                info!(
                    "[{}] Connection lost, attempting to reconnect...",
                    tunnel_id
                );
            }
            Err(e) => {
                error!("[{}] Failed to connect: {}", tunnel_id, e);

                // Update status to error
                update_tunnel_status(&tunnels, &tunnel_id, "error", None, Some(e.to_string()))
                    .await;

                // Check if non-recoverable
                if e.is_non_recoverable() {
                    error!("[{}] Non-recoverable error, stopping tunnel", tunnel_id);
                    break;
                }

                reconnect_attempt += 1;

                // Check for shutdown
                if shutdown_rx.try_recv().is_ok() {
                    info!("[{}] Tunnel stopped by request", tunnel_id);
                    update_tunnel_status(&tunnels, &tunnel_id, "disconnected", None, None).await;
                    break;
                }
            }
        }
    }
}

/// Update tunnel status in the shared state
async fn update_tunnel_status(
    tunnels: &Arc<RwLock<HashMap<String, RunningTunnel>>>,
    tunnel_id: &str,
    status: &str,
    public_url: Option<String>,
    error_message: Option<String>,
) {
    let mut tunnels = tunnels.write().await;
    if let Some(tunnel) = tunnels.get_mut(tunnel_id) {
        tunnel.info.status = status.to_string();
        tunnel.info.public_url = public_url;
        tunnel.info.error_message = error_message;
    }
}
