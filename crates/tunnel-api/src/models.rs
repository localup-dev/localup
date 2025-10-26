use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Tunnel protocol type
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TunnelProtocol {
    /// HTTP tunnel
    Http {
        /// Subdomain for the tunnel
        subdomain: String,
    },
    /// HTTPS tunnel
    Https {
        /// Subdomain for the tunnel
        subdomain: String,
    },
    /// TCP tunnel
    Tcp {
        /// Local port to forward
        port: u16,
    },
    /// TLS tunnel with SNI
    Tls {
        /// Domain for SNI routing
        domain: String,
    },
}

/// Tunnel endpoint information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TunnelEndpoint {
    /// Protocol type
    pub protocol: TunnelProtocol,
    /// Public URL accessible from internet
    pub public_url: String,
    /// Allocated port (for TCP tunnels)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

/// Tunnel status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TunnelStatus {
    /// Tunnel is connected and active
    Connected,
    /// Tunnel is disconnected
    Disconnected,
    /// Tunnel is connecting
    Connecting,
    /// Tunnel has an error
    Error,
}

/// Tunnel information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Tunnel {
    /// Unique tunnel identifier
    pub id: String,
    /// Tunnel endpoints
    pub endpoints: Vec<TunnelEndpoint>,
    /// Tunnel status
    pub status: TunnelStatus,
    /// Tunnel region/location
    pub region: String,
    /// Connection timestamp
    pub connected_at: DateTime<Utc>,
    /// Local address being forwarded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_addr: Option<String>,
}

/// Request to create a new tunnel
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTunnelRequest {
    /// List of endpoints to create
    pub endpoints: Vec<TunnelProtocol>,
    /// Desired region (optional, auto-selected if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

/// Response when creating a tunnel
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTunnelResponse {
    /// Created tunnel information
    pub tunnel: Tunnel,
    /// Authentication token for connecting
    pub token: String,
}

/// List of tunnels
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TunnelList {
    /// Tunnels
    pub tunnels: Vec<Tunnel>,
    /// Total count
    pub total: usize,
}

/// HTTP request captured in traffic inspector
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CapturedRequest {
    /// Unique request ID
    pub id: String,
    /// Tunnel ID this request belongs to
    pub tunnel_id: String,
    /// HTTP method
    pub method: String,
    /// Request path
    pub path: String,
    /// Request headers
    pub headers: Vec<(String, String)>,
    /// Request body (base64 encoded if binary)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Response status code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// Response headers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_headers: Option<Vec<(String, String)>>,
    /// Response body (base64 encoded if binary)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
    /// Request timestamp
    pub timestamp: DateTime<Utc>,
    /// Request duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Request size in bytes
    pub size_bytes: usize,
}

/// List of captured requests with pagination metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CapturedRequestList {
    /// Captured requests
    pub requests: Vec<CapturedRequest>,
    /// Total count (without pagination)
    pub total: usize,
    /// Current page offset
    pub offset: usize,
    /// Page size limit
    pub limit: usize,
}

/// Query parameters for filtering captured requests
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CapturedRequestQuery {
    /// Filter by tunnel ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tunnel_id: Option<String>,
    /// Filter by HTTP method
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// Filter by path (supports partial match)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Filter by status code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// Filter by minimum status code (for range queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_min: Option<u16>,
    /// Filter by maximum status code (for range queries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_max: Option<u16>,
    /// Pagination offset (default: 0)
    #[serde(default)]
    pub offset: Option<usize>,
    /// Pagination limit (default: 100, max: 1000)
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Tunnel metrics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TunnelMetrics {
    /// Tunnel ID
    pub tunnel_id: String,
    /// Total requests
    pub total_requests: u64,
    /// Requests per minute
    pub requests_per_minute: f64,
    /// Average latency in milliseconds
    pub avg_latency_ms: f64,
    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,
    /// Total bandwidth in bytes
    pub total_bandwidth_bytes: u64,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    /// Service status
    pub status: String,
    /// Service version
    pub version: String,
    /// Active tunnels count
    pub active_tunnels: usize,
}

/// Error response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    /// Error message
    pub error: String,
    /// Error code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// TCP connection information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CapturedTcpConnection {
    /// Connection ID
    pub id: String,
    /// Tunnel ID
    pub tunnel_id: String,
    /// Client address
    pub client_addr: String,
    /// Target port
    pub target_port: u16,
    /// Bytes received from client
    pub bytes_received: i64,
    /// Bytes sent to client
    pub bytes_sent: i64,
    /// Connection timestamp
    pub connected_at: DateTime<Utc>,
    /// Disconnection timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disconnected_at: Option<DateTime<Utc>>,
    /// Connection duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i32>,
    /// Disconnect reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disconnect_reason: Option<String>,
}

/// Query parameters for filtering TCP connections
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CapturedTcpConnectionQuery {
    /// Filter by tunnel ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tunnel_id: Option<String>,
    /// Filter by client address (partial match)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_addr: Option<String>,
    /// Filter by target port
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_port: Option<u16>,
    /// Pagination offset
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    /// Pagination limit (default: 100, max: 1000)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// List of TCP connections with pagination
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CapturedTcpConnectionList {
    /// TCP connections
    pub connections: Vec<CapturedTcpConnection>,
    /// Total count (without pagination)
    pub total: usize,
    /// Current offset
    pub offset: usize,
    /// Page size
    pub limit: usize,
}
