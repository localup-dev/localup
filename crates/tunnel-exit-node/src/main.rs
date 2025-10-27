//! Tunnel exit node (relay server)
//!
//! This binary creates a public-facing server that receives incoming connections
//! and routes them through established tunnels to local services.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tracing::{debug, error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use tunnel_auth::{JwtClaims, JwtValidator};
use tunnel_control::{PortAllocator as PortAllocatorTrait, TunnelConnectionManager, TunnelHandler};
use tunnel_router::RouteRegistry;
use tunnel_server_https::{HttpsServer, HttpsServerConfig};
use tunnel_server_tcp::{TcpServer, TcpServerConfig};
use tunnel_transport_quic::QuicConfig;

/// Tunnel exit node - accepts public connections and routes to tunnels
#[derive(Parser, Debug)]
#[command(name = "tunnel-exit-node")]
#[command(about = "Run a tunnel relay (exit node) server", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    server_args: ServerArgs,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate a JWT token for client authentication
    GenerateToken {
        /// JWT secret (must match the exit node's --jwt-secret)
        #[arg(long, env = "TUNNEL_JWT_SECRET")]
        secret: String,

        /// Tunnel ID (optional, defaults to "client")
        #[arg(long, default_value = "client")]
        tunnel_id: String,

        /// Token validity in hours (default: 24)
        #[arg(long, default_value = "24")]
        hours: i64,
    },
}

#[derive(Parser, Debug)]
struct ServerArgs {
    /// HTTP server bind address
    #[arg(long, default_value = "0.0.0.0:8080")]
    http_addr: String,

    /// Tunnel control port for client connections (QUIC by default)
    #[arg(long, default_value = "0.0.0.0:4443")]
    tunnel_addr: String,

    /// HTTPS server bind address (requires TLS certificates)
    #[arg(long)]
    https_addr: Option<String>,

    /// TLS certificate file path (PEM format, for HTTPS server and custom QUIC certs)
    /// If not specified for QUIC, a self-signed certificate is auto-generated
    #[arg(long)]
    tls_cert: Option<String>,

    /// TLS private key file path (PEM format, for HTTPS server and custom QUIC certs)
    /// If not specified for QUIC, a self-signed key is auto-generated
    #[arg(long)]
    tls_key: Option<String>,

    /// Public domain name for this relay (e.g., "relay.example.com" or "localhost" for testing)
    /// Subdomains will be constructed as: {subdomain}.{domain}
    #[arg(long, default_value = "localhost")]
    domain: String,

    /// JWT secret for authenticating tunnel clients
    /// Can also be set via TUNNEL_JWT_SECRET environment variable
    #[arg(long, env)]
    jwt_secret: Option<String>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// TCP port range for raw TCP tunnels (format: "10000-20000")
    #[arg(long)]
    tcp_port_range: Option<String>,

    /// API server bind address (for dashboard/management UI)
    #[arg(long, default_value = "127.0.0.1:3080")]
    api_addr: String,

    /// Disable API server
    #[arg(long)]
    no_api: bool,

    /// Database URL for request storage and traffic inspection
    /// PostgreSQL: "postgres://user:pass@localhost/tunnel_db"
    /// SQLite: "sqlite://./tunnel.db?mode=rwc"
    /// In-memory SQLite: "sqlite::memory:"
    /// If not provided, defaults to in-memory SQLite (data lost on restart)
    #[arg(long, env = "DATABASE_URL", default_value = "sqlite::memory:")]
    database_url: String,

    /// Use insecure plain TCP instead of encrypted QUIC for tunnel control
    /// WARNING: This disables encryption! Only use for debugging or legacy clients.
    /// By default, QUIC with TLS 1.3 encryption is used (zero-config with auto-generated certs).
    #[arg(long)]
    insecure: bool,
}

fn generate_token(secret: &str, tunnel_id: &str, hours: i64) -> Result<()> {
    use chrono::Duration;

    // Create JWT claims
    let claims = JwtClaims::new(
        tunnel_id.to_string(),
        "tunnel-client".to_string(),
        "tunnel-exit-node".to_string(),
        Duration::hours(hours),
    );

    // Encode the token
    let token = JwtValidator::encode(secret.as_bytes(), &claims)
        .map_err(|e| anyhow::anyhow!("Failed to generate token: {}", e))?;

    // Print success message
    println!("\n✅ JWT Token generated successfully!\n");
    println!("Tunnel ID:  {}", tunnel_id);
    println!("Valid for:  {} hours", hours);
    println!("Expires:    {}", claims.exp_formatted());
    println!("\n{}", "=".repeat(70));
    println!("TOKEN:");
    println!("{}", "=".repeat(70));
    println!("{}", token);
    println!("{}\n", "=".repeat(70));

    println!("Usage:");
    println!("  # Set as environment variable");
    println!("  export TUNNEL_AUTH_TOKEN=\"{}\"", token);
    println!();
    println!("  # Then connect with tunnel CLI");
    println!("  tunnel --port 3000 --subdomain myapp --relay localhost:4443");
    println!();
    println!("  # Or pass token directly");
    println!(
        "  tunnel --port 3000 --subdomain myapp --relay localhost:4443 --token \"{}\"",
        token
    );
    println!();

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize rustls crypto provider (required for QUIC/TLS)
    rustls::crypto::CryptoProvider::install_default(rustls::crypto::ring::default_provider())
        .unwrap();

    let cli = Cli::parse();

    // Handle subcommands
    if let Some(command) = cli.command {
        return match command {
            Commands::GenerateToken {
                secret,
                tunnel_id,
                hours,
            } => generate_token(&secret, &tunnel_id, hours),
        };
    }

    // Otherwise, run the server
    let args = cli.server_args;

    // Initialize logging
    init_logging(&args.log_level)?;

    info!("🚀 Starting tunnel exit node");
    info!("HTTP endpoint: {}", args.http_addr);
    info!("Tunnel control: {}", args.tunnel_addr);
    info!("Public domain: {}", args.domain);
    info!("Subdomains will be: {{name}}.{}", args.domain);

    if let Some(ref https_addr) = args.https_addr {
        info!("HTTPS endpoint: {}", https_addr);
    }

    // Initialize database connection
    info!("Connecting to database: {}", args.database_url);
    let db = tunnel_relay_db::connect(&args.database_url).await?;

    // Run migrations
    tunnel_relay_db::migrate(&db)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run database migrations: {}", e))?;

    // Initialize TCP port allocator if TCP range provided
    let port_allocator = if let Some(ref tcp_range) = args.tcp_port_range {
        let (start, end) = parse_port_range(tcp_range)?;
        info!(
            "TCP port range: {}-{} ({} ports available)",
            start,
            end,
            end - start + 1
        );
        Some(Arc::new(PortAllocator::new(start, end)))
    } else {
        None
    };

    // Start cleanup task for expired port reservations
    if let Some(ref allocator) = port_allocator {
        let allocator_clone = allocator.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60)); // Check every minute
            loop {
                interval.tick().await;
                allocator_clone.cleanup_expired();
            }
        });
        info!("✅ Port reservation cleanup task started (checks every 60s)");
    }

    // Create shared route registry
    let registry = Arc::new(RouteRegistry::new());
    info!("✅ Route registry initialized");
    info!("Routes will be registered automatically when tunnels connect");

    // Create JWT validator for tunnel authentication
    let jwt_validator = if let Some(jwt_secret) = args.jwt_secret {
        let validator = Arc::new(
            JwtValidator::new(jwt_secret.as_bytes())
                .with_audience("tunnel-exit-node".to_string())
                .with_issuer("tunnel-client".to_string()),
        );
        info!("✅ JWT authentication enabled");
        Some(validator)
    } else {
        info!("⚠️  Running without JWT authentication (not recommended for production)");
        None
    };

    // Create tunnel connection manager
    let tunnel_manager = Arc::new(TunnelConnectionManager::new());

    // Create pending requests tracker (shared between HTTP server and tunnel handler)
    let pending_requests = Arc::new(tunnel_control::PendingRequests::new());

    // Start HTTP server with tunnel manager and pending requests
    let http_addr: SocketAddr = args.http_addr.parse()?;
    let http_config = TcpServerConfig {
        bind_addr: http_addr,
    };
    let http_server = TcpServer::new(http_config, registry.clone())
        .with_tunnel_manager(tunnel_manager.clone())
        .with_pending_requests(pending_requests.clone())
        .with_database(db.clone());

    let http_handle = tokio::spawn(async move {
        info!("Starting HTTP relay server");
        if let Err(e) = http_server.start().await {
            error!("HTTP server error: {}", e);
        }
    });

    // Start HTTPS server if configured
    let https_handle = if let Some(ref https_addr) = args.https_addr {
        // HTTPS requires cert/key files
        let cert_path = args
            .tls_cert
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("HTTPS server requires --tls-cert"))?;
        let key_path = args
            .tls_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("HTTPS server requires --tls-key"))?;

        let https_addr: SocketAddr = https_addr.parse()?;
        let https_config = HttpsServerConfig {
            bind_addr: https_addr,
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
        };

        let https_server = HttpsServer::new(https_config, registry.clone())
            .with_tunnel_manager(tunnel_manager.clone())
            .with_pending_requests(pending_requests.clone());

        Some(tokio::spawn(async move {
            info!("Starting HTTPS relay server");
            if let Err(e) = https_server.start().await {
                error!("HTTPS server error: {}", e);
            }
        }))
    } else {
        None
    };

    // Start tunnel listener (QUIC by default, TCP if --insecure)
    info!(
        "🔧 Attempting to bind tunnel control to {}",
        args.tunnel_addr
    );

    let use_quic = !args.insecure;

    let mut tunnel_handler = TunnelHandler::new(
        tunnel_manager.clone(),
        registry.clone(),
        jwt_validator.clone(),
        args.domain.clone(),
        pending_requests.clone(),
    );

    // Add port allocator if TCP range was provided
    if let Some(ref allocator) = port_allocator {
        tunnel_handler = tunnel_handler
            .with_port_allocator(allocator.clone() as Arc<dyn tunnel_control::PortAllocator>);
        info!("✅ TCP port allocator configured");

        // Add TCP proxy spawner
        let tunnel_manager_for_spawner = tunnel_manager.clone();
        let db_for_spawner = db.clone();
        let spawner: tunnel_control::TcpProxySpawner =
            Arc::new(move |tunnel_id: String, port: u16| {
                let manager = tunnel_manager_for_spawner.clone();
                let tunnel_id_clone = tunnel_id.clone();
                let db_clone = db_for_spawner.clone();

                Box::pin(async move {
                    use std::net::SocketAddr;
                    use tunnel_server_tcp_proxy::{TcpProxyServer, TcpProxyServerConfig};

                    let bind_addr: SocketAddr = format!("0.0.0.0:{}", port)
                        .parse()
                        .map_err(|e| format!("Invalid bind address: {}", e))?;

                    let config = TcpProxyServerConfig {
                        bind_addr,
                        tunnel_id: tunnel_id.clone(),
                    };

                    let proxy_server =
                        TcpProxyServer::new(config, manager.clone()).with_database(db_clone);

                    // Note: No callback needed - TCP proxy opens new QUIC streams directly

                    // Start the proxy server in a background task
                    tokio::spawn(async move {
                        if let Err(e) = proxy_server.start().await {
                            error!(
                                "TCP proxy server error for tunnel {}: {}",
                                tunnel_id_clone, e
                            );
                        }
                    });

                    Ok(())
                })
            });

        tunnel_handler = tunnel_handler.with_tcp_proxy_spawner(spawner);
        info!("✅ TCP proxy spawner configured");
    }

    let tunnel_handler = Arc::new(tunnel_handler);

    // TODO: Re-enable API server after fixing tunnel-api
    let api_handle: Option<tokio::task::JoinHandle<()>> = None;
    info!("API server temporarily disabled");
    //     // Start API server for dashboard/management
    //     let api_handle = if !args.no_api {
    //         let api_addr: SocketAddr = args.api_addr.parse()?;
    //         let api_tunnel_manager = tunnel_manager.clone();
    //         let api_db = db.clone();
    //
    //         info!("Starting API server on {}", api_addr);
    //         info!("OpenAPI spec: http://{}/api/openapi.json", api_addr);
    //         info!("Swagger UI: http://{}/swagger-ui", api_addr);
    //
    //         Some(tokio::spawn(async move {
    //             // use tunnel_api::{ApiServer, ApiServerConfig};
    //
    //             let config = ApiServerConfig {
    //                 bind_addr: api_addr,
    //                 enable_cors: true,
    //                 cors_origins: Some(vec![
    //                     "http://localhost:3000".to_string(),
    //                     "http://127.0.0.1:3000".to_string(),
    //                 ]),
    //             };
    //
    //             let server = ApiServer::new(config, api_tunnel_manager, api_db);
    //             if let Err(e) = server.start().await {
    //                 error!("API server error: {}", e);
    //             }
    //         }))
    //     } else {
    //         info!("API server disabled (--no-api flag)");
    //         None
    //     };

    // Accept tunnel connections
    let tunnel_handle = if use_quic {
        // QUIC mode
        use tunnel_transport::TransportListener;
        use tunnel_transport_quic::QuicListener;

        let quic_config = if let (Some(cert), Some(key)) = (&args.tls_cert, &args.tls_key) {
            info!("🔐 Using custom TLS certificates for QUIC");
            Arc::new(QuicConfig::server_default(cert, key)?)
        } else {
            info!("🔐 Generating ephemeral self-signed certificate for QUIC...");
            let config = Arc::new(QuicConfig::server_self_signed()?);
            info!("✅ Self-signed certificate generated (valid for 90 days)");
            config
        };

        let tunnel_addr: std::net::SocketAddr = args.tunnel_addr.parse()?;
        let quic_listener = QuicListener::new(tunnel_addr, quic_config)?;

        info!(
            "🔌 Tunnel control listening on {} (QUIC with TLS 1.3)",
            args.tunnel_addr
        );
        info!("🔐 All tunnel traffic is encrypted end-to-end");

        tokio::spawn(async move {
            info!("🎯 QUIC accept loop started, waiting for connections...");
            loop {
                debug!("Waiting for next QUIC connection...");
                match quic_listener.accept().await {
                    Ok((connection, peer_addr)) => {
                        info!("🔗 New tunnel connection from {}", peer_addr);
                        let handler = tunnel_handler.clone();
                        let conn = Arc::new(connection);
                        tokio::spawn(async move {
                            handler.handle_connection(conn, peer_addr).await;
                        });
                    }
                    Err(e) => {
                        error!("❌ Failed to accept QUIC connection: {}", e);
                        // Log additional details for debugging
                        error!("   Error details: {:?}", e);
                        // If endpoint is closed, break the loop
                        if e.to_string().contains("endpoint closed")
                            || e.to_string().contains("Endpoint closed")
                        {
                            error!("🛑 QUIC endpoint closed, stopping accept loop");
                            break;
                        }
                    }
                }
            }
            error!("⚠️  QUIC accept loop exited unexpectedly!");
        })
    } else {
        // Insecure TCP mode (for debugging only)
        warn!("⚠️  INSECURE MODE: Using plain TCP without encryption");
        warn!("⚠️  This mode is for debugging only - NOT for production!");

        // We need a TCP-to-Transport adapter since TunnelHandler now expects TransportConnection
        // For now, just log a message that this isn't supported yet
        error!("❌ TCP mode (--insecure) is not currently supported with the new transport abstraction");
        error!("💡 Remove the --insecure flag to use QUIC with zero-config encryption");
        std::process::exit(1);
    };

    info!("✅ Tunnel exit node is running");
    info!("Ready to accept incoming connections");
    info!("  - HTTP traffic: {}", args.http_addr);
    if let Some(ref https_addr) = args.https_addr {
        info!("  - HTTPS traffic: {}", https_addr);
    }
    info!("  - Tunnel control: {}", args.tunnel_addr);
    if !args.no_api {
        info!(
            "  - API/Dashboard: {} (OpenAPI at /api/openapi.json)",
            args.api_addr
        );
    }
    info!("Press Ctrl+C to stop");

    // Wait for shutdown signal
    match signal::ctrl_c().await {
        Ok(()) => {
            info!("Shutdown signal received, stopping servers...");
        }
        Err(err) => {
            error!("Error listening for shutdown signal: {}", err);
        }
    }

    // Graceful shutdown
    http_handle.abort();
    if let Some(handle) = https_handle {
        handle.abort();
    }
    if let Some(handle) = api_handle {
        handle.abort();
    }
    tunnel_handle.abort();
    info!("✅ Tunnel exit node stopped");

    Ok(())
}

fn init_logging(log_level: &str) -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new(log_level))?;

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    Ok(())
}

use chrono::{DateTime, Utc};
/// TCP Proxy Manager and Port Allocator with reconnection support
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Allocation state for a port
#[derive(Debug, Clone)]
struct PortAllocation {
    port: u16,
    state: AllocationState,
}

#[derive(Debug, Clone)]
enum AllocationState {
    Active,
    Reserved { until: DateTime<Utc> },
}

pub struct PortAllocator {
    range_start: u16,
    range_end: u16,
    available_ports: Mutex<HashSet<u16>>,
    allocated_ports: Mutex<HashMap<String, PortAllocation>>, // tunnel_id -> allocation
    reservation_ttl_seconds: i64,
}

impl PortAllocator {
    pub fn new(range_start: u16, range_end: u16) -> Self {
        Self::with_reservation_ttl(range_start, range_end, 300) // Default 5 minute reservation
    }

    pub fn with_reservation_ttl(range_start: u16, range_end: u16, ttl_seconds: i64) -> Self {
        let mut available = HashSet::new();
        for port in range_start..=range_end {
            available.insert(port);
        }

        Self {
            range_start,
            range_end,
            available_ports: Mutex::new(available),
            allocated_ports: Mutex::new(HashMap::new()),
            reservation_ttl_seconds: ttl_seconds,
        }
    }

    /// Check if a port is actually available at the OS level
    fn is_port_available(port: u16) -> bool {
        use std::net::{SocketAddr, TcpListener};

        // Try to bind to 0.0.0.0:port
        let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
        TcpListener::bind(addr).is_ok()
    }

    /// Generate a deterministic port number from tunnel_id hash
    /// This ensures the same tunnel_id always gets the same port (if available)
    fn hash_to_port(&self, tunnel_id: &str) -> u16 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        tunnel_id.hash(&mut hasher);
        let hash = hasher.finish();

        let range_size = (self.range_end - self.range_start + 1) as u64;
        let port_offset = (hash % range_size) as u16;
        self.range_start + port_offset
    }

    /// Clean up expired reservations (should be called periodically)
    pub fn cleanup_expired(&self) {
        let mut available = self.available_ports.lock().unwrap();
        let mut allocated = self.allocated_ports.lock().unwrap();
        let now = Utc::now();

        let expired: Vec<String> = allocated
            .iter()
            .filter_map(|(tunnel_id, allocation)| match &allocation.state {
                AllocationState::Reserved { until } if *until < now => Some(tunnel_id.clone()),
                _ => None,
            })
            .collect();

        for tunnel_id in expired {
            if let Some(allocation) = allocated.remove(&tunnel_id) {
                available.insert(allocation.port);
                info!(
                    "Cleaned up expired port reservation for tunnel {} (port {})",
                    tunnel_id, allocation.port
                );
            }
        }
    }
}

impl PortAllocatorTrait for PortAllocator {
    fn allocate(&self, tunnel_id: &str) -> Result<u16, String> {
        let mut available = self.available_ports.lock().unwrap();
        let mut allocated = self.allocated_ports.lock().unwrap();

        // Check if already allocated (active or reserved)
        if let Some(allocation) = allocated.get(tunnel_id) {
            let port = allocation.port;
            // Reactivate if it was reserved
            if matches!(allocation.state, AllocationState::Reserved { .. }) {
                info!(
                    "Reusing reserved port {} for reconnecting tunnel {}",
                    port, tunnel_id
                );
                allocated.insert(
                    tunnel_id.to_string(),
                    PortAllocation {
                        port,
                        state: AllocationState::Active,
                    },
                );
            }
            return Ok(port);
        }

        // Try to allocate deterministic port based on tunnel_id hash
        let preferred_port = self.hash_to_port(tunnel_id);

        if available.contains(&preferred_port) && Self::is_port_available(preferred_port) {
            // Preferred port is available in our tracking AND at OS level!
            available.remove(&preferred_port);
            allocated.insert(
                tunnel_id.to_string(),
                PortAllocation {
                    port: preferred_port,
                    state: AllocationState::Active,
                },
            );
            info!(
                "🎯 Allocated deterministic port {} for tunnel {} (hash-based)",
                preferred_port, tunnel_id
            );
            return Ok(preferred_port);
        } else if available.contains(&preferred_port) && !Self::is_port_available(preferred_port) {
            // Port was in our available set but is actually in use - remove it from tracking
            warn!("Port {} was marked available but is in use by another process, removing from available pool", preferred_port);
            available.remove(&preferred_port);
        }

        // Preferred port not available, try nearby ports (within ±10 range)
        for offset in 1..=10 {
            for &port in &[
                preferred_port.saturating_add(offset),
                preferred_port.saturating_sub(offset),
            ] {
                if port >= self.range_start && port <= self.range_end && available.contains(&port) {
                    // Verify port is actually available at OS level
                    if Self::is_port_available(port) {
                        available.remove(&port);
                        allocated.insert(
                            tunnel_id.to_string(),
                            PortAllocation {
                                port,
                                state: AllocationState::Active,
                            },
                        );
                        info!(
                            "Allocated nearby port {} for tunnel {} (preferred {} was taken)",
                            port, tunnel_id, preferred_port
                        );
                        return Ok(port);
                    } else {
                        // Port in use by another process, remove from available pool
                        warn!(
                            "Port {} was marked available but is in use, removing from pool",
                            port
                        );
                        available.remove(&port);
                    }
                }
            }
        }

        // Fallback: allocate any available port, checking OS-level availability
        let available_ports: Vec<u16> = available.iter().copied().collect();
        for &port in &available_ports {
            if Self::is_port_available(port) {
                available.remove(&port);
                allocated.insert(
                    tunnel_id.to_string(),
                    PortAllocation {
                        port,
                        state: AllocationState::Active,
                    },
                );
                info!(
                    "Allocated fallback port {} for tunnel {} (preferred {} was taken)",
                    port, tunnel_id, preferred_port
                );
                return Ok(port);
            } else {
                // Port in use by another process, remove from available pool
                warn!(
                    "Port {} was marked available but is in use, removing from pool",
                    port
                );
                available.remove(&port);
            }
        }

        Err("No available ports in range (all ports in use)".to_string())
    }

    fn deallocate(&self, tunnel_id: &str) {
        let mut allocated = self.allocated_ports.lock().unwrap();

        // Instead of immediately freeing, mark as reserved for reconnection
        if let Some(allocation) = allocated.get_mut(tunnel_id) {
            if matches!(allocation.state, AllocationState::Active) {
                let until = Utc::now() + chrono::Duration::seconds(self.reservation_ttl_seconds);
                allocation.state = AllocationState::Reserved { until };
                info!(
                    "Port {} for tunnel {} marked as reserved until {} (TTL: {}s)",
                    allocation.port,
                    tunnel_id,
                    until.format("%Y-%m-%d %H:%M:%S"),
                    self.reservation_ttl_seconds
                );
            }
        }
    }

    fn get_allocated_port(&self, tunnel_id: &str) -> Option<u16> {
        self.allocated_ports
            .lock()
            .unwrap()
            .get(tunnel_id)
            .map(|alloc| alloc.port)
    }
}

fn parse_port_range(range_str: &str) -> Result<(u16, u16)> {
    let parts: Vec<&str> = range_str.split('-').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!(
            "Invalid port range format. Expected: START-END (e.g., 10000-20000)"
        ));
    }

    let start: u16 = parts[0]
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid start port: {}", parts[0]))?;
    let end: u16 = parts[1]
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid end port: {}", parts[1]))?;

    if start >= end {
        return Err(anyhow::anyhow!("Start port must be less than end port"));
    }

    Ok((start, end))
}
