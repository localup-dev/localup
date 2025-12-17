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
    pub localup_id: String,
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
    pub localup_id: Option<String>,
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
    pub localup_id: String,
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
    pub localup_id: String,
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
    pub localup_id: Option<String>,
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

/// Custom domain status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum CustomDomainStatus {
    /// Certificate provisioning in progress
    Pending,
    /// Certificate active and valid
    Active,
    /// Certificate expired
    Expired,
    /// Certificate provisioning failed
    Failed,
}

/// Custom domain information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CustomDomain {
    /// Unique ID for URL routing
    pub id: String,
    /// Domain name
    pub domain: String,
    /// Certificate status
    pub status: CustomDomainStatus,
    /// When the certificate was provisioned
    pub provisioned_at: DateTime<Utc>,
    /// When the certificate expires
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether to automatically renew the certificate
    pub auto_renew: bool,
    /// Error message if provisioning failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Certificate details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CertificateDetails {
    /// Domain name
    pub domain: String,
    /// Certificate subject (CN)
    pub subject: String,
    /// Certificate issuer
    pub issuer: String,
    /// Serial number (hex)
    pub serial_number: String,
    /// Not valid before
    pub not_before: DateTime<Utc>,
    /// Not valid after
    pub not_after: DateTime<Utc>,
    /// Subject Alternative Names (SANs)
    pub san: Vec<String>,
    /// Signature algorithm
    pub signature_algorithm: String,
    /// Public key algorithm
    pub public_key_algorithm: String,
    /// Certificate fingerprint (SHA-256)
    pub fingerprint_sha256: String,
    /// Certificate in PEM format
    pub pem: String,
}

/// Request to upload a custom domain certificate
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadCustomDomainRequest {
    /// Domain name (e.g., "api.example.com")
    pub domain: String,
    /// Certificate in PEM format (base64 encoded)
    pub cert_pem: String,
    /// Private key in PEM format (base64 encoded)
    pub key_pem: String,
    /// Whether to automatically renew the certificate
    #[serde(default = "default_auto_renew")]
    pub auto_renew: bool,
}

fn default_auto_renew() -> bool {
    true
}

/// Response after uploading a custom domain
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadCustomDomainResponse {
    /// Domain name
    pub domain: String,
    /// Current status
    pub status: CustomDomainStatus,
    /// Success message
    pub message: String,
}

/// List of custom domains
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CustomDomainList {
    /// Custom domains
    pub domains: Vec<CustomDomain>,
    /// Total count
    pub total: usize,
}

/// Request to initiate ACME challenge for a domain
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InitiateChallengeRequest {
    /// Domain name to validate
    pub domain: String,
    /// Challenge type (http-01 or dns-01)
    #[serde(default = "default_challenge_type")]
    pub challenge_type: String,
}

fn default_challenge_type() -> String {
    "http-01".to_string()
}

/// Challenge information for domain validation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ChallengeInfo {
    /// HTTP-01 challenge
    Http01 {
        /// Domain being validated
        domain: String,
        /// Random token from ACME server
        token: String,
        /// Key authorization to serve
        key_authorization: String,
        /// Where to place the file
        /// Format: http://{domain}/.well-known/acme-challenge/{token}
        file_path: String,
        /// Instructions for user
        instructions: Vec<String>,
    },
    /// DNS-01 challenge
    Dns01 {
        /// Domain being validated
        domain: String,
        /// DNS record name (_acme-challenge.{domain})
        record_name: String,
        /// DNS TXT record value
        record_value: String,
        /// Instructions for user
        instructions: Vec<String>,
    },
}

/// Response after initiating a challenge
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InitiateChallengeResponse {
    /// Domain name
    pub domain: String,
    /// Challenge details
    pub challenge: ChallengeInfo,
    /// Challenge ID for completing the validation
    pub challenge_id: String,
    /// Expiration time for this challenge
    pub expires_at: DateTime<Utc>,
}

/// Request to complete/verify a challenge
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CompleteChallengeRequest {
    /// Domain name
    pub domain: String,
    /// Challenge ID from initiate response
    pub challenge_id: String,
}

/// Request to pre-validate a challenge (check setup before submitting to ACME)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PreValidateChallengeRequest {
    /// Domain name
    pub domain: String,
    /// Challenge ID from initiate response
    pub challenge_id: String,
}

/// Response from pre-validation check
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PreValidateChallengeResponse {
    /// Whether the challenge is ready to be submitted
    pub ready: bool,
    /// Challenge type (http-01 or dns-01)
    pub challenge_type: String,
    /// What was checked
    pub checked: String,
    /// What was expected
    pub expected: String,
    /// What was found (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found: Option<String>,
    /// Error message if not ready
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Additional details or suggestions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

// ============================================================================
// Authentication Models
// ============================================================================

/// User registration request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterRequest {
    /// User email address (must be unique)
    pub email: String,
    /// User password (minimum 8 characters)
    pub password: String,
    /// User full name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
}

/// User registration response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterResponse {
    /// Newly created user
    pub user: User,
    /// Session token for immediate login
    pub token: String,
    /// Token expiration timestamp
    pub expires_at: DateTime<Utc>,
    /// Authentication token for tunnel connections (only shown once)
    pub auth_token: String,
}

/// User login request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// User email address
    pub email: String,
    /// User password
    pub password: String,
}

/// User login response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    /// Logged in user
    pub user: User,
    /// Session token
    pub token: String,
    /// Token expiration timestamp
    pub expires_at: DateTime<Utc>,
}

/// User role
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    /// System administrator with full access
    Admin,
    /// Regular user
    User,
}

/// User information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct User {
    /// User UUID
    pub id: String,
    /// User email
    pub email: String,
    /// User full name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    /// User role
    pub role: UserRole,
    /// Whether the account is active
    pub is_active: bool,
    /// When the user was created
    pub created_at: DateTime<Utc>,
    /// When the user was last updated
    pub updated_at: DateTime<Utc>,
}

/// List of users
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserList {
    /// Users
    pub users: Vec<User>,
    /// Total count
    pub total: usize,
}

/// Team role
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TeamRole {
    /// Team owner with full access
    Owner,
    /// Team admin with elevated permissions
    Admin,
    /// Regular team member
    Member,
}

/// Team information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Team {
    /// Team UUID
    pub id: String,
    /// Team name
    pub name: String,
    /// Team slug (URL-friendly)
    pub slug: String,
    /// User ID of the team owner
    pub owner_id: String,
    /// When the team was created
    pub created_at: DateTime<Utc>,
    /// When the team was last updated
    pub updated_at: DateTime<Utc>,
}

/// Team member information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TeamMember {
    /// Team ID
    pub team_id: String,
    /// User information
    pub user: User,
    /// Role in the team
    pub role: TeamRole,
    /// When the user joined the team
    pub joined_at: DateTime<Utc>,
}

/// List of teams
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TeamList {
    /// Teams
    pub teams: Vec<Team>,
    /// Total count
    pub total: usize,
}

/// Request to create an auth token
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateAuthTokenRequest {
    /// User-defined name for this token
    pub name: String,
    /// Description of what this token is used for (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Token expiration in days (null = never expires)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_days: Option<i64>,
    /// Team ID if this is a team token (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
}

/// Response after creating an auth token
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateAuthTokenResponse {
    /// Token ID
    pub id: String,
    /// Token name
    pub name: String,
    /// The actual JWT token (SHOWN ONLY ONCE!)
    pub token: String,
    /// When the token expires (null = never)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// When the token was created
    pub created_at: DateTime<Utc>,
}

/// Auth token information (without the actual token value)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthToken {
    /// Token ID
    pub id: String,
    /// User ID who owns this token
    pub user_id: String,
    /// Team ID if this is a team token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    /// Token name
    pub name: String,
    /// Token description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When the token was last used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    /// When the token expires (null = never)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether the token is active
    pub is_active: bool,
    /// When the token was created
    pub created_at: DateTime<Utc>,
}

/// List of auth tokens
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthTokenList {
    /// Auth tokens
    pub tokens: Vec<AuthToken>,
    /// Total count
    pub total: usize,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthConfig {
    /// Whether public user registration is allowed
    pub signup_enabled: bool,
    /// Relay configuration (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay: Option<RelayConfig>,
}

/// Relay configuration for client setup
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RelayConfig {
    /// Public domain for the relay (e.g., "tunnel.kfs.es")
    pub domain: String,
    /// Relay address for client connections (e.g., "tunnel.kfs.es:4443")
    pub relay_addr: String,
    /// Whether HTTP/HTTPS tunnels are supported
    pub supports_http: bool,
    /// Whether TCP tunnels are supported
    pub supports_tcp: bool,
    /// HTTP port (if supports_http is true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_port: Option<u16>,
    /// HTTPS port (if supports_http is true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub https_port: Option<u16>,
}

/// Request to update an auth token
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateAuthTokenRequest {
    /// Updated token name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Updated description (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the token is active (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
}

// Re-export protocol discovery types with ToSchema
pub use localup_proto::{ProtocolDiscoveryResponse, TransportEndpoint, TransportProtocol};
