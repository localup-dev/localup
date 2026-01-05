//! IPC protocol for daemon communication
//!
//! Messages are JSON-encoded with a length prefix (4 bytes, big-endian).

use localup_lib::{HttpMetric, MetricsEvent};
use serde::{Deserialize, Serialize};

/// Request from client to daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DaemonRequest {
    /// Ping the daemon to check if it's alive
    Ping,

    /// List all tunnels
    ListTunnels,

    /// Get tunnel by ID
    GetTunnel { id: String },

    /// Start a tunnel
    StartTunnel {
        id: String,
        name: String,
        relay_address: String,
        auth_token: String,
        local_host: String,
        local_port: u16,
        protocol: String,
        subdomain: Option<String>,
        custom_domain: Option<String>,
    },

    /// Stop a tunnel
    StopTunnel { id: String },

    /// Update tunnel configuration (will restart if running)
    UpdateTunnel {
        id: String,
        name: Option<String>,
        relay_address: Option<String>,
        auth_token: Option<String>,
        local_host: Option<String>,
        local_port: Option<u16>,
        protocol: Option<String>,
        subdomain: Option<String>,
        custom_domain: Option<String>,
    },

    /// Delete a tunnel (will stop if running)
    DeleteTunnel { id: String },

    /// Get metrics for a tunnel
    GetTunnelMetrics {
        id: String,
        offset: Option<usize>,
        limit: Option<usize>,
    },

    /// Clear metrics for a tunnel
    ClearTunnelMetrics { id: String },

    /// Subscribe to metrics events for a tunnel (streaming)
    SubscribeMetrics { id: String },

    /// Unsubscribe from metrics events
    UnsubscribeMetrics { id: String },

    /// Shutdown the daemon
    Shutdown,
}

/// Response from daemon to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum DaemonResponse {
    /// Success with no data
    Ok,

    /// Pong response
    Pong {
        version: String,
        uptime_seconds: u64,
        tunnel_count: usize,
    },

    /// Error response
    Error { message: String },

    /// Single tunnel info
    Tunnel(TunnelInfo),

    /// List of tunnels
    Tunnels(Vec<TunnelInfo>),

    /// Metrics response with pagination
    Metrics {
        items: Vec<HttpMetric>,
        total: usize,
        offset: usize,
        limit: usize,
    },

    /// Real-time metrics event (streamed)
    MetricsEvent {
        tunnel_id: String,
        event: MetricsEvent,
    },

    /// Subscription started successfully
    Subscribed { id: String },
}

/// Tunnel information from daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelInfo {
    pub id: String,
    pub name: String,
    pub relay_address: String,
    pub local_host: String,
    pub local_port: u16,
    pub protocol: String,
    pub subdomain: Option<String>,
    pub custom_domain: Option<String>,
    pub status: String,
    pub public_url: Option<String>,
    pub localup_id: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<String>,
}

impl TunnelInfo {
    /// Check if the tunnel is connected
    pub fn is_connected(&self) -> bool {
        self.status == "connected"
    }

    /// Check if the tunnel is connecting
    pub fn is_connecting(&self) -> bool {
        self.status == "connecting"
    }
}
