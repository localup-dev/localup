//! Tunnel CLI - Command-line interface for creating tunnels

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use localup_cli::{config, daemon, localup_store, service};
use localup_client::{
    ExitNodeConfig, MetricsServer, ProtocolConfig, ReverseTunnelClient, ReverseTunnelConfig,
    TunnelClient, TunnelConfig,
};

/// Tunnel CLI - Expose local servers to the internet
#[derive(Parser, Debug)]
#[command(name = "localup")]
#[command(about = "Expose local servers through secure tunnels", long_about = None)]
#[command(version = env!("GIT_TAG"))]
#[command(long_version = concat!(env!("GIT_TAG"), "\nCommit: ", env!("GIT_HASH"), "\nBuilt: ", env!("BUILD_TIME")))]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Local port to expose (standalone mode only)
    #[arg(short, long)]
    port: Option<u16>,

    /// Local address to expose (host:port format) (standalone mode only)
    /// Alternative to --port. Use this to bind to a specific address
    #[arg(long)]
    address: Option<String>,

    /// Protocol to use (http, https, tcp, tls) (standalone mode only)
    #[arg(long)]
    protocol: Option<String>,

    /// Authentication token / JWT secret (standalone mode only)
    #[arg(short, long, env = "TUNNEL_AUTH_TOKEN")]
    token: Option<String>,

    /// Subdomain for HTTP/HTTPS tunnels (standalone mode only)
    #[arg(short, long)]
    subdomain: Option<String>,

    /// Relay server address (standalone mode only)
    #[arg(short, long, env)]
    relay: Option<String>,

    /// Remote port for TCP/TLS tunnels (standalone mode only)
    #[arg(long)]
    remote_port: Option<u16>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Port for metrics web dashboard (standalone mode only)
    #[arg(long, default_value = "9090")]
    metrics_port: u16,

    /// Disable metrics collection and web dashboard (standalone mode only)
    #[arg(long)]
    no_metrics: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Add a new tunnel configuration
    Add {
        /// Tunnel name
        name: String,
        /// Local port to expose
        #[arg(short, long)]
        port: Option<u16>,
        /// Local address to expose (host:port format) - alternative to --port
        #[arg(long)]
        address: Option<String>,
        /// Protocol (http, https, tcp, tls)
        #[arg(long, default_value = "http")]
        protocol: String,
        /// Authentication token (optional if relay has no auth)
        #[arg(short, long, env = "TUNNEL_AUTH_TOKEN")]
        token: Option<String>,
        /// Subdomain for HTTP/HTTPS/TLS tunnels
        #[arg(short, long)]
        subdomain: Option<String>,
        /// Custom domain for HTTPS tunnels
        #[arg(long)]
        domain: Option<String>,
        /// Relay server address (host:port)
        #[arg(short, long)]
        relay: Option<String>,
        /// Remote port for TCP/TLS tunnels
        #[arg(long)]
        remote_port: Option<u16>,
        /// Auto-enable (start with daemon)
        #[arg(long)]
        enabled: bool,
    },
    /// List all tunnel configurations
    List,
    /// Show tunnel details
    Show {
        /// Tunnel name
        name: String,
    },
    /// Remove a tunnel configuration
    Remove {
        /// Tunnel name
        name: String,
    },
    /// Enable auto-start with daemon
    Enable {
        /// Tunnel name
        name: String,
    },
    /// Disable auto-start with daemon
    Disable {
        /// Tunnel name
        name: String,
    },
    /// Manage daemon (multi-tunnel mode)
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
    /// Manage system service (background daemon)
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },
    /// Connect to a reverse tunnel (access service behind agent)
    Connect {
        /// Relay server address (e.g., relay.example.com:4443)
        #[arg(long, env = "LOCALUP_RELAY")]
        relay: String,

        /// Remote address to connect to (e.g., "192.168.1.100:8080")
        #[arg(long)]
        remote_address: String,

        /// Agent ID to route through
        #[arg(long)]
        agent_id: String,

        /// Local address to bind to (default: localhost:0)
        #[arg(long)]
        local_address: Option<String>,

        /// Authentication token for relay (JWT)
        #[arg(long, env = "LOCALUP_AUTH_TOKEN")]
        token: Option<String>,

        /// Authentication token for agent server (JWT)
        #[arg(long, env = "LOCALUP_AGENT_TOKEN")]
        agent_token: Option<String>,

        /// Skip TLS certificate verification (INSECURE - dev only)
        #[arg(long)]
        insecure: bool,
    },
    /// Run as reverse tunnel agent (forwards traffic to a specific address)
    Agent {
        /// Relay server address (host:port)
        #[arg(long, env = "LOCALUP_RELAY_ADDR", default_value = "localhost:4443")]
        relay: String,

        /// Authentication token for the relay
        #[arg(long, env = "LOCALUP_AUTH_TOKEN")]
        token: String,

        /// Target address to forward traffic to (host:port)
        #[arg(long, env = "LOCALUP_TARGET_ADDRESS")]
        target_address: String,

        /// Agent ID (auto-generated if not provided)
        #[arg(long, env = "LOCALUP_AGENT_ID")]
        agent_id: Option<String>,

        /// Skip TLS certificate verification (INSECURE - dev only)
        #[arg(long, env = "LOCALUP_INSECURE")]
        insecure: bool,

        /// JWT secret for validating agent tokens (optional)
        #[arg(long, env = "LOCALUP_JWT_SECRET")]
        jwt_secret: Option<String>,

        /// Log level (trace, debug, info, warn, error)
        #[arg(long, env = "RUST_LOG", default_value = "info")]
        log_level: String,
    },
    /// Run as exit node / relay server
    Relay {
        #[command(subcommand)]
        command: RelayCommands,
    },
    /// Run as agent server (combines relay and agent functionality)
    AgentServer {
        /// Listen address for QUIC server
        #[arg(
            short = 'l',
            long,
            default_value = "0.0.0.0:4443",
            env = "LOCALUP_LISTEN"
        )]
        listen: String,

        /// TLS certificate path (auto-generated if not provided)
        #[arg(long, env = "LOCALUP_CERT")]
        cert: Option<String>,

        /// TLS key path (auto-generated if not provided)
        #[arg(long, env = "LOCALUP_KEY")]
        key: Option<String>,

        /// JWT secret for authentication (optional)
        #[arg(long, env = "LOCALUP_JWT_SECRET")]
        jwt_secret: Option<String>,

        /// Relay server address to connect to (optional)
        #[arg(long, env = "LOCALUP_RELAY_ADDR")]
        relay_addr: Option<String>,

        /// Server ID on the relay (required if relay_addr is set)
        #[arg(long, env = "LOCALUP_RELAY_ID")]
        relay_id: Option<String>,

        /// Authentication token for relay server (optional)
        #[arg(long, env = "LOCALUP_RELAY_TOKEN")]
        relay_token: Option<String>,

        /// Target address for relay forwarding (required if relay_addr is set)
        #[arg(long, env = "LOCALUP_TARGET_ADDRESS")]
        target_address: Option<String>,

        /// Enable verbose logging
        #[arg(short, long)]
        verbose: bool,
    },
    /// Generate a JWT token for client authentication
    GenerateToken {
        /// JWT secret (must match the relay's --jwt-secret)
        #[arg(long, env = "TUNNEL_JWT_SECRET")]
        secret: String,

        /// Subject/Tunnel identifier (optional, if not specified a random UUID is generated)
        /// Use this to identify the tunnel in logs and routing
        #[arg(long)]
        sub: Option<String>,

        /// Token validity in hours (default: 24)
        #[arg(long, default_value = "24")]
        hours: i64,

        /// Enable reverse tunnel access (allows client to request agent-to-client connections)
        #[arg(long)]
        reverse_tunnel: bool,

        /// Allowed agent IDs for reverse tunnels (repeatable, e.g., --agent agent-1 --agent agent-2)
        /// If not specified, all agents are allowed
        #[arg(long = "agent")]
        allowed_agents: Vec<String>,

        /// Allowed target addresses for reverse tunnels (repeatable, format: host:port)
        /// Example: --allowed-address 192.168.1.100:8080 --allowed-address 10.0.0.5:22
        /// If not specified, all addresses are allowed
        #[arg(long = "allowed-address")]
        allowed_addresses: Vec<String>,

        /// Output only the JWT token (useful for scripts)
        #[arg(long)]
        token_only: bool,
    },
    /// Manage global CLI configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// Set the default authentication token
    SetToken {
        /// Authentication token to store
        token: String,
    },
    /// Get the default authentication token
    GetToken,
    /// Clear the default authentication token
    ClearToken,
}

#[derive(Subcommand, Debug)]
enum RelayCommands {
    /// TCP tunnel relay (port-based routing)
    Tcp {
        /// Tunnel control port for client connections (QUIC)
        #[arg(long, default_value = "0.0.0.0:4443")]
        localup_addr: String,

        /// TCP port range for raw TCP tunnels (format: "10000-20000")
        #[arg(long, default_value = "10000-20000")]
        tcp_port_range: String,

        /// Public domain name for this relay
        #[arg(long, default_value = "localhost")]
        domain: String,

        /// JWT secret for authenticating tunnel clients
        #[arg(long, env = "JWT_SECRET")]
        jwt_secret: Option<String>,

        /// Log level (trace, debug, info, warn, error)
        #[arg(long, default_value = "info")]
        log_level: String,

        /// API server bind address
        #[arg(long, default_value = "127.0.0.1:3080")]
        api_addr: String,

        /// Disable API server
        #[arg(long)]
        no_api: bool,

        /// Database URL for storing traffic logs
        #[arg(long, env = "DATABASE_URL")]
        database_url: Option<String>,

        /// Allow public user registration (disabled by default for security)
        #[arg(long, env = "ALLOW_SIGNUP")]
        allow_signup: bool,
    },

    /// TLS/SNI relay (SNI-based routing, no certificates needed)
    Tls {
        /// Tunnel control port for client connections (QUIC)
        #[arg(long, default_value = "0.0.0.0:4443")]
        localup_addr: String,

        /// TLS/SNI server bind address
        #[arg(long, default_value = "0.0.0.0:4443")]
        tls_addr: String,

        /// Public domain name for this relay
        #[arg(long, default_value = "localhost")]
        domain: String,

        /// JWT secret for authenticating tunnel clients
        #[arg(long, env = "JWT_SECRET")]
        jwt_secret: Option<String>,

        /// Log level (trace, debug, info, warn, error)
        #[arg(long, default_value = "info")]
        log_level: String,

        /// API server bind address
        #[arg(long, default_value = "127.0.0.1:3080")]
        api_addr: String,

        /// Disable API server
        #[arg(long)]
        no_api: bool,

        /// Database URL for storing traffic logs
        #[arg(long, env = "DATABASE_URL")]
        database_url: Option<String>,

        /// Allow public user registration (disabled by default for security)
        #[arg(long, env = "ALLOW_SIGNUP")]
        allow_signup: bool,
    },

    /// HTTP/HTTPS relay (host-based routing with TLS termination)
    Http {
        /// Tunnel control port for client connections (QUIC)
        #[arg(long, default_value = "0.0.0.0:4443")]
        localup_addr: String,

        /// HTTP server bind address
        #[arg(long, default_value = "0.0.0.0:8080")]
        http_addr: String,

        /// HTTPS server bind address (requires TLS certificates)
        #[arg(long)]
        https_addr: Option<String>,

        /// TLS certificate file path (PEM format)
        #[arg(long)]
        tls_cert: Option<String>,

        /// TLS private key file path (PEM format)
        #[arg(long)]
        tls_key: Option<String>,

        /// Public domain name for this relay
        #[arg(long, default_value = "localhost")]
        domain: String,

        /// JWT secret for authenticating tunnel clients
        #[arg(long, env = "JWT_SECRET")]
        jwt_secret: Option<String>,

        /// Log level (trace, debug, info, warn, error)
        #[arg(long, default_value = "info")]
        log_level: String,

        /// API server bind address
        #[arg(long, default_value = "127.0.0.1:3080")]
        api_addr: String,

        /// Disable API server
        #[arg(long)]
        no_api: bool,

        /// Database URL for storing traffic logs
        #[arg(long, env = "DATABASE_URL")]
        database_url: Option<String>,

        /// Admin email for auto-creating admin user on startup
        #[arg(long, env = "ADMIN_EMAIL")]
        admin_email: Option<String>,

        /// Admin password for auto-creating admin user on startup
        #[arg(long, env = "ADMIN_PASSWORD")]
        admin_password: Option<String>,

        /// Admin username for auto-creating admin user on startup (optional, defaults to email)
        #[arg(long, env = "ADMIN_USERNAME")]
        admin_username: Option<String>,

        /// Allow public user registration (disabled by default for security)
        #[arg(long, env = "ALLOW_SIGNUP")]
        allow_signup: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum DaemonCommands {
    /// Start daemon in foreground
    Start,
    /// Check daemon status
    Status,
}

#[derive(Subcommand, Debug, Clone)]
enum ServiceCommands {
    /// Install system service
    Install,
    /// Uninstall system service
    Uninstall,
    /// Start service
    Start,
    /// Stop service
    Stop,
    /// Restart service
    Restart,
    /// Check service status
    Status,
    /// View service logs
    Logs {
        /// Number of lines to show
        #[arg(short, long, default_value = "50")]
        lines: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize rustls crypto provider (required for QUIC/TLS)
    rustls::crypto::CryptoProvider::install_default(rustls::crypto::ring::default_provider())
        .unwrap();

    let cli = Cli::parse();

    // Initialize logging
    init_logging(&cli.log_level)?;

    match cli.command {
        Some(Commands::Add {
            name,
            port,
            address,
            protocol,
            token,
            subdomain,
            domain,
            relay,
            remote_port,
            enabled,
        }) => handle_add_tunnel(
            name,
            port,
            address,
            protocol,
            token,
            subdomain,
            relay,
            remote_port,
            enabled,
        ),
        Some(Commands::List) => handle_list_tunnels(),
        Some(Commands::Show { name }) => handle_show_tunnel(name),
        Some(Commands::Remove { name }) => handle_remove_tunnel(name),
        Some(Commands::Enable { name }) => handle_enable_tunnel(name),
        Some(Commands::Disable { name }) => handle_disable_tunnel(name),
        Some(Commands::Daemon { ref command }) => handle_daemon_command(command.clone()).await,
        Some(Commands::Service { ref command }) => handle_service_command(command.clone()),
        Some(Commands::Connect {
            relay,
            remote_address,
            agent_id,
            local_address,
            token,
            agent_token,
            insecure,
        }) => {
            handle_connect_command(
                relay,
                remote_address,
                agent_id,
                local_address,
                token,
                agent_token,
                insecure,
            )
            .await
        }
        Some(Commands::Agent {
            relay,
            token,
            target_address,
            agent_id,
            insecure,
            jwt_secret,
            log_level,
        }) => {
            handle_agent_command(
                relay,
                token,
                target_address,
                agent_id,
                insecure,
                jwt_secret,
                log_level,
            )
            .await
        }
        Some(Commands::Relay { command }) => handle_relay_subcommand(command).await,
        Some(Commands::AgentServer {
            listen,
            cert,
            key,
            jwt_secret,
            relay_addr,
            relay_id,
            relay_token,
            target_address,
            verbose,
        }) => {
            handle_agent_server_command(
                listen,
                cert,
                key,
                jwt_secret,
                relay_addr,
                relay_id,
                relay_token,
                target_address,
                verbose,
            )
            .await
        }
        Some(Commands::GenerateToken {
            secret,
            sub,
            hours,
            reverse_tunnel,
            allowed_agents,
            allowed_addresses,
            token_only,
        }) => {
            handle_generate_token_command(
                secret,
                sub,
                hours,
                reverse_tunnel,
                allowed_agents,
                allowed_addresses,
                token_only,
            )
            .await
        }
        Some(Commands::Config { ref command }) => handle_config_command(command).await,
        None => {
            // Standalone mode - run a single tunnel
            run_standalone(cli).await
        }
    }
}

async fn handle_daemon_command(command: DaemonCommands) -> Result<()> {
    match command {
        DaemonCommands::Start => {
            info!("Starting daemon...");

            let daemon = daemon::Daemon::new()?;
            let (command_tx, command_rx) = tokio::sync::mpsc::channel(32);

            // Spawn Ctrl+C handler
            let command_tx_clone = command_tx.clone();
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                info!("Shutting down daemon...");
                command_tx_clone
                    .send(daemon::DaemonCommand::Shutdown)
                    .await
                    .ok();
            });

            daemon.run(command_rx).await?;
            Ok(())
        }
        DaemonCommands::Status => {
            // TODO: Implement IPC to query running daemon
            println!("Daemon status: Not implemented yet");
            println!("Use 'localup service status' to check if service is running");
            Ok(())
        }
    }
}

fn handle_service_command(command: ServiceCommands) -> Result<()> {
    let service_manager = service::ServiceManager::new();

    if !service_manager.is_supported() {
        eprintln!("❌ Service management is not supported on this platform");
        eprintln!("   Supported platforms: macOS (launchd), Linux (systemd)");
        std::process::exit(1);
    }

    match command {
        ServiceCommands::Install => service_manager.install(),
        ServiceCommands::Uninstall => service_manager.uninstall(),
        ServiceCommands::Start => service_manager.start(),
        ServiceCommands::Stop => service_manager.stop(),
        ServiceCommands::Restart => service_manager.restart(),
        ServiceCommands::Status => {
            let status = service_manager.status()?;
            println!("Service status: {}", status);
            Ok(())
        }
        ServiceCommands::Logs { lines } => {
            let logs = service_manager.logs(lines)?;
            print!("{}", logs);
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_add_tunnel(
    name: String,
    port: Option<u16>,
    address: Option<String>,
    protocol: String,
    token: Option<String>,
    subdomain: Option<String>,
    relay: Option<String>,
    remote_port: Option<u16>,
    enabled: bool,
) -> Result<()> {
    let store = localup_store::TunnelStore::new()?;

    // Parse port and address - user must provide one or the other
    let (local_host, local_port) = if let Some(addr) = address {
        // User provided --address
        parse_local_address(&addr)?
    } else if let Some(p) = port {
        // User provided --port
        (String::from("localhost"), p)
    } else {
        return Err(anyhow::anyhow!(
            "Either --port or --address must be provided for tunnel configuration"
        ));
    };

    // Parse protocol
    let protocol_config = parse_protocol(&protocol, local_port, subdomain, remote_port)?;

    // Parse exit node
    let exit_node = if let Some(relay_addr) = relay {
        validate_relay_addr(&relay_addr)?;
        ExitNodeConfig::Custom(relay_addr)
    } else {
        ExitNodeConfig::Auto
    };

    // Create tunnel config
    let config = TunnelConfig {
        local_host,
        protocols: vec![protocol_config],
        auth_token: token.unwrap_or_default(),
        exit_node,
        failover: true,
        connection_timeout: Duration::from_secs(30),
    };

    let stored_tunnel = localup_store::StoredTunnel {
        name: name.clone(),
        enabled,
        config,
    };

    store.save(&stored_tunnel)?;

    println!("✅ Tunnel '{}' added successfully", name);
    println!(
        "   Configuration: {}",
        store.base_dir().join(format!("{}.json", name)).display()
    );
    if enabled {
        println!("   Status: Enabled (will auto-start with daemon)");
    } else {
        println!(
            "   Status: Disabled (use 'localup enable {}' to enable)",
            name
        );
    }

    Ok(())
}

fn handle_list_tunnels() -> Result<()> {
    let store = localup_store::TunnelStore::new()?;
    let tunnels = store.list()?;

    if tunnels.is_empty() {
        println!("No tunnels configured");
        println!("Add a tunnel with: localup add <name> --port <port> --protocol <protocol> --token <token>");
        return Ok(());
    }

    println!("Configured tunnels ({})", tunnels.len());
    println!();

    for tunnel in tunnels {
        let status = if tunnel.enabled {
            "✅ Enabled"
        } else {
            "⚪ Disabled"
        };
        println!("  {} {}", status, tunnel.name);

        for protocol in &tunnel.config.protocols {
            match protocol {
                ProtocolConfig::Http {
                    local_port,
                    subdomain,
                } => {
                    println!("    Protocol: HTTP, Port: {}", local_port);
                    if let Some(sub) = subdomain {
                        println!("    Subdomain: {}", sub);
                    }
                }
                ProtocolConfig::Https {
                    local_port,
                    subdomain,
                } => {
                    println!("    Protocol: HTTPS, Port: {}", local_port);
                    if let Some(sub) = subdomain {
                        println!("    Subdomain: {}", sub);
                    }
                }
                ProtocolConfig::Tcp {
                    local_port,
                    remote_port,
                } => {
                    print!("    Protocol: TCP, Port: {}", local_port);
                    if let Some(remote) = remote_port {
                        print!(" → Remote: {}", remote);
                    }
                    println!();
                }
                ProtocolConfig::Tls {
                    local_port,
                    sni_hostname,
                } => {
                    print!("    Protocol: TLS, Port: {}", local_port);
                    if let Some(sni) = sni_hostname {
                        print!(", SNI: {}", sni);
                    }
                    println!();
                }
            }
        }

        match &tunnel.config.exit_node {
            ExitNodeConfig::Auto => println!("    Relay: Auto"),
            ExitNodeConfig::Custom(addr) => println!("    Relay: {}", addr),
            _ => {}
        }

        println!();
    }

    Ok(())
}

fn handle_show_tunnel(name: String) -> Result<()> {
    let store = localup_store::TunnelStore::new()?;
    let tunnel = store.load(&name)?;
    let json = serde_json::to_string_pretty(&tunnel)?;
    println!("{}", json);
    Ok(())
}

fn handle_remove_tunnel(name: String) -> Result<()> {
    let store = localup_store::TunnelStore::new()?;
    store.remove(&name)?;
    println!("✅ Tunnel '{}' removed", name);
    Ok(())
}

fn handle_enable_tunnel(name: String) -> Result<()> {
    let store = localup_store::TunnelStore::new()?;
    store.enable(&name)?;
    println!("✅ Tunnel '{}' enabled (will auto-start with daemon)", name);
    Ok(())
}

fn handle_disable_tunnel(name: String) -> Result<()> {
    let store = localup_store::TunnelStore::new()?;
    store.disable(&name)?;
    println!("✅ Tunnel '{}' disabled", name);
    Ok(())
}

/// Parse port/address string into (host, port)
/// Supports:
/// - "3000" -> ("localhost", 3000)
/// - "127.0.0.1:3000" -> ("127.0.0.1", 3000)
/// - "example.com:8080" -> ("example.com", 8080)
fn parse_local_address(addr_str: &str) -> Result<(String, u16)> {
    if addr_str.contains(':') {
        // Full address with host:port
        let parts: Vec<&str> = addr_str.rsplitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!(
                "Invalid address format: {}. Expected 'host:port' or just 'port'",
                addr_str
            ));
        }
        let host = parts[1].to_string();
        let port: u16 = parts[0].parse().context(format!(
            "Invalid port number '{}' in address '{}'",
            parts[0], addr_str
        ))?;
        Ok((host, port))
    } else {
        // Just a port number, default to localhost
        let port: u16 = addr_str.parse().context(format!(
            "Invalid port number '{}'. Must be a number or 'host:port' format",
            addr_str
        ))?;
        Ok(("localhost".to_string(), port))
    }
}

async fn run_standalone(cli: Cli) -> Result<()> {
    // Check if required arguments are present for standalone mode
    if cli.token.is_none() {
        eprintln!("Error: Standalone mode requires --token argument");
        eprintln!();
        eprintln!("Usage:");
        eprintln!("  localup --port <PORT> --protocol <PROTOCOL> --token <TOKEN>");
        eprintln!("  localup --address <HOST:PORT> --protocol <PROTOCOL> --token <TOKEN>");
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  localup --port 3000 --protocol http --token <TOKEN>");
        eprintln!("  localup --address 127.0.0.1:8080 --protocol http --token <TOKEN>");
        eprintln!();
        eprintln!("Or use tunnel management commands:");
        eprintln!("  localup add <name> --port <PORT> --token <TOKEN>");
        eprintln!("  localup add <name> --address <HOST:PORT> --token <TOKEN>");
        eprintln!("  localup daemon start");
        eprintln!("  localup service install");
        eprintln!();
        eprintln!("For more help, run: localup --help");
        std::process::exit(1);
    }

    let token = cli.token.unwrap();
    let protocol_str = cli.protocol.unwrap_or_else(|| "http".to_string());

    // Parse port and address - user must provide one or the other
    let (local_host, local_port) = if let Some(addr) = cli.address {
        // User provided --address
        parse_local_address(&addr)?
    } else if let Some(p) = cli.port {
        // User provided --port
        (String::from("localhost"), p)
    } else {
        return Err(anyhow::anyhow!(
            "Either --port or --address must be provided for standalone mode"
        ));
    };

    info!("🚀 Tunnel CLI starting (standalone mode)...");
    info!("Protocol: {}", protocol_str);
    info!("Local address: {}:{}", local_host, local_port);

    // Parse protocol configuration
    let protocol = parse_protocol(
        &protocol_str,
        local_port,
        cli.subdomain.clone(),
        cli.remote_port,
    )?;

    // Parse exit node configuration
    let exit_node = if let Some(relay_addr) = cli.relay {
        info!("Using custom relay: {}", relay_addr);
        validate_relay_addr(&relay_addr)?;
        ExitNodeConfig::Custom(relay_addr)
    } else {
        info!("Using automatic relay selection");
        ExitNodeConfig::Auto
    };

    // Build tunnel configuration
    let config = TunnelConfig {
        local_host,
        protocols: vec![protocol],
        auth_token: token.clone(),
        exit_node,
        failover: true,
        connection_timeout: Duration::from_secs(30),
    };

    // Create cancellation token for Ctrl+C
    let (cancel_tx, mut cancel_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Spawn Ctrl+C handler
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Shutting down tunnel...");
        cancel_tx.send(()).await.ok();
    });

    // Reconnection loop with exponential backoff
    let mut reconnect_attempt = 0u32;
    let mut metrics_server_started = false;

    loop {
        // Calculate backoff delay (exponential: 1s, 2s, 4s, 8s, 16s, max 30s)
        let backoff_seconds = if reconnect_attempt == 0 {
            0
        } else {
            std::cmp::min(2u64.pow(reconnect_attempt - 1), 30)
        };

        if backoff_seconds > 0 {
            info!(
                "⏳ Waiting {} seconds before reconnecting...",
                backoff_seconds
            );

            // Use select! to make sleep cancellable by Ctrl+C
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(backoff_seconds)) => {
                    // Sleep completed normally
                }
                _ = cancel_rx.recv() => {
                    // Ctrl+C was pressed during sleep
                    info!("Shutdown requested, exiting...");
                    break;
                }
            }
        } else {
            // Check if user pressed Ctrl+C (non-blocking when backoff is 0)
            if cancel_rx.try_recv().is_ok() {
                info!("Shutdown requested, exiting...");
                break;
            }
        }

        info!(
            "Connecting to tunnel... (attempt {})",
            reconnect_attempt + 1
        );

        match TunnelClient::connect(config.clone()).await {
            Ok(client) => {
                reconnect_attempt = 0; // Reset on successful connection

                info!("✅ Tunnel connected successfully!");

                // Display public URL if available
                if let Some(url) = client.public_url() {
                    println!();
                    println!("🌍 Your local server is now public!");
                    println!("📍 Local:  http://localhost:{}", local_port);
                    println!("🌐 Public: {}", url);
                    println!();
                }

                // Start metrics server if enabled (only once)
                if !cli.no_metrics && !metrics_server_started {
                    let metrics = client.metrics().clone();
                    let endpoints = client.endpoints().to_vec();

                    // Try to bind to requested port, fallback to any available port
                    let requested_addr = format!("127.0.0.1:{}", cli.metrics_port);
                    let listener = match TcpListener::bind(&requested_addr).await {
                        Ok(listener) => listener,
                        Err(_) => {
                            warn!(
                                "Port {} already in use, finding available port...",
                                cli.metrics_port
                            );
                            TcpListener::bind("127.0.0.1:0")
                                .await
                                .expect("Failed to bind to any port")
                        }
                    };

                    let metrics_addr = listener.local_addr().expect("Failed to get local address");
                    let actual_port = metrics_addr.port();
                    drop(listener); // Release the port for the server to bind

                    // Local upstream URL for replay functionality
                    let local_upstream = format!("http://localhost:{}", local_port);

                    tokio::spawn(async move {
                        let server =
                            MetricsServer::new(metrics_addr, metrics, endpoints, local_upstream);
                        if let Err(e) = server.run().await {
                            error!("Metrics server error: {}", e);
                        }
                    });

                    println!("📊 Metrics dashboard: http://127.0.0.1:{}", actual_port);
                    println!();
                    metrics_server_started = true;
                }

                info!("Tunnel is active. Press Ctrl+C to stop.");

                // Get disconnect handle before moving client into wait()
                let disconnect_future = client.disconnect_handle();

                // Spawn wait task
                let mut wait_task = tokio::spawn(client.wait());

                // Wait for Ctrl+C or tunnel close
                tokio::select! {
                    wait_result = &mut wait_task => {
                        match wait_result {
                            Ok(Ok(_)) => {
                                info!("Tunnel closed gracefully");
                            }
                            Ok(Err(e)) => {
                                error!("Tunnel error: {}", e);
                            }
                            Err(e) => {
                                error!("Tunnel task panicked: {}", e);
                            }
                        }
                    }
                    _ = cancel_rx.recv() => {
                        info!("Shutdown requested, sending disconnect...");

                        // Send graceful disconnect signal
                        if let Err(e) = disconnect_future.await {
                            error!("Failed to trigger disconnect: {}", e);
                        }

                        // Wait for the tunnel to gracefully close (with timeout)
                        match tokio::time::timeout(
                            tokio::time::Duration::from_secs(5),
                            wait_task
                        ).await {
                            Ok(Ok(Ok(_))) => {
                                info!("✅ Tunnel closed gracefully");
                            }
                            Ok(Ok(Err(e))) => {
                                error!("Tunnel error during shutdown: {}", e);
                            }
                            Ok(Err(e)) => {
                                error!("Tunnel task panicked during shutdown: {}", e);
                            }
                            Err(_) => {
                                warn!("Graceful shutdown timed out after 5s");
                            }
                        }

                        info!("Shutting down...");
                        break;
                    }
                }

                info!("🔄 Connection lost, attempting to reconnect...");
            }
            Err(e) => {
                error!("❌ Failed to connect tunnel: {}", e);

                // Check if this is a non-recoverable error - don't retry
                if e.is_non_recoverable() {
                    error!("🚫 Non-recoverable error detected.");

                    // Provide specific guidance based on error type
                    match &e {
                        localup_client::TunnelError::AuthenticationFailed(reason) => {
                            error!("   Authentication failed: {}", reason);
                            error!("   Token provided: {}", token);
                            error!("   Please check your authentication token and try again.");
                        }
                        localup_client::TunnelError::ConfigError(reason) => {
                            error!("   Configuration error: {}", reason);
                            error!("   Please check your configuration and try again.");
                        }
                        _ => {}
                    }

                    error!("   Exiting to prevent retries...");
                    break;
                }

                // Recoverable error - will retry with exponential backoff
                reconnect_attempt += 1;

                // Check if user pressed Ctrl+C
                if cancel_rx.try_recv().is_ok() {
                    info!("Shutdown requested, exiting...");
                    break;
                }
            }
        }
    }

    Ok(())
}

fn parse_protocol(
    protocol: &str,
    port: u16,
    subdomain: Option<String>,
    remote_port: Option<u16>,
) -> Result<ProtocolConfig> {
    match protocol.to_lowercase().as_str() {
        "http" => Ok(ProtocolConfig::Http {
            local_port: port,
            subdomain,
        }),
        "https" => Ok(ProtocolConfig::Https {
            local_port: port,
            subdomain,
        }),
        "tcp" => Ok(ProtocolConfig::Tcp {
            local_port: port,
            remote_port,
        }),
        "tls" => Ok(ProtocolConfig::Tls {
            local_port: port,
            sni_hostname: subdomain,
        }),
        _ => Err(anyhow::anyhow!(
            "Invalid protocol: {}. Valid options: http, https, tcp, tls",
            protocol
        )),
    }
}

fn validate_relay_addr(relay_addr: &str) -> Result<()> {
    if !relay_addr.contains(':') {
        anyhow::bail!(
            "Invalid relay address: {}. Expected format: host:port or ip:port",
            relay_addr
        );
    }

    // Try to parse as SocketAddr (IP:port) first
    if relay_addr.parse::<SocketAddr>().is_err() {
        // If that fails, validate it looks like hostname:port
        let parts: Vec<&str> = relay_addr.split(':').collect();
        if parts.len() != 2 {
            anyhow::bail!(
                "Invalid relay address: {}. Expected format: host:port",
                relay_addr
            );
        }
        if parts[1].parse::<u16>().is_err() {
            anyhow::bail!("Invalid port in relay address: {}", relay_addr);
        }
    }

    Ok(())
}

async fn handle_connect_command(
    relay: String,
    remote_address: String,
    agent_id: String,
    local_address: Option<String>,
    token: Option<String>,
    agent_token: Option<String>,
    insecure: bool,
) -> Result<()> {
    info!("🚀 Connecting to reverse tunnel...");
    info!("Relay: {}", relay);
    info!("Remote address: {}", remote_address);
    info!("Agent ID: {}", agent_id);

    // Validate relay address
    validate_relay_addr(&relay)?;

    // Build configuration
    let mut config =
        ReverseTunnelConfig::new(relay.clone(), remote_address.clone(), agent_id.clone())
            .with_insecure(insecure);

    if let Some(token_value) = token {
        config = config.with_auth_token(token_value);
    }

    if let Some(agent_token_value) = agent_token {
        config = config.with_agent_token(agent_token_value);
    }

    if let Some(local_addr) = local_address {
        config = config.with_local_bind_address(local_addr);
    }

    // Exponential backoff parameters for reconnection
    let initial_backoff = std::time::Duration::from_secs(1);
    let max_backoff = std::time::Duration::from_secs(60);
    let backoff_multiplier = 2.0;

    let mut current_backoff = initial_backoff;
    let mut attempt = 0;
    let mut first_connection = true;

    // Try to connect with automatic reconnection on disconnect
    loop {
        attempt += 1;

        // Connect to reverse tunnel
        let client = match ReverseTunnelClient::connect(config.clone()).await {
            Ok(client) => client,
            Err(e) => {
                error!(
                    "❌ Failed to connect to reverse tunnel (attempt {}): {}",
                    attempt, e
                );

                // Only show detailed errors on first attempt
                if first_connection {
                    match &e {
                        localup_client::ReverseTunnelError::AgentNotAvailable(msg) => {
                            eprintln!();
                            eprintln!("Agent not available:");
                            eprintln!("  {}", msg);
                            eprintln!();
                            eprintln!("Make sure the agent is:");
                            eprintln!("  1. Running on the target network");
                            eprintln!("  2. Connected to the relay server ({})", relay);
                            eprintln!("  3. Using the correct agent ID ({})", agent_id);
                            eprintln!();
                            eprintln!("Retrying with exponential backoff...");
                            eprintln!();
                        }
                        localup_client::ReverseTunnelError::ConnectionFailed(msg) => {
                            eprintln!();
                            eprintln!("Connection failed:");
                            eprintln!("  {}", msg);
                            eprintln!();
                            eprintln!("Check that:");
                            eprintln!("  1. The relay server is reachable at {}", relay);
                            eprintln!("  2. The relay server is running");
                            eprintln!("  3. Your network allows outbound QUIC/UDP connections");
                            eprintln!();
                            eprintln!("Retrying with exponential backoff...");
                            eprintln!();
                        }
                        localup_client::ReverseTunnelError::Rejected(msg) => {
                            eprintln!();
                            eprintln!("Reverse tunnel rejected:");
                            eprintln!("  {}", msg);
                            eprintln!();
                            if msg.contains("auth") || msg.contains("token") {
                                eprintln!(
                                    "Authentication may be required. Use --token to provide credentials."
                                );
                            }
                            eprintln!("Retrying with exponential backoff...");
                            eprintln!();
                        }
                        localup_client::ReverseTunnelError::Timeout(msg) => {
                            eprintln!();
                            eprintln!("Connection timeout:");
                            eprintln!("  {}", msg);
                            eprintln!();
                            eprintln!("The relay server may be slow or unreachable.");
                            eprintln!("Retrying with exponential backoff...");
                            eprintln!();
                        }
                        _ => {
                            eprintln!();
                            eprintln!("Error: {}", e);
                            eprintln!("Retrying with exponential backoff...");
                            eprintln!();
                        }
                    }
                }

                // Wait and retry with exponential backoff
                info!(
                    "Reconnecting in {}s (attempt {})...",
                    current_backoff.as_secs(),
                    attempt
                );
                tokio::time::sleep(current_backoff).await;

                // Increase backoff for next attempt
                let next_backoff = std::time::Duration::from_secs_f64(
                    current_backoff.as_secs_f64() * backoff_multiplier,
                );
                current_backoff = next_backoff.min(max_backoff);
                first_connection = false;
                continue;
            }
        };

        // Successfully connected - print appropriate message
        if attempt > 1 {
            println!();
            println!("✅ Reconnected after {} attempts!", attempt - 1);
        } else {
            println!();
            println!("✅ Reverse tunnel established!");
        }
        println!();
        println!("Local address:  {}", client.local_addr());
        println!("Remote address: {}", client.remote_address());
        println!("Agent ID:       {}", client.agent_id());
        println!("Tunnel ID:      {}", client.localup_id());
        println!();
        println!(
            "Connect to {} to access the remote service.",
            client.local_addr()
        );
        println!();
        println!("Press Ctrl+C to disconnect.");
        println!();

        // Create cancellation token for Ctrl+C
        let (cancel_tx, mut cancel_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Spawn Ctrl+C handler
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            info!("Shutting down reverse tunnel...");
            cancel_tx.send(()).await.ok();
        });

        // Wait for tunnel to close or Ctrl+C
        let wait_task = tokio::spawn(async move { client.wait().await });

        let localup_closed = tokio::select! {
            wait_result = wait_task => {
                match wait_result {
                    Ok(Ok(_)) => {
                        info!("Reverse tunnel closed gracefully");
                        true
                    }
                    Ok(Err(e)) => {
                        error!("Reverse tunnel error: {}", e);
                        true
                    }
                    Err(e) => {
                        error!("Reverse tunnel task panicked: {}", e);
                        true
                    }
                }
            }
            _ = cancel_rx.recv() => {
                info!("Shutdown requested, closing reverse tunnel...");
                println!();
                println!("🛑 Shutting down...");
                false
            }
        };

        // If user pressed Ctrl+C, exit; otherwise reconnect
        if !localup_closed {
            break;
        }
    }

    Ok(())
}

async fn handle_agent_command(
    relay: String,
    token: String,
    target_address: String,
    agent_id: Option<String>,
    insecure: bool,
    jwt_secret: Option<String>,
    log_level: String,
) -> Result<()> {
    use localup_agent::{Agent, AgentConfig};
    use uuid::Uuid;

    // The logging is already initialized in main, but reinitialize at this log level
    let _ = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new(&log_level));

    info!("Starting LocalUp Agent");

    // Create agent configuration
    let agent_id = agent_id.unwrap_or_else(|| Uuid::new_v4().to_string());

    let config = AgentConfig {
        agent_id: agent_id.clone(),
        relay_addr: relay.clone(),
        auth_token: token.clone(),
        target_address: target_address.clone(),
        insecure,
        local_address: None,
        jwt_secret,
    };

    // Display configuration (redact token)
    info!("Agent configuration:");
    info!("  Agent ID: {}", config.agent_id);
    info!("  Relay: {}", config.relay_addr);
    info!(
        "  Token: {}...",
        &config.auth_token[..config.auth_token.len().min(10)]
    );
    info!("  Target address: {}", config.target_address);
    info!("  Insecure mode: {}", config.insecure);

    if config.insecure {
        warn!("⚠️  Running in INSECURE mode - certificate verification is DISABLED");
        warn!("⚠️  This should ONLY be used for local development");
    }

    // Create and start agent
    let mut agent = Agent::new(config).context("Failed to create agent")?;

    // Run agent with Ctrl+C handling
    tokio::select! {
        result = agent.start() => {
            if let Err(e) = result {
                error!("Agent error: {}", e);
                return Err(e.into());
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down gracefully...");
            agent.stop().await;
        }
    }

    info!("Agent stopped");
    Ok(())
}

async fn handle_relay_subcommand(command: RelayCommands) -> Result<()> {
    match command {
        RelayCommands::Tcp {
            localup_addr,
            tcp_port_range,
            domain,
            jwt_secret,
            log_level,
            api_addr,
            no_api,
            database_url,
            allow_signup,
        } => {
            handle_relay_command(
                String::new(), // http_addr - not used for TCP
                localup_addr,
                None, // https_addr
                None, // tls_addr
                None, // tls_cert
                None, // tls_key
                domain,
                jwt_secret,
                log_level,
                Some(tcp_port_range),
                api_addr,
                no_api,
                database_url,
                None, // admin_email
                None, // admin_password
                None, // admin_username
                allow_signup,
            )
            .await
        }
        RelayCommands::Tls {
            localup_addr,
            tls_addr,
            domain,
            jwt_secret,
            log_level,
            api_addr,
            no_api,
            database_url,
            allow_signup,
        } => {
            handle_relay_command(
                String::new(), // http_addr - not used for TLS
                localup_addr,
                None, // https_addr
                Some(tls_addr),
                None, // tls_cert
                None, // tls_key
                domain,
                jwt_secret,
                log_level,
                None, // tcp_port_range
                api_addr,
                no_api,
                database_url,
                None, // admin_email
                None, // admin_password
                None, // admin_username
                allow_signup,
            )
            .await
        }
        RelayCommands::Http {
            localup_addr,
            http_addr,
            https_addr,
            tls_cert,
            tls_key,
            domain,
            jwt_secret,
            log_level,
            api_addr,
            no_api,
            database_url,
            admin_email,
            admin_password,
            admin_username,
            allow_signup,
        } => {
            handle_relay_command(
                http_addr,
                localup_addr,
                https_addr,
                None, // tls_addr
                tls_cert,
                tls_key,
                domain,
                jwt_secret,
                log_level,
                None, // tcp_port_range
                api_addr,
                no_api,
                database_url,
                admin_email,
                admin_password,
                admin_username,
                allow_signup,
            )
            .await
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_relay_command(
    http_addr: String,
    localup_addr: String,
    https_addr: Option<String>,
    tls_addr: Option<String>,
    tls_cert: Option<String>,
    tls_key: Option<String>,
    domain: String,
    jwt_secret: Option<String>,
    log_level: String,
    tcp_port_range: Option<String>,
    api_addr: String,
    no_api: bool,
    database_url: Option<String>,
    admin_email: Option<String>,
    admin_password: Option<String>,
    admin_username: Option<String>,
    allow_signup: bool,
) -> Result<()> {
    use localup_auth::JwtValidator;
    use localup_control::{
        AgentRegistry, PortAllocator as PortAllocatorTrait, TunnelConnectionManager, TunnelHandler,
    };
    use localup_router::RouteRegistry;
    use localup_server_https::{HttpsServer, HttpsServerConfig};
    use localup_server_tcp::{TcpServer, TcpServerConfig};
    use localup_server_tls::{TlsServer, TlsServerConfig};
    use localup_transport::TransportListener;
    use localup_transport_quic::QuicListener;
    use std::net::SocketAddr;
    use std::sync::Arc;

    // Reinitialize logging at this level
    let _ = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new(&log_level));

    info!("🚀 Starting tunnel exit node");
    if !http_addr.is_empty() {
        info!("HTTP endpoint: {}", http_addr);
    }
    info!("Tunnel control: {}", localup_addr);
    info!("Public domain: {}", domain);
    info!("Subdomains will be: {{name}}.{}", domain);

    if let Some(ref https_addr) = https_addr {
        info!("HTTPS endpoint: {}", https_addr);
    }

    if let Some(ref tls_addr) = tls_addr {
        info!("TLS/SNI endpoint: {}", tls_addr);
    }

    // Initialize database connection
    let db_url = database_url.unwrap_or_else(|| "sqlite::memory:".to_string());
    info!("Connecting to database: {}", db_url);
    let db = localup_relay_db::connect(&db_url).await?;

    // Run migrations
    localup_relay_db::migrate(&db)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run database migrations: {}", e))?;

    // Auto-create admin user if credentials provided
    if let (Some(email), Some(password)) = (admin_email, admin_password) {
        use localup_auth::hash_password;
        use localup_relay_db::entities::user;
        use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

        // Check if user already exists
        let existing_user = user::Entity::find()
            .filter(user::Column::Email.eq(&email))
            .one(&db)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to check for existing user: {}", e))?;

        if existing_user.is_none() {
            let password_hash = hash_password(&password)
                .map_err(|e| anyhow::anyhow!("Failed to hash admin password: {}", e))?;

            let full_name = admin_username
                .unwrap_or_else(|| email.split('@').next().unwrap_or("admin").to_string());
            let user_id = uuid::Uuid::new_v4();

            let new_user = user::ActiveModel {
                id: Set(user_id),
                email: Set(email.clone()),
                password_hash: Set(password_hash),
                full_name: Set(Some(full_name.clone())),
                role: Set(user::UserRole::Admin),
                is_active: Set(true),
                created_at: Set(chrono::Utc::now()),
                updated_at: Set(chrono::Utc::now()),
            };

            new_user
                .insert(&db)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create admin user: {}", e))?;

            // Create default team for the admin user
            use localup_relay_db::entities::{team, team_member};
            let team_name = format!("{}'s Team", full_name);
            let team_id = uuid::Uuid::new_v4();

            let new_team = team::ActiveModel {
                id: Set(team_id),
                name: Set(team_name.clone()),
                slug: Set(full_name.to_lowercase().replace(' ', "-")),
                owner_id: Set(user_id),
                created_at: Set(chrono::Utc::now()),
                updated_at: Set(chrono::Utc::now()),
            };

            new_team
                .insert(&db)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create default team: {}", e))?;

            // Add admin user as team owner
            let new_team_member = team_member::ActiveModel {
                team_id: Set(team_id),
                user_id: Set(user_id),
                role: Set(team_member::TeamRole::Owner),
                joined_at: Set(chrono::Utc::now()),
            };

            new_team_member
                .insert(&db)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to add admin to team: {}", e))?;

            info!("✅ Admin user created: {} ({})", full_name, email);
            info!("   Default team created: {}", team_name);
            info!("   You can now log in at the web portal");
        } else {
            info!("ℹ️  Admin user already exists: {}", email);
        }
    }

    // Initialize TCP port allocator if TCP range provided
    let port_allocator = if let Some(ref tcp_range) = tcp_port_range {
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

    // Create JWT validator for tunnel authentication
    // Note: Only validates signature and expiration (no issuer/audience validation)
    let jwt_validator = if let Some(jwt_secret) = jwt_secret {
        let validator = Arc::new(JwtValidator::new(jwt_secret.as_bytes()));
        info!("✅ JWT authentication enabled (signature only)");
        Some(validator)
    } else {
        info!("⚠️  Running without JWT authentication (not recommended for production)");
        None
    };

    // Log signup configuration
    if allow_signup {
        info!("✅ Public user registration enabled (--allow-signup)");
        info!("   ⚠️  For production, consider disabling public signup for security");
    } else {
        info!("🔒 Public user registration disabled (invite-only mode)");
        info!("   Admin can create users manually via the admin panel");
    }

    // Create tunnel connection manager
    let localup_manager = Arc::new(TunnelConnectionManager::new());

    // Create agent registry for reverse tunnels
    let agent_registry = Arc::new(AgentRegistry::new());
    info!("✅ Agent registry initialized (reverse tunnels enabled)");

    // Create pending requests tracker
    let pending_requests = Arc::new(localup_control::PendingRequests::new());

    // Start HTTP server (only if address is not empty)
    let mut http_port: Option<u16> = None;
    let http_handle = if !http_addr.is_empty() {
        let http_addr_parsed: SocketAddr = http_addr.parse()?;
        http_port = Some(http_addr_parsed.port());
        let http_config = TcpServerConfig {
            bind_addr: http_addr_parsed,
        };
        let http_server = TcpServer::new(http_config, registry.clone())
            .with_localup_manager(localup_manager.clone())
            .with_pending_requests(pending_requests.clone())
            .with_database(db.clone());

        Some(tokio::spawn(async move {
            info!("Starting HTTP relay server");
            if let Err(e) = http_server.start().await {
                error!("HTTP server error: {}", e);
            }
        }))
    } else {
        None
    };

    // Start HTTPS server if configured
    let mut https_port: Option<u16> = None;
    let https_handle = if let Some(ref https_addr) = https_addr {
        let cert_path = tls_cert
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("HTTPS server requires --tls-cert"))?;
        let key_path = tls_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("HTTPS server requires --tls-key"))?;

        let https_addr_parsed: SocketAddr = https_addr.parse()?;
        https_port = Some(https_addr_parsed.port());
        let https_config = HttpsServerConfig {
            bind_addr: https_addr_parsed,
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
        };

        let https_server = HttpsServer::new(https_config, registry.clone())
            .with_localup_manager(localup_manager.clone())
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

    // Start TLS/SNI server if configured
    let mut tls_port: Option<u16> = None;
    let _tls_handle = if let Some(ref tls_addr_str) = tls_addr {
        let tls_addr_parsed: SocketAddr = tls_addr_str.parse()?;
        tls_port = Some(tls_addr_parsed.port());
        info!("🔐 TLS port extracted: {}", tls_port.unwrap_or(0));
        let tls_config = TlsServerConfig {
            bind_addr: tls_addr_parsed,
        };

        let tls_server = TlsServer::new(tls_config, registry.clone())
            .with_localup_manager(localup_manager.clone());
        info!("✅ TLS/SNI server configured (routes based on Server Name Indication)");

        let tls_addr_display = tls_addr_str.clone();
        Some(tokio::spawn(async move {
            info!("Starting TLS/SNI relay server on {}", tls_addr_display);
            if let Err(e) = tls_server.start().await {
                error!("TLS server error: {}", e);
            }
        }))
    } else {
        None
    };

    // Create tunnel handler
    let mut localup_handler = TunnelHandler::new(
        localup_manager.clone(),
        registry.clone(),
        jwt_validator.clone(),
        domain.clone(),
        pending_requests.clone(),
    )
    .with_agent_registry(agent_registry.clone());

    // Configure actual relay ports
    if let Some(port) = http_port {
        info!("📡 Configuring HTTP relay port: {}", port);
        localup_handler = localup_handler.with_http_port(port);
    }
    if let Some(port) = https_port {
        info!("📡 Configuring HTTPS relay port: {}", port);
        localup_handler = localup_handler.with_https_port(port);
    }
    if let Some(port) = tls_port {
        info!("📡 Configuring TLS relay port: {}", port);
        localup_handler = localup_handler.with_tls_port(port);
    }

    // Add port allocator if TCP range was provided
    if let Some(ref allocator) = port_allocator {
        localup_handler =
            localup_handler.with_port_allocator(allocator.clone() as Arc<dyn PortAllocatorTrait>);
        info!("✅ TCP port allocator configured");

        // Add TCP proxy spawner
        let localup_manager_for_spawner = localup_manager.clone();
        let db_for_spawner = db.clone();
        let spawner: localup_control::TcpProxySpawner =
            Arc::new(move |localup_id: String, port: u16| {
                let manager = localup_manager_for_spawner.clone();
                let localup_id_clone = localup_id.clone();
                let db_clone = db_for_spawner.clone();

                Box::pin(async move {
                    use localup_server_tcp_proxy::{TcpProxyServer, TcpProxyServerConfig};
                    use std::net::SocketAddr;

                    let bind_addr: SocketAddr = format!("0.0.0.0:{}", port)
                        .parse()
                        .map_err(|e| format!("Invalid bind address: {}", e))?;

                    let config = TcpProxyServerConfig {
                        bind_addr,
                        localup_id: localup_id.clone(),
                    };

                    let proxy_server =
                        TcpProxyServer::new(config, manager.clone()).with_database(db_clone);

                    // Note: No callback needed - TCP proxy opens new QUIC streams directly

                    // Start the proxy server in a background task
                    tokio::spawn(async move {
                        if let Err(e) = proxy_server.start().await {
                            error!(
                                "TCP proxy server error for tunnel {}: {}",
                                localup_id_clone, e
                            );
                        }
                    });

                    Ok(())
                })
            });

        localup_handler = localup_handler.with_tcp_proxy_spawner(spawner);
        info!("✅ TCP proxy spawner configured");
    }

    let localup_handler = Arc::new(localup_handler);

    // Start tunnel listener (QUIC)
    info!("🔧 Attempting to bind tunnel control to {}", localup_addr);

    let quic_config = if let (Some(cert), Some(key)) = (&tls_cert, &tls_key) {
        info!("🔐 Using custom TLS certificates for QUIC");
        Arc::new(localup_transport_quic::QuicConfig::server_default(
            cert, key,
        )?)
    } else {
        info!("🔐 Generating ephemeral self-signed certificate for QUIC...");
        let config = Arc::new(localup_transport_quic::QuicConfig::server_self_signed()?);
        info!("✅ Self-signed certificate generated (valid for 90 days)");
        config
    };

    let localup_addr_parsed: SocketAddr = localup_addr.parse()?;
    let quic_listener = QuicListener::new(localup_addr_parsed, quic_config)?;

    info!(
        "🔌 Tunnel control listening on {} (QUIC with TLS 1.3)",
        localup_addr
    );
    info!("🔐 All tunnel traffic is encrypted end-to-end");

    let localup_handle = tokio::spawn(async move {
        info!("🎯 QUIC accept loop started, waiting for connections...");
        loop {
            match quic_listener.accept().await {
                Ok((connection, peer_addr)) => {
                    info!("🔗 New tunnel connection from {}", peer_addr);
                    let handler = localup_handler.clone();
                    let conn = Arc::new(connection);
                    tokio::spawn(async move {
                        handler.handle_connection(conn, peer_addr).await;
                    });
                }
                Err(e) => {
                    error!("❌ Failed to accept QUIC connection: {}", e);
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
    });

    // Start API server for dashboard/management
    let api_handle = if !no_api {
        let api_addr_parsed: SocketAddr = api_addr.parse()?;
        let api_localup_manager = localup_manager.clone();
        let api_db = db.clone();
        let api_allow_signup = allow_signup;

        info!("Starting API server on {}", api_addr_parsed);
        info!("OpenAPI spec: http://{}/api/openapi.json", api_addr_parsed);
        info!("Swagger UI: http://{}/swagger-ui", api_addr_parsed);

        Some(tokio::spawn(async move {
            use localup_api::{ApiServer, ApiServerConfig};

            let config = ApiServerConfig {
                bind_addr: api_addr_parsed,
                enable_cors: true,
                cors_origins: Some(vec![
                    "http://localhost:5173".to_string(),
                    "http://127.0.0.1:5173".to_string(),
                    "http://localhost:3000".to_string(),
                    "http://127.0.0.1:3000".to_string(),
                    "http://localhost:3001".to_string(),
                    "http://127.0.0.1:3001".to_string(),
                    "http://localhost:3002".to_string(),
                    "http://127.0.0.1:3002".to_string(),
                ]),
            };

            let server = ApiServer::new(config, api_localup_manager, api_db, api_allow_signup);
            if let Err(e) = server.start().await {
                error!("API server error: {}", e);
            }
        }))
    } else {
        info!("API server disabled (--no-api flag)");
        None
    };

    info!("✅ Tunnel exit node is running");
    info!("Ready to accept incoming connections");
    if !http_addr.is_empty() {
        info!("  - HTTP traffic: {}", http_addr);
    }
    if let Some(ref https_addr) = https_addr {
        info!("  - HTTPS traffic: {}", https_addr);
    }
    info!("  - Tunnel control: {}", localup_addr);
    info!("Press Ctrl+C to stop");

    // Wait for shutdown signal
    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            info!("Shutdown signal received, stopping servers...");
        }
        Err(err) => {
            error!("Error listening for shutdown signal: {}", err);
        }
    }

    // Graceful shutdown
    if let Some(handle) = http_handle {
        handle.abort();
    }
    if let Some(handle) = https_handle {
        handle.abort();
    }
    if let Some(handle) = api_handle {
        handle.abort();
    }
    localup_handle.abort();
    info!("✅ Tunnel exit node stopped");

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_agent_server_command(
    listen: String,
    cert: Option<String>,
    key: Option<String>,
    jwt_secret: Option<String>,
    relay_addr: Option<String>,
    relay_id: Option<String>,
    relay_token: Option<String>,
    target_address: Option<String>,
    verbose: bool,
) -> Result<()> {
    use localup_agent_server::{AccessControl, AgentServer, AgentServerConfig, RelayConfig};
    use std::net::SocketAddr;

    // Initialize tracing with appropriate level
    let filter = if verbose {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "localup_agent_server=debug,localup_agent=debug".into())
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "localup_agent_server=info,localup_agent=info".into())
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse relay configuration if provided
    let relay_config = if let Some(relay_addr_str) = &relay_addr {
        let relay_id = match &relay_id {
            Some(id) => id.clone(),
            None => {
                return Err(anyhow::anyhow!(
                    "Relay ID (--relay-id) is required when relay address (--relay-addr) is set"
                ));
            }
        };

        let target_address = match &target_address {
            Some(addr) => addr.clone(),
            None => {
                return Err(anyhow::anyhow!(
                    "Target address (--target-address) is required when relay address (--relay-addr) is set"
                ));
            }
        };

        match relay_addr_str.parse::<SocketAddr>() {
            Ok(relay_addr) => {
                tracing::info!("🔄 Relay server enabled: {}", relay_addr);
                tracing::info!("Server ID on relay: {}", relay_id);
                tracing::info!("Backend target address: {}", target_address);
                if relay_token.is_some() {
                    tracing::info!("✅ Relay authentication enabled");
                } else {
                    tracing::warn!("⚠️  No relay authentication token");
                }
                Some(RelayConfig {
                    relay_addr,
                    server_id: relay_id,
                    target_address,
                    relay_token,
                })
            }
            Err(e) => {
                tracing::error!("Failed to parse relay address '{}': {}", relay_addr_str, e);
                return Err(anyhow::anyhow!("Invalid relay address: {}", e));
            }
        }
    } else if relay_id.is_some() {
        return Err(anyhow::anyhow!(
            "Relay address (--relay-addr) is required when relay ID (--relay-id) is set"
        ));
    } else {
        None
    };

    // Parse listen address
    let listen_addr: SocketAddr = listen.parse().context("Failed to parse listen address")?;

    // Create access control (no CIDR/port restrictions for now from CLI)
    let access_control = AccessControl::new(vec![], vec![]);

    // Create server config
    let config = AgentServerConfig {
        listen_addr,
        cert_path: cert,
        key_path: key,
        access_control,
        jwt_secret,
        relay_config,
    };

    // Create and run server
    let server = AgentServer::new(config)?;
    server.run().await?;

    Ok(())
}

async fn handle_generate_token_command(
    secret: String,
    sub: Option<String>,
    hours: i64,
    reverse_tunnel: bool,
    allowed_agents: Vec<String>,
    allowed_addresses: Vec<String>,
    token_only: bool,
) -> Result<()> {
    use chrono::Duration;
    use localup_auth::{JwtClaims, JwtValidator};
    use uuid::Uuid;

    // Generate a unique subject if not provided, or use the provided one
    let subject = sub.unwrap_or_else(|| Uuid::new_v4().to_string());

    // Create claims with the specified validity period
    let mut claims = JwtClaims::new(
        subject.clone(),
        "localup-relay".to_string(),
        "localup-client".to_string(),
        Duration::hours(hours),
    );

    // Add reverse tunnel configuration if enabled
    if reverse_tunnel {
        claims = claims.with_reverse_tunnel(true);
        if !allowed_agents.is_empty() {
            claims = claims.with_allowed_agents(allowed_agents.clone());
        }
        if !allowed_addresses.is_empty() {
            claims = claims.with_allowed_addresses(allowed_addresses.clone());
        }
    }

    // Encode the token
    let token = JwtValidator::encode(secret.as_bytes(), &claims)?;

    // Output token only if requested (useful for scripts)
    if token_only {
        println!("{}", token);
    } else {
        // Display the token with details
        println!();
        println!("✅ JWT Token generated successfully!");
        println!();
        println!("Token: {}", token);
        println!();
        println!("Token details:");
        println!("  - Subject: {}", subject);
        println!("  - Expires in: {} hour(s)", hours);
        println!("  - Expires at: {}", claims.exp_formatted());
        println!(
            "  - Reverse tunnel: {}",
            if reverse_tunnel {
                "enabled"
            } else {
                "disabled"
            }
        );

        if reverse_tunnel {
            if let Some(ref agents) = claims.allowed_agents {
                println!("  - Allowed agents: {}", agents.join(", "));
            } else {
                println!("  - Allowed agents: all");
            }
            if let Some(ref addrs) = claims.allowed_addresses {
                println!("  - Allowed addresses: {}", addrs.join(", "));
            } else {
                println!("  - Allowed addresses: all");
            }
        }
        println!();
        println!("Use this token in your client configuration:");
        println!("  localup --token {}", token);
        println!();
    }

    Ok(())
}

async fn handle_config_command(command: &ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::SetToken { token } => {
            config::ConfigManager::set_token(token.clone())?;
            println!("✅ Auth token saved successfully!");
            println!("   Token stored in: ~/.localup/config.json");
            println!();
            println!("You can now use 'localup' without specifying --token every time:");
            println!("   localup --port 3000 --protocol http");
            Ok(())
        }
        ConfigCommands::GetToken => match config::ConfigManager::get_token()? {
            Some(token) => {
                println!("📌 Current auth token:");
                println!("{}", token);
                Ok(())
            }
            None => {
                println!("❌ No auth token configured");
                println!();
                println!("Set a token with:");
                println!("   localup config set-token <TOKEN>");
                Ok(())
            }
        },
        ConfigCommands::ClearToken => {
            config::ConfigManager::clear_token()?;
            println!("✅ Auth token cleared successfully!");
            Ok(())
        }
    }
}

fn init_logging(log_level: &str) -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new(log_level))
        .context("Failed to initialize logging filter")?;

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    Ok(())
}

// TCP Port Allocator and related types (for handle_relay_command)
use chrono::{DateTime, Utc};
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
    allocated_ports: Mutex<HashMap<String, PortAllocation>>, // localup_id -> allocation
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

    /// Generate a deterministic port number from localup_id hash
    /// This ensures the same localup_id always gets the same port (if available)
    fn hash_to_port(&self, localup_id: &str) -> u16 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        localup_id.hash(&mut hasher);
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
            .filter_map(|(localup_id, allocation)| match &allocation.state {
                AllocationState::Reserved { until } if *until < now => Some(localup_id.clone()),
                _ => None,
            })
            .collect();

        if !expired.is_empty() {
            info!(
                "🧹 Cleanup check: found {} expired port reservations",
                expired.len()
            );
        }

        for localup_id in expired {
            if let Some(allocation) = allocated.remove(&localup_id) {
                available.insert(allocation.port);
                info!(
                    "✅ Cleaned up expired port reservation for tunnel {} (port {})",
                    localup_id, allocation.port
                );
            }
        }

        // Log current allocation status
        let active_count = allocated
            .values()
            .filter(|a| matches!(a.state, AllocationState::Active))
            .count();
        let reserved_count = allocated
            .values()
            .filter(|a| matches!(a.state, AllocationState::Reserved { .. }))
            .count();
        if active_count > 0 || reserved_count > 0 {
            debug!(
                "Port allocator status: {} active, {} reserved, {} available",
                active_count,
                reserved_count,
                available.len()
            );
        }
    }
}

impl localup_control::PortAllocator for PortAllocator {
    fn allocate(&self, localup_id: &str, requested_port: Option<u16>) -> Result<u16, String> {
        let mut available = self.available_ports.lock().unwrap();
        let mut allocated = self.allocated_ports.lock().unwrap();

        // Check if already allocated (active or reserved)
        if let Some(allocation) = allocated.get(localup_id) {
            let port = allocation.port;
            // Reactivate if it was reserved
            if matches!(allocation.state, AllocationState::Reserved { .. }) {
                info!(
                    "Reusing reserved port {} for reconnecting tunnel {}",
                    port, localup_id
                );
                allocated.insert(
                    localup_id.to_string(),
                    PortAllocation {
                        port,
                        state: AllocationState::Active,
                    },
                );
            }
            return Ok(port);
        }

        // If user requested a specific port, try to allocate it
        if let Some(req_port) = requested_port {
            if available.contains(&req_port) && Self::is_port_available(req_port) {
                // Requested port is available!
                available.remove(&req_port);
                allocated.insert(
                    localup_id.to_string(),
                    PortAllocation {
                        port: req_port,
                        state: AllocationState::Active,
                    },
                );
                info!(
                    "✅ Allocated requested port {} for tunnel {}",
                    req_port, localup_id
                );
                return Ok(req_port);
            } else if available.contains(&req_port) && !Self::is_port_available(req_port) {
                // Port in our pool but in use by another process
                available.remove(&req_port);
                return Err(format!(
                    "Requested port {} is already allocated to another tunnel",
                    req_port
                ));
            } else {
                // Port not in our allocation range
                return Err(format!(
                    "Requested port {} is outside the configured port range ({}-{})",
                    req_port, self.range_start, self.range_end
                ));
            }
        }

        // No specific port requested, try to allocate deterministic port based on localup_id hash
        let preferred_port = self.hash_to_port(localup_id);

        if available.contains(&preferred_port) && Self::is_port_available(preferred_port) {
            // Preferred port is available in our tracking AND at OS level!
            available.remove(&preferred_port);
            allocated.insert(
                localup_id.to_string(),
                PortAllocation {
                    port: preferred_port,
                    state: AllocationState::Active,
                },
            );
            info!(
                "🎯 Allocated deterministic port {} for tunnel {} (hash-based)",
                preferred_port, localup_id
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
                            localup_id.to_string(),
                            PortAllocation {
                                port,
                                state: AllocationState::Active,
                            },
                        );
                        info!(
                            "Allocated nearby port {} for tunnel {} (preferred {} was taken)",
                            port, localup_id, preferred_port
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
                    localup_id.to_string(),
                    PortAllocation {
                        port,
                        state: AllocationState::Active,
                    },
                );
                info!(
                    "Allocated fallback port {} for tunnel {} (preferred {} was taken)",
                    port, localup_id, preferred_port
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

    fn deallocate(&self, localup_id: &str) {
        let mut allocated = self.allocated_ports.lock().unwrap();

        // Instead of immediately freeing, mark as reserved for reconnection
        if let Some(allocation) = allocated.get_mut(localup_id) {
            if matches!(allocation.state, AllocationState::Active) {
                let until = Utc::now() + chrono::Duration::seconds(self.reservation_ttl_seconds);
                allocation.state = AllocationState::Reserved { until };
                info!(
                    "⏱️  Port {} for tunnel {} marked as reserved until {} (TTL: {}s, will be cleaned after this timeout)",
                    allocation.port,
                    localup_id,
                    until.format("%Y-%m-%d %H:%M:%S"),
                    self.reservation_ttl_seconds
                );
            } else {
                debug!(
                    "Tunnel {} port already in {} state, skipping deallocate",
                    localup_id,
                    match &allocation.state {
                        AllocationState::Active => "Active",
                        AllocationState::Reserved { .. } => "Reserved",
                    }
                );
            }
        } else {
            warn!(
                "Tried to deallocate port for tunnel {} but allocation not found!",
                localup_id
            );
        }
    }

    fn get_allocated_port(&self, localup_id: &str) -> Option<u16> {
        self.allocated_ports
            .lock()
            .unwrap()
            .get(localup_id)
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
