//! Tunnel manager for running tunnel clients

use std::collections::HashMap;

/// Status of a running tunnel
#[derive(Debug, Clone)]
pub struct RunningTunnel {
    /// Tunnel config ID
    pub config_id: String,
    /// Current status
    pub status: TunnelStatus,
    /// Public URL when connected
    pub public_url: Option<String>,
    /// LocalUp ID assigned by relay
    pub localup_id: Option<String>,
    /// Error message if failed
    pub error_message: Option<String>,
}

/// Tunnel status
#[derive(Debug, Clone, PartialEq)]
pub enum TunnelStatus {
    Connecting,
    Connected,
    Disconnected,
    Error,
}

impl TunnelStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TunnelStatus::Connecting => "connecting",
            TunnelStatus::Connected => "connected",
            TunnelStatus::Disconnected => "disconnected",
            TunnelStatus::Error => "error",
        }
    }
}

/// Manages running tunnel clients
pub struct TunnelManager {
    /// Running tunnels by config ID
    tunnels: HashMap<String, RunningTunnel>,
}

impl TunnelManager {
    /// Create new tunnel manager
    pub fn new() -> Self {
        Self {
            tunnels: HashMap::new(),
        }
    }

    /// Get all running tunnels
    pub fn get_all(&self) -> Vec<RunningTunnel> {
        self.tunnels.values().cloned().collect()
    }

    /// Get tunnel by config ID
    pub fn get(&self, config_id: &str) -> Option<&RunningTunnel> {
        self.tunnels.get(config_id)
    }

    /// Check if tunnel is running
    pub fn is_running(&self, config_id: &str) -> bool {
        self.tunnels.get(config_id).map_or(false, |t| {
            t.status == TunnelStatus::Connected || t.status == TunnelStatus::Connecting
        })
    }

    /// Update tunnel status
    pub fn update_status(
        &mut self,
        config_id: &str,
        status: TunnelStatus,
        public_url: Option<String>,
        localup_id: Option<String>,
        error_message: Option<String>,
    ) {
        if let Some(tunnel) = self.tunnels.get_mut(config_id) {
            tunnel.status = status;
            tunnel.public_url = public_url;
            tunnel.localup_id = localup_id;
            tunnel.error_message = error_message;
        } else {
            self.tunnels.insert(
                config_id.to_string(),
                RunningTunnel {
                    config_id: config_id.to_string(),
                    status,
                    public_url,
                    localup_id,
                    error_message,
                },
            );
        }
    }

    /// Remove tunnel from manager
    pub fn remove(&mut self, config_id: &str) {
        self.tunnels.remove(config_id);
    }
}

impl Default for TunnelManager {
    fn default() -> Self {
        Self::new()
    }
}
