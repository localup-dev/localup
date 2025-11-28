//! High-level Relay Builder API with Type-Safe Protocol Builders
//!
//! Provides type-safe builders for each protocol (HTTPS, TCP, TLS) with compile-time
//! guarantee that only valid configurations are accepted.
//!
//! # Quick Start
//!
//! ```ignore
//! use localup_lib::{HttpsRelayBuilder, TcpRelayBuilder, TlsRelayBuilder};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // HTTPS relay
//!     let relay = HttpsRelayBuilder::new("127.0.0.1:443", "cert.pem", "key.pem")?
//!         .control_plane("127.0.0.1:4443")?
//!         .jwt_secret(b"my-secret")
//!         .build()?;
//!     relay.run().await?;
//!
//!     // TCP relay with dynamic port allocation
//!     let relay = TcpRelayBuilder::new()
//!         .control_plane("127.0.0.1:4443")?
//!         .tcp_port_range(10000, Some(20000))
//!         .jwt_secret(b"my-secret")
//!         .build()?;
//!     relay.run().await?;
//!
//!     // TLS relay
//!     let relay = TlsRelayBuilder::new("127.0.0.1:443")?
//!         .control_plane("127.0.0.1:4443")?
//!         .jwt_secret(b"my-secret")
//!         .build()?;
//!     relay.run().await?;
//!
//!     Ok(())
//! }
//! ```

use crate::{
    AgentRegistry, HttpsServer, HttpsServerConfig, JwtClaims, JwtValidator, PendingRequests,
    QuicConfig, QuicListener, RouteRegistry, TlsServer, TlsServerConfig, TransportListener,
    TunnelConnectionManager, TunnelHandler,
};
use chrono::Duration;
use localup_control::{PortAllocator, TcpProxySpawner};
use localup_proto::ProtocolDiscoveryResponse;
use localup_server_tcp_proxy::{TcpProxyServer, TcpProxyServerConfig};
use localup_transport_h2::{H2Config, H2Listener};
use localup_transport_websocket::{WebSocketConfig, WebSocketListener};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Relay builder errors
#[derive(Error, Debug)]
pub enum RelayBuilderError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Token generation error: {0}")]
    TokenError(String),
}

/// Generate a JWT token for tunnel client authentication
///
/// # Arguments
/// * `localup_id` - Unique identifier for the tunnel
/// * `secret` - Secret key used to sign the token (must match relay's jwt_secret)
/// * `hours_valid` - How many hours the token is valid for
///
/// # Example
/// ```ignore
/// let token = generate_token("my-tunnel", b"secret-key", 24)?;
/// // Use token in TunnelConfig
/// ```
pub fn generate_token(
    localup_id: &str,
    secret: &[u8],
    hours_valid: i64,
) -> Result<String, RelayBuilderError> {
    let claims = JwtClaims::new(
        localup_id.to_string(),
        "localup-relay".to_string(),
        "localup-client".to_string(),
        Duration::hours(hours_valid),
    );

    JwtValidator::encode(secret, &claims)
        .map_err(|e| RelayBuilderError::TokenError(format!("Failed to encode token: {}", e)))
}

/// Simple port allocator for TCP tunnels
/// Allocates ports starting from a configurable range and increments for each tunnel
pub struct SimplePortAllocator {
    allocations: Arc<Mutex<HashMap<String, u16>>>,
    next_port: Arc<Mutex<u16>>,
    max_port: Option<u16>,
}

impl SimplePortAllocator {
    /// Create a new allocator with a custom port range
    ///
    /// # Arguments
    /// * `start_port` - First port to allocate
    /// * `max_port` - Optional maximum port (inclusive). If None, no upper limit.
    pub fn with_range(start_port: u16, max_port: Option<u16>) -> Self {
        Self {
            allocations: Arc::new(Mutex::new(HashMap::new())),
            next_port: Arc::new(Mutex::new(start_port)),
            max_port,
        }
    }
}

impl PortAllocator for SimplePortAllocator {
    fn allocate(&self, localup_id: &str, requested_port: Option<u16>) -> Result<u16, String> {
        let mut allocations = self.allocations.lock().unwrap();

        // If already allocated for this tunnel, return existing port
        if let Some(&port) = allocations.get(localup_id) {
            return Ok(port);
        }

        // If a specific port was requested, use it
        if let Some(port) = requested_port {
            allocations.insert(localup_id.to_string(), port);
            return Ok(port);
        }

        // Otherwise, allocate the next available port
        let mut next_port = self.next_port.lock().unwrap();
        let port = *next_port;

        // Check if we've exceeded max port
        if let Some(max) = self.max_port {
            if port > max {
                return Err(format!(
                    "Port allocator exhausted: exceeded max port {}",
                    max
                ));
            }
        }

        *next_port = port.saturating_add(1);

        allocations.insert(localup_id.to_string(), port);
        Ok(port)
    }

    fn deallocate(&self, localup_id: &str) {
        if let Ok(mut allocations) = self.allocations.lock() {
            allocations.remove(localup_id);
        }
    }

    fn get_allocated_port(&self, localup_id: &str) -> Option<u16> {
        self.allocations.lock().unwrap().get(localup_id).copied()
    }
}

/// HTTPS server configuration
#[derive(Clone, Debug)]
struct HttpsConfig {
    bind_addr: String,
    cert_path: String,
    key_path: String,
}

/// TLS server configuration
#[derive(Clone, Debug)]
struct TlsConfig {
    bind_addr: String,
}

/// Control plane configuration
#[derive(Clone, Debug)]
struct ControlPlaneConfig {
    bind_addr: String,
    domain: String,
    /// Additional transport configurations
    transports: TransportConfigs,
}

/// Transport configurations for multi-protocol support
#[derive(Clone, Debug, Default)]
pub struct TransportConfigs {
    /// QUIC transport (enabled by default)
    pub quic_enabled: bool,
    pub quic_port: Option<u16>,
    /// WebSocket transport configuration
    pub websocket_enabled: bool,
    pub websocket_port: Option<u16>,
    pub websocket_path: String,
    /// HTTP/2 transport configuration
    pub h2_enabled: bool,
    pub h2_port: Option<u16>,
    /// TLS certificate and key for TCP-based transports (WebSocket, H2)
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
}

impl TransportConfigs {
    /// Create default transport config (QUIC only)
    pub fn quic_only() -> Self {
        Self {
            quic_enabled: true,
            quic_port: None,
            websocket_enabled: false,
            websocket_port: None,
            websocket_path: "/localup".to_string(),
            h2_enabled: false,
            h2_port: None,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }

    /// Enable all transports
    pub fn all() -> Self {
        Self {
            quic_enabled: true,
            quic_port: None,
            websocket_enabled: true,
            websocket_port: None,
            websocket_path: "/localup".to_string(),
            h2_enabled: true,
            h2_port: None,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }

    /// Build protocol discovery response from this config
    pub fn to_discovery_response(&self, base_port: u16) -> ProtocolDiscoveryResponse {
        let mut response = ProtocolDiscoveryResponse::default();

        if self.quic_enabled {
            let port = self.quic_port.unwrap_or(base_port);
            response = response.with_quic(port);
        }

        if self.websocket_enabled {
            let port = self.websocket_port.unwrap_or(base_port);
            response = response.with_websocket(port, &self.websocket_path);
        }

        if self.h2_enabled {
            let port = self.h2_port.unwrap_or(base_port);
            response = response.with_h2(port);
        }

        response
    }
}

/// Protocol marker types for type-safe builders
pub struct Https;
pub struct Tcp;
pub struct Tls;

/// Base relay builder shared by all protocol types
pub struct RelayBuilder<P> {
    protocol_config: Option<ProtocolSpecificConfig>,
    control_plane_config: Option<ControlPlaneConfig>,
    jwt_secret: Option<Vec<u8>>,
    domain: String,
    tcp_port_range_start: u16,
    tcp_port_range_end: Option<u16>,
    // Configurable trait implementations
    storage: Option<Arc<dyn crate::TunnelStorage>>,
    domain_provider: Option<Arc<dyn crate::DomainProvider>>,
    certificate_provider: Option<Arc<dyn crate::CertificateProvider>>,
    port_allocator: Option<Arc<dyn localup_control::PortAllocator>>,
    // Transport configurations
    transport_configs: TransportConfigs,
    _marker: std::marker::PhantomData<P>,
}

enum ProtocolSpecificConfig {
    Https(HttpsConfig),
    Tcp,
    Tls(TlsConfig),
}

// ============================================================================
// HTTPS Builder
// ============================================================================

impl RelayBuilder<Https> {
    /// Create a new HTTPS relay builder
    ///
    /// # Arguments
    /// * `bind_addr` - Address to bind HTTPS server to (e.g., "127.0.0.1:443")
    /// * `cert_path` - Path to TLS certificate file
    /// * `key_path` - Path to TLS private key file
    pub fn new(
        bind_addr: &str,
        cert_path: &str,
        key_path: &str,
    ) -> Result<Self, RelayBuilderError> {
        Ok(Self {
            protocol_config: Some(ProtocolSpecificConfig::Https(HttpsConfig {
                bind_addr: bind_addr.to_string(),
                cert_path: cert_path.to_string(),
                key_path: key_path.to_string(),
            })),
            control_plane_config: None,
            jwt_secret: None,
            domain: "localhost".to_string(),
            tcp_port_range_start: 9000,
            tcp_port_range_end: None,
            storage: None,
            domain_provider: None,
            certificate_provider: None,
            port_allocator: None,
            transport_configs: TransportConfigs::quic_only(),
            _marker: std::marker::PhantomData,
        })
    }

    /// Build the HTTPS relay
    pub fn build(self) -> Result<Relay, RelayBuilderError> {
        self.build_internal()
    }
}

// ============================================================================
// TCP Builder
// ============================================================================

impl RelayBuilder<Tcp> {
    /// Create a new TCP relay builder with dynamic port allocation
    ///
    /// TCP relay requires a control plane for dynamic port allocation.
    /// Each tunnel registers with the control plane and receives a dedicated port.
    pub fn new() -> Self {
        Self {
            protocol_config: Some(ProtocolSpecificConfig::Tcp),
            control_plane_config: None,
            jwt_secret: None,
            domain: "localhost".to_string(),
            tcp_port_range_start: 9000,
            tcp_port_range_end: None,
            storage: None,
            domain_provider: None,
            certificate_provider: None,
            port_allocator: None,
            transport_configs: TransportConfigs::quic_only(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Configure TCP port range for dynamic allocation
    ///
    /// # Arguments
    /// * `start_port` - First port to allocate (default: 9000)
    /// * `end_port` - Optional maximum port (inclusive). If None, no upper limit.
    ///
    /// # Example
    /// ```ignore
    /// TcpRelayBuilder::new()
    ///     .tcp_port_range(10000, Some(20000))  // Allocate ports 10000-20000
    ///     .build()?
    /// ```
    pub fn tcp_port_range(mut self, start_port: u16, end_port: Option<u16>) -> Self {
        self.tcp_port_range_start = start_port;
        self.tcp_port_range_end = end_port;
        self
    }

    /// Build the TCP relay
    pub fn build(self) -> Result<Relay, RelayBuilderError> {
        // TCP relay requires control plane for port allocation
        if self.control_plane_config.is_none() {
            return Err(RelayBuilderError::ConfigError(
                "TCP relay requires control plane to be configured".to_string(),
            ));
        }
        self.build_internal()
    }
}

impl Default for RelayBuilder<Tcp> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TLS Builder
// ============================================================================

impl RelayBuilder<Tls> {
    /// Create a new TLS/SNI relay builder
    ///
    /// # Arguments
    /// * `bind_addr` - Address to bind TLS server to (e.g., "127.0.0.1:443")
    ///
    /// Note: TLS server uses SNI-based passthrough routing. Backend services
    /// provide their own certificates.
    pub fn new(bind_addr: &str) -> Result<Self, RelayBuilderError> {
        Ok(Self {
            protocol_config: Some(ProtocolSpecificConfig::Tls(TlsConfig {
                bind_addr: bind_addr.to_string(),
            })),
            control_plane_config: None,
            jwt_secret: None,
            domain: "localhost".to_string(),
            tcp_port_range_start: 9000,
            tcp_port_range_end: None,
            storage: None,
            domain_provider: None,
            certificate_provider: None,
            port_allocator: None,
            transport_configs: TransportConfigs::quic_only(),
            _marker: std::marker::PhantomData,
        })
    }

    /// Build the TLS relay
    pub fn build(self) -> Result<Relay, RelayBuilderError> {
        self.build_internal()
    }
}

// ============================================================================
// Shared Methods for All Builders
// ============================================================================

impl<P> RelayBuilder<P> {
    /// Configure control plane (QUIC listener for tunnel clients)
    ///
    /// # Arguments
    /// * `bind_addr` - Address to bind control plane to (e.g., "0.0.0.0:4443")
    ///
    /// The control plane handles tunnel client registration, authentication,
    /// and route management using QUIC with auto-generated TLS certificates.
    pub fn control_plane(mut self, bind_addr: &str) -> Result<Self, RelayBuilderError> {
        self.control_plane_config = Some(ControlPlaneConfig {
            bind_addr: bind_addr.to_string(),
            domain: self.domain.clone(),
            transports: self.transport_configs.clone(),
        });
        Ok(self)
    }

    /// Configure transport protocols for the control plane
    ///
    /// By default, only QUIC is enabled. Use this to enable WebSocket and/or HTTP/2.
    pub fn transports(mut self, configs: TransportConfigs) -> Self {
        self.transport_configs = configs;
        if let Some(ref mut cp) = self.control_plane_config {
            cp.transports = self.transport_configs.clone();
        }
        self
    }

    /// Enable WebSocket transport on the control plane
    ///
    /// # Arguments
    /// * `port` - Optional port override (defaults to same as QUIC port)
    /// * `path` - WebSocket path (e.g., "/localup")
    pub fn with_websocket(mut self, port: Option<u16>, path: &str) -> Self {
        self.transport_configs.websocket_enabled = true;
        self.transport_configs.websocket_port = port;
        self.transport_configs.websocket_path = path.to_string();
        if let Some(ref mut cp) = self.control_plane_config {
            cp.transports = self.transport_configs.clone();
        }
        self
    }

    /// Enable HTTP/2 transport on the control plane
    ///
    /// # Arguments
    /// * `port` - Optional port override (defaults to same as QUIC port)
    pub fn with_h2(mut self, port: Option<u16>) -> Self {
        self.transport_configs.h2_enabled = true;
        self.transport_configs.h2_port = port;
        if let Some(ref mut cp) = self.control_plane_config {
            cp.transports = self.transport_configs.clone();
        }
        self
    }

    /// Set TLS certificate for TCP-based transports (WebSocket, HTTP/2)
    ///
    /// # Arguments
    /// * `cert_path` - Path to TLS certificate file
    /// * `key_path` - Path to TLS private key file
    pub fn transport_tls(mut self, cert_path: &str, key_path: &str) -> Self {
        self.transport_configs.tls_cert_path = Some(cert_path.to_string());
        self.transport_configs.tls_key_path = Some(key_path.to_string());
        if let Some(ref mut cp) = self.control_plane_config {
            cp.transports = self.transport_configs.clone();
        }
        self
    }

    /// Enable all transport protocols (QUIC, WebSocket, HTTP/2)
    pub fn with_all_transports(mut self) -> Self {
        self.transport_configs = TransportConfigs::all();
        if let Some(ref mut cp) = self.control_plane_config {
            cp.transports = self.transport_configs.clone();
        }
        self
    }

    /// Set the public domain for tunnel URLs
    ///
    /// This is used for generating tunnel URLs (e.g., "{subdomain}.{domain}")
    pub fn domain(mut self, domain: &str) -> Self {
        self.domain = domain.to_string();
        if let Some(ref mut cp) = self.control_plane_config {
            cp.domain = domain.to_string();
        }
        self
    }

    /// Set JWT secret for authentication
    ///
    /// If not set, a default secret will be generated
    pub fn jwt_secret(mut self, secret: &[u8]) -> Self {
        self.jwt_secret = Some(secret.to_vec());
        self
    }

    /// Configure custom tunnel storage implementation
    ///
    /// By default, tunnels are stored in-memory. Provide a custom implementation
    /// to persist to a database, files, etc.
    pub fn storage(mut self, storage: Arc<dyn crate::TunnelStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Configure custom domain provider for subdomain generation
    ///
    /// By default, subdomains are generated with a simple counter (tunnel-1, tunnel-2, etc.)
    /// Provide a custom implementation for memorable names, UUID-based, etc.
    pub fn domain_provider(mut self, provider: Arc<dyn crate::DomainProvider>) -> Self {
        self.domain_provider = Some(provider);
        self
    }

    /// Configure custom certificate provider
    ///
    /// By default, self-signed certificates are generated on demand.
    /// Provide a custom implementation for ACME/Let's Encrypt, cached certs, etc.
    pub fn certificate_provider(mut self, provider: Arc<dyn crate::CertificateProvider>) -> Self {
        self.certificate_provider = Some(provider);
        self
    }

    /// Configure custom port allocator for TCP tunnels
    ///
    /// By default, ports are allocated sequentially from a configurable range.
    /// Provide a custom implementation for random selection, reserved pools, etc.
    pub fn port_allocator(mut self, allocator: Arc<dyn localup_control::PortAllocator>) -> Self {
        self.port_allocator = Some(allocator);
        self
    }

    /// Internal build implementation shared by all protocols
    fn build_internal(self) -> Result<Relay, RelayBuilderError> {
        // Create shared infrastructure
        let route_registry = Arc::new(RouteRegistry::new());
        let tunnel_manager = Arc::new(TunnelConnectionManager::new());
        let pending_requests = Arc::new(PendingRequests::new());

        // Create JWT validator with default or provided secret
        let jwt_secret = self
            .jwt_secret
            .unwrap_or_else(|| b"default-secret".to_vec());
        // Only verify JWT signature using the secret - no issuer/audience checks
        let jwt_validator = Arc::new(JwtValidator::new(&jwt_secret));

        // Create trait implementations - use custom or defaults
        let _storage = self
            .storage
            .unwrap_or_else(|| Arc::new(crate::InMemoryTunnelStorage::new()));

        let domain_provider = self
            .domain_provider
            .unwrap_or_else(|| Arc::new(crate::SimpleCounterDomainProvider::new()));

        let _certificate_provider = self
            .certificate_provider
            .unwrap_or_else(|| Arc::new(crate::SelfSignedCertificateProvider));

        let port_allocator = self.port_allocator.unwrap_or_else(|| {
            Arc::new(SimplePortAllocator::with_range(
                self.tcp_port_range_start,
                self.tcp_port_range_end,
            ))
        });

        let mut https_server_handle = None;
        let mut tls_server_handle = None;

        // Configure protocol-specific server if provided
        if let Some(protocol_config) = &self.protocol_config {
            match protocol_config {
                ProtocolSpecificConfig::Https(https_cfg) => {
                    let config = HttpsServerConfig {
                        bind_addr: https_cfg.bind_addr.parse().map_err(|_| {
                            RelayBuilderError::ParseError(format!(
                                "Invalid HTTPS bind address: {}",
                                https_cfg.bind_addr
                            ))
                        })?,
                        cert_path: https_cfg.cert_path.clone(),
                        key_path: https_cfg.key_path.clone(),
                    };

                    let server = HttpsServer::new(config, route_registry.clone())
                        .with_localup_manager(tunnel_manager.clone())
                        .with_pending_requests(pending_requests.clone());

                    https_server_handle = Some(server);
                }
                ProtocolSpecificConfig::Tls(tls_cfg) => {
                    let config = TlsServerConfig {
                        bind_addr: tls_cfg.bind_addr.parse().map_err(|_| {
                            RelayBuilderError::ParseError(format!(
                                "Invalid TLS bind address: {}",
                                tls_cfg.bind_addr
                            ))
                        })?,
                    };

                    let server = TlsServer::new(config, route_registry.clone());

                    tls_server_handle = Some(server);
                }
                ProtocolSpecificConfig::Tcp => {
                    // TCP doesn't have a server here - it's spawned dynamically by control plane
                }
            }
        }

        // Create control plane handler if configured
        let control_plane_config = if let Some(cp_cfg) = &self.control_plane_config {
            let control_plane_addr: SocketAddr = cp_cfg.bind_addr.parse().map_err(|_| {
                RelayBuilderError::ParseError(format!(
                    "Invalid control plane bind address: {}",
                    cp_cfg.bind_addr
                ))
            })?;

            // Use the port allocator created above (either custom or default)

            // Create TCP proxy spawner that uses TcpProxyServer for raw TCP forwarding
            let localup_manager_for_spawner = tunnel_manager.clone();
            let tcp_proxy_spawner: TcpProxySpawner =
                Arc::new(move |localup_id: String, port: u16| {
                    let manager = localup_manager_for_spawner.clone();
                    let localup_id_clone = localup_id.clone();

                    Box::pin(async move {
                        let bind_addr: SocketAddr = format!("0.0.0.0:{}", port)
                            .parse()
                            .map_err(|e| format!("Invalid bind address: {}", e))?;

                        let config = TcpProxyServerConfig {
                            bind_addr,
                            localup_id: localup_id.clone(),
                        };

                        let proxy_server = TcpProxyServer::new(config, manager);

                        // Start the proxy server in a background task
                        tokio::spawn(async move {
                            if let Err(e) = proxy_server.start().await {
                                eprintln!(
                                    "TCP proxy server error for tunnel {}: {}",
                                    localup_id_clone, e
                                );
                            }
                        });

                        Ok(())
                    })
                });

            let handler = TunnelHandler::new(
                tunnel_manager.clone(),
                route_registry.clone(),
                Some(jwt_validator.clone()),
                cp_cfg.domain.clone(),
                pending_requests.clone(),
            )
            .with_agent_registry(Arc::new(AgentRegistry::new()))
            .with_port_allocator(port_allocator)
            .with_tcp_proxy_spawner(tcp_proxy_spawner)
            .with_domain_provider(domain_provider);

            let transport_configs = cp_cfg.transports.clone();
            Some((control_plane_addr, Arc::new(handler), transport_configs))
        } else {
            None
        };

        // Build protocol discovery response
        let protocol_discovery = self.control_plane_config.as_ref().map(|cp| {
            let base_port = cp
                .bind_addr
                .parse::<SocketAddr>()
                .map(|a| a.port())
                .unwrap_or(4443);
            cp.transports.to_discovery_response(base_port)
        });

        Ok(Relay {
            https_server: https_server_handle,
            tls_server: tls_server_handle,
            control_plane_config,
            route_registry,
            tunnel_manager,
            pending_requests,
            jwt_validator,
            protocol_discovery,
        })
    }
}

/// A configured and running tunnel relay
pub struct Relay {
    https_server: Option<HttpsServer>,
    tls_server: Option<TlsServer>,
    control_plane_config: Option<(SocketAddr, Arc<TunnelHandler>, TransportConfigs)>,
    pub route_registry: Arc<RouteRegistry>,
    pub tunnel_manager: Arc<TunnelConnectionManager>,
    pub pending_requests: Arc<PendingRequests>,
    pub jwt_validator: Arc<JwtValidator>,
    /// Protocol discovery response for this relay
    pub protocol_discovery: Option<ProtocolDiscoveryResponse>,
}

impl Relay {
    /// Get the route registry for manual route registration
    pub fn routes(&self) -> Arc<RouteRegistry> {
        self.route_registry.clone()
    }

    /// Get the tunnel manager
    pub fn tunnel_manager(&self) -> Arc<TunnelConnectionManager> {
        self.tunnel_manager.clone()
    }

    /// Get the JWT validator
    pub fn jwt_validator(&self) -> Arc<JwtValidator> {
        self.jwt_validator.clone()
    }

    /// Start all configured servers and wait for shutdown signal
    pub async fn run(self) -> Result<(), RelayBuilderError> {
        // Initialize Rustls crypto provider (required before using TLS/QUIC)
        let _ = rustls::crypto::ring::default_provider().install_default();

        // Use JoinSet for automatic task cancellation on shutdown
        let mut join_set = tokio::task::JoinSet::new();

        // Start HTTPS server if configured
        if let Some(https_server) = self.https_server {
            join_set.spawn(async move {
                if let Err(e) = https_server.start().await {
                    eprintln!("❌ HTTPS server error: {}", e);
                }
            });
        }

        // Start TLS server if configured
        if let Some(tls_server) = self.tls_server {
            join_set.spawn(async move {
                if let Err(e) = tls_server.start().await {
                    eprintln!("❌ TLS server error: {}", e);
                }
            });
        }

        // Start control plane listeners if configured
        if let Some((control_addr, handler, transport_configs)) = self.control_plane_config {
            let base_port = control_addr.port();

            // Start QUIC listener if enabled
            if transport_configs.quic_enabled {
                let quic_port = transport_configs.quic_port.unwrap_or(base_port);
                let quic_addr = SocketAddr::new(control_addr.ip(), quic_port);
                let handler_clone = handler.clone();

                join_set.spawn(async move {
                    println!("🔌 Starting QUIC control plane on {}", quic_addr);

                    match QuicConfig::server_self_signed() {
                        Ok(config) => {
                            let quic_config = Arc::new(config);
                            match QuicListener::new(quic_addr, quic_config) {
                                Ok(listener) => {
                                    println!(
                                        "✅ QUIC control plane listening on {} (TLS 1.3 encrypted)",
                                        quic_addr
                                    );
                                    loop {
                                        match listener.accept().await {
                                            Ok((connection, peer_addr)) => {
                                                println!(
                                                    "🔗 New QUIC tunnel connection from {}",
                                                    peer_addr
                                                );
                                                let h = handler_clone.clone();
                                                let conn = Arc::new(connection);
                                                tokio::spawn(async move {
                                                    h.handle_connection(conn, peer_addr).await;
                                                });
                                            }
                                            Err(e) => {
                                                if e.to_string().contains("endpoint closed") {
                                                    eprintln!("❌ QUIC endpoint closed");
                                                    break;
                                                }
                                                eprintln!("⚠️  QUIC accept error: {}", e);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("❌ Failed to create QUIC listener: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to create QUIC config: {}", e);
                        }
                    }
                });
            }

            // Start WebSocket listener if enabled
            if transport_configs.websocket_enabled {
                let ws_port = transport_configs.websocket_port.unwrap_or(base_port);
                let ws_addr = SocketAddr::new(control_addr.ip(), ws_port);
                let ws_path = transport_configs.websocket_path.clone();
                let cert_path = transport_configs.tls_cert_path.clone();
                let key_path = transport_configs.tls_key_path.clone();
                let handler_clone = handler.clone();

                join_set.spawn(async move {
                    println!(
                        "🔌 Starting WebSocket control plane on {} (path: {})",
                        ws_addr, ws_path
                    );

                    // Create config - use provided certs or self-signed
                    let config_result = match (cert_path, key_path) {
                        (Some(cert), Some(key)) => WebSocketConfig::server_default(&cert, &key),
                        _ => WebSocketConfig::server_self_signed(),
                    };

                    let config = match config_result {
                        Ok(mut c) => {
                            c.path = ws_path.clone();
                            Arc::new(c)
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to create WebSocket config: {}", e);
                            return;
                        }
                    };

                    match WebSocketListener::new(ws_addr, config) {
                        Ok(listener) => {
                            println!("✅ WebSocket control plane listening on {}", ws_addr);
                            loop {
                                match listener.accept().await {
                                    Ok((connection, peer_addr)) => {
                                        println!(
                                            "🔗 New WebSocket tunnel connection from {}",
                                            peer_addr
                                        );
                                        let h = handler_clone.clone();
                                        let conn = Arc::new(connection);
                                        tokio::spawn(async move {
                                            h.handle_connection(conn, peer_addr).await;
                                        });
                                    }
                                    Err(e) => {
                                        eprintln!("⚠️  WebSocket accept error: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to create WebSocket listener: {}", e);
                        }
                    }
                });
            }

            // Start HTTP/2 listener if enabled
            if transport_configs.h2_enabled {
                let h2_port = transport_configs.h2_port.unwrap_or(base_port);
                let h2_addr = SocketAddr::new(control_addr.ip(), h2_port);
                let cert_path = transport_configs.tls_cert_path.clone();
                let key_path = transport_configs.tls_key_path.clone();
                let handler_clone = handler.clone();

                join_set.spawn(async move {
                    println!("🔌 Starting HTTP/2 control plane on {}", h2_addr);

                    // Create config - use provided certs or self-signed
                    let config_result = match (cert_path, key_path) {
                        (Some(cert), Some(key)) => H2Config::server_default(&cert, &key),
                        _ => H2Config::server_self_signed(),
                    };

                    let config = match config_result {
                        Ok(c) => Arc::new(c),
                        Err(e) => {
                            eprintln!("❌ Failed to create HTTP/2 config: {}", e);
                            return;
                        }
                    };

                    match H2Listener::new(h2_addr, config) {
                        Ok(listener) => {
                            println!("✅ HTTP/2 control plane listening on {}", h2_addr);
                            loop {
                                match listener.accept().await {
                                    Ok((connection, peer_addr)) => {
                                        println!(
                                            "🔗 New HTTP/2 tunnel connection from {}",
                                            peer_addr
                                        );
                                        let h = handler_clone.clone();
                                        let conn = Arc::new(connection);
                                        tokio::spawn(async move {
                                            h.handle_connection(conn, peer_addr).await;
                                        });
                                    }
                                    Err(e) => {
                                        eprintln!("⚠️  HTTP/2 accept error: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to create HTTP/2 listener: {}", e);
                        }
                    }
                });
            }
        }

        // Wait for shutdown signal (SIGINT from Ctrl+C or SIGTERM from pkill/systemd)
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};

            let mut sigterm =
                signal(SignalKind::terminate()).map_err(RelayBuilderError::IoError)?;
            let mut sigint = signal(SignalKind::interrupt()).map_err(RelayBuilderError::IoError)?;

            tokio::select! {
                _ = sigterm.recv() => println!("📢 Received SIGTERM"),
                _ = sigint.recv() => println!("📢 Received SIGINT (Ctrl+C)"),
            }
        }

        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .map_err(|e| RelayBuilderError::IoError(e))?;
        }

        println!("\n✅ Shutting down relay...");

        // Cancel all spawned tasks
        join_set.abort_all();

        // Wait for all tasks to finish (they'll be cancelled)
        while join_set.join_next().await.is_some() {
            // Tasks are being cancelled
        }

        println!("✅ Relay stopped");
        Ok(())
    }
}

// Type aliases for convenience
/// HTTPS relay builder
pub type HttpsRelayBuilder = RelayBuilder<Https>;

/// TCP relay builder
pub type TcpRelayBuilder = RelayBuilder<Tcp>;

/// TLS relay builder
pub type TlsRelayBuilder = RelayBuilder<Tls>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_https_relay_builder() {
        let result =
            HttpsRelayBuilder::new("127.0.0.1:443", "cert.pem", "key.pem").and_then(|b| b.build());

        assert!(result.is_ok());
    }

    #[test]
    fn test_tcp_relay_builder_requires_control_plane() {
        let result = TcpRelayBuilder::new().build();

        // TCP relay requires control plane
        assert!(result.is_err());
    }

    #[test]
    fn test_tcp_relay_builder_with_control_plane() {
        let result = TcpRelayBuilder::new()
            .tcp_port_range(10000, Some(20000))
            .control_plane("127.0.0.1:4443")
            .and_then(|b| b.build());

        assert!(result.is_ok());
    }

    #[test]
    fn test_tls_relay_builder() {
        let result = TlsRelayBuilder::new("127.0.0.1:443").and_then(|b| b.build());

        assert!(result.is_ok());
    }

    #[test]
    fn test_https_relay_with_control_plane() {
        let result = HttpsRelayBuilder::new("127.0.0.1:443", "cert.pem", "key.pem")
            .and_then(|b| b.control_plane("127.0.0.1:4443"))
            .and_then(|b| b.build());

        assert!(result.is_ok());
    }

    #[test]
    fn test_tcp_relay_with_domain() {
        let result = TcpRelayBuilder::new()
            .control_plane("127.0.0.1:4443")
            .map(|b| b.domain("example.com"))
            .and_then(|b| b.build());

        assert!(result.is_ok());
    }

    #[test]
    fn test_https_relay_with_jwt_secret() {
        let secret = b"my-secret";
        let result = HttpsRelayBuilder::new("127.0.0.1:443", "cert.pem", "key.pem")
            .map(|b| b.jwt_secret(secret).build())
            .and_then(|r| r);

        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_https_bind_addr() {
        let result = HttpsRelayBuilder::new("invalid-address", "cert.pem", "key.pem")
            .and_then(|b| b.build());

        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_control_plane_addr() {
        let result = HttpsRelayBuilder::new("127.0.0.1:443", "cert.pem", "key.pem")
            .and_then(|b| b.control_plane("invalid-address"))
            .and_then(|b| b.build());

        assert!(result.is_err());
    }
}
