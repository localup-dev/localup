//! Tunnel Library - Public API for Rust applications using the geo-distributed tunnel system
//!
//! This library re-exports all the tunnel crates, providing a unified entry point
//! for Rust applications that want to integrate tunnel functionality (either as clients or relay servers).
//!
//! # Quick Start - Tunnel Client
//!
//! ```ignore
//! use localup_lib::{TunnelClient, TunnelConfig, ExitNodeConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = TunnelConfig {
//!         local_host: "127.0.0.1".to_string(),
//!         exit_node: ExitNodeConfig::Custom("localhost:4443".to_string()),
//!         ..Default::default()
//!     };
//!
//!     let client = TunnelClient::connect(config).await?;
//!
//!     if let Some(url) = client.public_url() {
//!         println!("Tunnel URL: {}", url);
//!     }
//!
//!     client.wait().await?;
//!     Ok(())
//! }
//! ```
//!
//! # Programmatic Exit Node Creation
//!
//! You can programmatically create exit nodes with custom authentication logic:
//!
//! ```ignore
//! use localup_lib::{
//!     TcpServer, TcpServerConfig, HttpsServer, HttpsServerConfig,
//!     RouteRegistry, TunnelConnectionManager, PendingRequests,
//!     JwtValidator,
//! };
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Shared infrastructure
//! let route_registry = Arc::new(RouteRegistry::new());
//! let localup_manager = Arc::new(TunnelConnectionManager::new());
//! let pending_requests = Arc::new(PendingRequests::new());
//!
//! // JWT authentication
//! let jwt_secret = std::env::var("JWT_SECRET")?;
//! let jwt_validator = JwtValidator::new(jwt_secret.as_bytes())
//!     .with_issuer("your-app".to_string())
//!     .with_audience("your-app/relay".to_string());
//!
//! // HTTP server (port 8080)
//! let http_config = TcpServerConfig {
//!     bind_addr: "0.0.0.0:8080".parse()?,
//! };
//! let http_server = TcpServer::new(http_config, route_registry.clone())
//!     .with_localup_manager(localup_manager.clone())
//!     .with_pending_requests(pending_requests.clone());
//!
//! tokio::spawn(async move { http_server.start().await });
//!
//! // HTTPS server (port 443)
//! let https_config = HttpsServerConfig {
//!     bind_addr: "0.0.0.0:443".parse()?,
//!     cert_path: "cert.pem".to_string(),
//!     key_path: "key.pem".to_string(),
//! };
//! let https_server = HttpsServer::new(https_config, route_registry.clone())
//!     .with_localup_manager(localup_manager.clone())
//!     .with_pending_requests(pending_requests.clone());
//!
//! tokio::spawn(async move { https_server.start().await });
//!
//! // Control plane listener (implement JWT validation here)
//! // - Listen for QUIC connections
//! // - Validate JWT tokens using jwt_validator
//! // - Register routes in route_registry
//! // - Store connections in localup_manager
//! # Ok(())
//! # }
//! ```
//!
//! ## Key Components
//!
//! - **RouteRegistry**: Maps routes (TCP ports, HTTP hosts, SNI) to tunnel IDs
//! - **TunnelConnectionManager**: Manages active tunnel QUIC connections
//! - **PendingRequests**: Tracks in-flight HTTP requests
//! - **JwtValidator**: Validates JWT tokens for tunnel authentication
//! - **TcpServer**: HTTP exit node (routes by Host header)
//! - **HttpsServer**: HTTPS exit node with TLS termination
//! - **TlsServer**: TLS/SNI exit node with TLS passthrough
//!
//! ## Authentication Flow
//!
//! 1. Your application generates an auth token (JWT, API key, etc.)
//! 2. Client presents token during tunnel connection
//! 3. Your control plane validates token using `AuthValidator` trait
//! 4. On success, register route and store connection
//! 5. Exit nodes route traffic based on registered routes
//!
//! ## Custom Authentication
//!
//! The `AuthValidator` trait allows you to implement any authentication strategy:
//!
//! ```ignore
//! use localup_lib::{async_trait, AuthValidator, AuthResult, AuthError};
//! use std::collections::HashMap;
//!
//! // Example: API Key Validator
//! struct ApiKeyValidator {
//!     valid_keys: HashMap<String, String>, // key -> localup_id
//! }
//!
//! #[async_trait]
//! impl AuthValidator for ApiKeyValidator {
//!     async fn validate(&self, token: &str) -> Result<AuthResult, AuthError> {
//!         match self.valid_keys.get(token) {
//!             Some(localup_id) => Ok(AuthResult::new(localup_id.clone())
//!                 .with_metadata("auth_type".to_string(), "api_key".to_string())),
//!             None => Err(AuthError::InvalidToken("Unknown API key".to_string())),
//!         }
//!     }
//! }
//!
//! // Use it with any validator
//! let validator: Arc<dyn AuthValidator> = Arc::new(ApiKeyValidator::new());
//! let auth_result = validator.validate(&token).await?;
//! ```
//!
//! Built-in validators:
//! - **JwtValidator**: Validates JWT tokens (implements `AuthValidator`)
//! - Custom: API keys, database lookup, OAuth, etc. (implement `AuthValidator`)
//!
//! # Architecture
//!
//! The tunnel system is composed of several focused crates:
//!
//! - **`tunnel-proto`**: Protocol definitions and message types
//! - **`tunnel-transport`**: Transport abstraction (QUIC, TCP, etc.)
//! - **`tunnel-client`**: Tunnel client library
//! - **`tunnel-control`**: Control plane for tunnel management
//! - **`tunnel-auth`**: Authentication and JWT handling
//! - **`tunnel-router`**: Routing logic (TCP port, SNI, HTTP host)
//! - **`tunnel-server-*`**: Protocol-specific servers (TCP, TLS, HTTP, HTTPS)
//! - **`tunnel-cert`**: Certificate management and ACME
//! - **`tunnel-relay-db`**: Database layer for traffic inspection
//!
//! All types from these crates are re-exported here for convenience.

// Re-export protocol types
pub use localup_proto::{
    Endpoint, HttpAuthConfig, Protocol, TunnelConfig as ProtoTunnelConfig, TunnelMessage,
};

// Re-export HTTP authentication types (for incoming request authentication)
pub use localup_http_auth::{
    AuthResult as HttpAuthResult, BasicAuthProvider, BearerTokenProvider, HeaderAuthProvider,
    HttpAuthProvider, HttpAuthenticator,
};

// Re-export transport layer
pub use localup_transport::{
    TransportConnection, TransportError, TransportListener, TransportStream,
};
pub use localup_transport_quic::{QuicConfig, QuicConnection, QuicConnector, QuicListener};

// Re-export client types (primary API for tunnel clients)
pub use localup_client::{
    BodyContent, BodyData, DbMetricsStore, ExitNodeConfig, HttpMetric, MetricsStats, MetricsStore,
    ProtocolConfig, Region, ReverseTunnelClient, ReverseTunnelConfig, ReverseTunnelError,
    TcpConnectionState, TcpMetric, TunnelClient, TunnelConfig, TunnelError,
};

// Re-export control plane types (for building custom relays)
pub use localup_control::{
    AgentRegistry, PendingRequests, PortAllocator, RegisteredAgent, TunnelConnectionManager,
    TunnelHandler,
};

// Re-export server types (for building custom relays/exit nodes)
pub use localup_server_https::{HttpsServer, HttpsServerConfig};
pub use localup_server_tcp::{TcpServer, TcpServerConfig, TcpServerError};
pub use localup_server_tcp_proxy::{TcpProxyServer, TcpProxyServerConfig};
pub use localup_server_tls::{TlsServer, TlsServerConfig};

// Re-export router types
pub use localup_router::{RouteKey, RouteRegistry, RouteTarget};

// Re-export auth types (for custom authentication)
pub use localup_auth::{
    async_trait, Algorithm, AuthError, AuthResult, AuthValidator, DecodingKey, EncodingKey,
    JwtClaims, JwtError, JwtValidator, Token, TokenError, TokenGenerator, Validation,
};

// Re-export certificate types
pub use localup_cert::{
    generate_self_signed_cert, generate_self_signed_cert_with_domains, Certificate,
    SelfSignedCertificate,
};

// Re-export domain provider types from control plane (where they're actually used)
pub use localup_control::{
    DomainContext, DomainProvider, DomainProviderError, RestrictedDomainProvider,
    SimpleCounterDomainProvider,
};

// Re-export database types (for traffic inspection)
#[cfg(feature = "db")]
pub use localup_relay_db::{
    entities::{captured_request, prelude::*},
    migrator::Migrator,
};

// High-level relay builder API
pub mod relay;
pub mod relay_config;
pub use relay::{
    generate_token, HttpsRelayBuilder, SimplePortAllocator, TcpRelayBuilder, TlsRelayBuilder,
    TransportConfigs,
};
pub use relay_config::{
    CertificateData, CertificateProvider, ConfigError, InMemoryTunnelStorage,
    SelfSignedCertificateProvider, TunnelRecord, TunnelStorage,
};

// Re-export protocol discovery types
pub use localup_proto::{ProtocolDiscoveryResponse, TransportEndpoint, TransportProtocol};

// Re-export additional transport types
pub use localup_transport_h2::{H2Config, H2Connector, H2Listener};
pub use localup_transport_websocket::{WebSocketConfig, WebSocketConnector, WebSocketListener};
