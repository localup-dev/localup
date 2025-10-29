//! Tunnel Manager Service - Coordinates multiple tunnel instances
//!
//! This module manages the lifecycle of multiple tunnels, allowing the desktop app
//! to create, start, stop, and monitor multiple tunnels simultaneously.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tunnel_lib::{
    ExitNodeConfig, MetricsStore, ProtocolConfig, Region, TunnelClient, TunnelConfig,
};
use uuid::Uuid;

/// Tunnel instance information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TunnelInfo {
    pub id: String,
    pub name: String,
    pub status: TunnelStatus,
    pub config: TunnelConfigDto,
    pub endpoints: Vec<EndpointDto>,
    pub created_at: i64,
    pub connected_at: Option<i64>,
    pub error: Option<String>,
}

/// Tunnel status
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelStatus {
    Connecting,
    Connected,
    Disconnected,
    Error,
}

/// Configuration DTO for serialization
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TunnelConfigDto {
    pub name: String,
    pub local_host: String,
    pub protocols: Vec<ProtocolConfigDto>,
    pub auth_token: String,
    pub exit_node: ExitNodeConfigDto,
    pub failover: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProtocolConfigDto {
    Tcp {
        local_port: u16,
        remote_port: Option<u16>,
    },
    Tls {
        local_port: u16,
        subdomain: Option<String>,
        remote_port: Option<u16>,
    },
    Http {
        local_port: u16,
        subdomain: Option<String>,
    },
    Https {
        local_port: u16,
        subdomain: Option<String>,
        custom_domain: Option<String>,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ExitNodeConfigDto {
    Auto,
    Nearest,
    Specific { region: RegionDto },
    MultiRegion { regions: Vec<RegionDto> },
    Custom { address: String },
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegionDto {
    UsEast,
    UsWest,
    EuWest,
    EuCentral,
    AsiaPacific,
    SouthAmerica,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EndpointDto {
    pub protocol: String,
    pub public_url: String,
    pub port: Option<u16>,
}

/// Active tunnel instance
struct ActiveTunnel {
    metrics: Arc<MetricsStore>,
    info: TunnelInfo,
    disconnect_tx: tokio::sync::mpsc::Sender<()>,
    _handle: tokio::task::JoinHandle<Result<(), String>>,
}

/// Tunnel Manager - Orchestrates multiple tunnels
pub struct TunnelManager {
    tunnels: Arc<RwLock<HashMap<String, ActiveTunnel>>>,
}

impl TunnelManager {
    pub fn new() -> Self {
        Self {
            tunnels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create and start a new tunnel
    pub async fn create_tunnel(&self, config_dto: TunnelConfigDto) -> Result<TunnelInfo, String> {
        let tunnel_id = Uuid::new_v4().to_string();

        // Convert DTO to tunnel-lib config
        let config = Self::dto_to_config(config_dto.clone())?;

        // Create initial info
        let mut info = TunnelInfo {
            id: tunnel_id.clone(),
            name: config_dto.name.clone(),
            status: TunnelStatus::Connecting,
            config: config_dto,
            endpoints: Vec::new(),
            created_at: chrono::Utc::now().timestamp_millis(),
            connected_at: None,
            error: None,
        };

        // Connect to tunnel
        let client = match TunnelClient::connect(config).await {
            Ok(client) => client,
            Err(e) => {
                info.status = TunnelStatus::Error;
                info.error = Some(e.to_string());
                return Err(e.to_string());
            }
        };

        // Get endpoints (before consuming client)
        let endpoints = client
            .endpoints()
            .iter()
            .map(|ep| EndpointDto {
                protocol: format!("{:?}", ep.protocol),
                public_url: ep.public_url.clone(),
                port: ep.port,
            })
            .collect();

        // Clone metrics (before consuming client)
        let metrics = Arc::new(client.metrics().clone());

        info.status = TunnelStatus::Connected;
        info.endpoints = endpoints;
        info.connected_at = Some(chrono::Utc::now().timestamp_millis());

        // Create channel for disconnect signaling
        let (disconnect_tx, mut disconnect_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Spawn task to actually run the tunnel
        let tunnel_id_clone = tunnel_id.clone();
        let tunnels_ref = Arc::clone(&self.tunnels);

        let handle = tokio::spawn(async move {
            tokio::select! {
                // Run the tunnel connection loop (handles all traffic)
                result = client.wait() => {
                    match result {
                        Ok(()) => {
                            tracing::info!("Tunnel {} closed gracefully", tunnel_id_clone);

                            // Update status to disconnected
                            let mut tunnels = tunnels_ref.write().await;
                            if let Some(tunnel) = tunnels.get_mut(&tunnel_id_clone) {
                                tunnel.info.status = TunnelStatus::Disconnected;
                            }

                            Ok(())
                        }
                        Err(e) => {
                            tracing::error!("Tunnel {} error: {}", tunnel_id_clone, e);

                            // Update status to error
                            let mut tunnels = tunnels_ref.write().await;
                            if let Some(tunnel) = tunnels.get_mut(&tunnel_id_clone) {
                                tunnel.info.status = TunnelStatus::Error;
                                tunnel.info.error = Some(e.to_string());
                            }

                            Err(e.to_string())
                        }
                    }
                }
                // Or wait for disconnect signal
                _ = disconnect_rx.recv() => {
                    tracing::info!("Tunnel {} received disconnect signal", tunnel_id_clone);
                    Ok(())
                }
            }
        });

        // Store active tunnel
        let active_tunnel = ActiveTunnel {
            metrics,
            info: info.clone(),
            disconnect_tx,
            _handle: handle,
        };

        self.tunnels.write().await.insert(tunnel_id, active_tunnel);

        Ok(info)
    }

    /// Stop a tunnel
    pub async fn stop_tunnel(&self, tunnel_id: &str) -> Result<(), String> {
        let mut tunnels = self.tunnels.write().await;

        if let Some(tunnel) = tunnels.get_mut(tunnel_id) {
            tracing::info!("🛑 [TunnelManager] Stopping tunnel: {}", tunnel_id);

            // Update status to disconnected
            tunnel.info.status = TunnelStatus::Disconnected;

            // Send disconnect signal to the running tunnel task
            if let Err(e) = tunnel.disconnect_tx.send(()).await {
                tracing::warn!("Failed to send disconnect signal: {}", e);
            }

            // Give it a moment to clean up
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            tracing::info!(
                "✅ [TunnelManager] Tunnel stopped successfully: {}",
                tunnel_id
            );
            Ok(())
        } else {
            tracing::error!("❌ [TunnelManager] Tunnel not found: {}", tunnel_id);
            Err("Tunnel not found".to_string())
        }
    }

    /// Get all tunnels
    pub async fn list_tunnels(&self) -> Vec<TunnelInfo> {
        let tunnels = self.tunnels.read().await;
        tunnels.values().map(|t| t.info.clone()).collect()
    }

    /// Get a specific tunnel
    pub async fn get_tunnel(&self, tunnel_id: &str) -> Option<TunnelInfo> {
        let tunnels = self.tunnels.read().await;
        tunnels.get(tunnel_id).map(|t| t.info.clone())
    }

    /// Get metrics for a tunnel
    pub async fn get_metrics(&self, tunnel_id: &str) -> Option<Arc<MetricsStore>> {
        let tunnels = self.tunnels.read().await;
        tunnels.get(tunnel_id).map(|t| Arc::clone(&t.metrics))
    }

    /// Stop all tunnels
    pub async fn stop_all_tunnels(&self) {
        let mut tunnels = self.tunnels.write().await;

        for (id, tunnel) in tunnels.drain() {
            // Send disconnect signal to each running tunnel task
            if let Err(e) = tunnel.disconnect_tx.send(()).await {
                tracing::warn!("Error sending disconnect to tunnel {}: {}", id, e);
            }
        }

        // Give them a moment to clean up
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    /// Convert DTO to tunnel-lib config
    fn dto_to_config(dto: TunnelConfigDto) -> Result<TunnelConfig, String> {
        let protocols = dto
            .protocols
            .into_iter()
            .map(Self::dto_to_protocol_config)
            .collect();

        let exit_node = Self::dto_to_exit_node_config(dto.exit_node);

        Ok(TunnelConfig {
            local_host: dto.local_host,
            protocols,
            auth_token: dto.auth_token,
            exit_node,
            failover: dto.failover,
            connection_timeout: std::time::Duration::from_secs(30),
        })
    }

    fn dto_to_protocol_config(dto: ProtocolConfigDto) -> ProtocolConfig {
        match dto {
            ProtocolConfigDto::Tcp {
                local_port,
                remote_port,
            } => ProtocolConfig::Tcp {
                local_port,
                remote_port,
            },
            ProtocolConfigDto::Tls {
                local_port,
                subdomain,
                remote_port,
            } => ProtocolConfig::Tls {
                local_port,
                subdomain,
                remote_port,
            },
            ProtocolConfigDto::Http {
                local_port,
                subdomain,
            } => ProtocolConfig::Http {
                local_port,
                subdomain,
            },
            ProtocolConfigDto::Https {
                local_port,
                subdomain,
                custom_domain,
            } => ProtocolConfig::Https {
                local_port,
                subdomain,
                custom_domain,
            },
        }
    }

    fn dto_to_exit_node_config(dto: ExitNodeConfigDto) -> ExitNodeConfig {
        match dto {
            ExitNodeConfigDto::Auto => ExitNodeConfig::Auto,
            ExitNodeConfigDto::Nearest => ExitNodeConfig::Nearest,
            ExitNodeConfigDto::Specific { region } => {
                ExitNodeConfig::Specific(Self::dto_to_region(region))
            }
            ExitNodeConfigDto::MultiRegion { regions } => {
                ExitNodeConfig::MultiRegion(regions.into_iter().map(Self::dto_to_region).collect())
            }
            ExitNodeConfigDto::Custom { address } => ExitNodeConfig::Custom(address),
        }
    }

    fn dto_to_region(dto: RegionDto) -> Region {
        match dto {
            RegionDto::UsEast => Region::UsEast,
            RegionDto::UsWest => Region::UsWest,
            RegionDto::EuWest => Region::EuWest,
            RegionDto::EuCentral => Region::EuCentral,
            RegionDto::AsiaPacific => Region::AsiaPacific,
            RegionDto::SouthAmerica => Region::SouthAmerica,
        }
    }
}

impl Default for TunnelManager {
    fn default() -> Self {
        Self::new()
    }
}
