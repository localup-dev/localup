//! Tunnel CLI - Command-line interface for creating tunnels

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use tunnel_cli::{daemon, service, tunnel_store};
use tunnel_client::{ExitNodeConfig, MetricsServer, ProtocolConfig, TunnelClient, TunnelConfig};

/// Tunnel CLI - Expose local servers to the internet
#[derive(Parser, Debug)]
#[command(name = "localup")]
#[command(about = "Expose local servers through secure tunnels", long_about = None)]
#[command(version = env!("GIT_TAG"))]
#[command(long_version = concat!(env!("GIT_TAG"), "\nCommit: ", env!("GIT_HASH"), "\nBuilt: ", env!("BUILD_TIME")))]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Local port to expose (standalone mode)
    #[arg(short, long, global = true)]
    port: Option<u16>,

    /// Protocol to use (http, https, tcp, tls) (standalone mode)
    #[arg(long, global = true)]
    protocol: Option<String>,

    /// Authentication token / JWT secret (standalone mode)
    #[arg(short, long, env = "TUNNEL_AUTH_TOKEN", global = true)]
    token: Option<String>,

    /// Subdomain for HTTP/HTTPS tunnels (standalone mode)
    #[arg(short, long, global = true)]
    subdomain: Option<String>,

    /// Custom domain for HTTPS tunnels (standalone mode)
    #[arg(long, global = true)]
    domain: Option<String>,

    /// Relay server address (standalone mode)
    #[arg(short, long, env, global = true)]
    relay: Option<String>,

    /// Remote port for TCP/TLS tunnels (standalone mode)
    #[arg(long, global = true)]
    remote_port: Option<u16>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info", global = true)]
    log_level: String,

    /// Port for metrics web dashboard
    #[arg(long, default_value = "9090", global = true)]
    metrics_port: u16,

    /// Disable metrics collection and web dashboard
    #[arg(long, global = true)]
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
        port: u16,
        /// Protocol (http, https, tcp, tls)
        #[arg(long, default_value = "http")]
        protocol: String,
        /// Authentication token
        #[arg(short, long, env = "TUNNEL_AUTH_TOKEN")]
        token: String,
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
            protocol,
            token,
            subdomain,
            domain,
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
    port: u16,
    protocol: String,
    token: String,
    subdomain: Option<String>,
    domain: Option<String>,
    relay: Option<String>,
    remote_port: Option<u16>,
    enabled: bool,
) -> Result<()> {
    let store = tunnel_store::TunnelStore::new()?;

    // Parse protocol
    let protocol_config = parse_protocol(&protocol, port, subdomain, domain, remote_port)?;

    // Parse exit node
    let exit_node = if let Some(relay_addr) = relay {
        validate_relay_addr(&relay_addr)?;
        ExitNodeConfig::Custom(relay_addr)
    } else {
        ExitNodeConfig::Auto
    };

    // Create tunnel config
    let config = TunnelConfig {
        local_host: "localhost".to_string(),
        protocols: vec![protocol_config],
        auth_token: token,
        exit_node,
        failover: true,
        connection_timeout: Duration::from_secs(30),
    };

    let stored_tunnel = tunnel_store::StoredTunnel {
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
    let store = tunnel_store::TunnelStore::new()?;
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
                    custom_domain,
                } => {
                    println!("    Protocol: HTTPS, Port: {}", local_port);
                    if let Some(sub) = subdomain {
                        println!("    Subdomain: {}", sub);
                    }
                    if let Some(domain) = custom_domain {
                        println!("    Custom domain: {}", domain);
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
                    subdomain,
                    remote_port,
                } => {
                    print!("    Protocol: TLS, Port: {}", local_port);
                    if let Some(sub) = subdomain {
                        print!(", Subdomain: {}", sub);
                    }
                    if let Some(remote) = remote_port {
                        print!(", Remote: {}", remote);
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
    let store = tunnel_store::TunnelStore::new()?;
    let tunnel = store.load(&name)?;
    let json = serde_json::to_string_pretty(&tunnel)?;
    println!("{}", json);
    Ok(())
}

fn handle_remove_tunnel(name: String) -> Result<()> {
    let store = tunnel_store::TunnelStore::new()?;
    store.remove(&name)?;
    println!("✅ Tunnel '{}' removed", name);
    Ok(())
}

fn handle_enable_tunnel(name: String) -> Result<()> {
    let store = tunnel_store::TunnelStore::new()?;
    store.enable(&name)?;
    println!("✅ Tunnel '{}' enabled (will auto-start with daemon)", name);
    Ok(())
}

fn handle_disable_tunnel(name: String) -> Result<()> {
    let store = tunnel_store::TunnelStore::new()?;
    store.disable(&name)?;
    println!("✅ Tunnel '{}' disabled", name);
    Ok(())
}

async fn run_standalone(cli: Cli) -> Result<()> {
    // Check if required arguments are present for standalone mode
    if cli.port.is_none() || cli.token.is_none() {
        eprintln!("Error: Standalone mode requires --port and --token arguments");
        eprintln!();
        eprintln!("Usage:");
        eprintln!("  localup --port <PORT> --protocol <PROTOCOL> --token <TOKEN>");
        eprintln!();
        eprintln!("Or use tunnel management commands:");
        eprintln!("  localup add <name> --port <PORT> --token <TOKEN>");
        eprintln!("  localup daemon start");
        eprintln!("  localup service install");
        eprintln!();
        eprintln!("For more help, run: localup --help");
        std::process::exit(1);
    }

    let port = cli.port.unwrap();
    let token = cli.token.unwrap();
    let protocol_str = cli.protocol.unwrap_or_else(|| "http".to_string());

    info!("🚀 Tunnel CLI starting (standalone mode)...");
    info!("Protocol: {}", protocol_str);
    info!("Local port: {}", port);

    // Parse protocol configuration
    let protocol = parse_protocol(
        &protocol_str,
        port,
        cli.subdomain.clone(),
        cli.domain.clone(),
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
        local_host: "localhost".to_string(),
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
            tokio::time::sleep(tokio::time::Duration::from_secs(backoff_seconds)).await;
        }

        // Check if user pressed Ctrl+C during backoff
        if cancel_rx.try_recv().is_ok() {
            info!("Shutdown requested, exiting...");
            break;
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
                    println!("📍 Local:  http://localhost:{}", port);
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
                    let local_upstream = format!("http://localhost:{}", port);

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
                        tunnel_client::TunnelError::AuthenticationFailed(reason) => {
                            error!("   Authentication failed: {}", reason);
                            error!("   Token provided: {}", token);
                            error!("   Please check your authentication token and try again.");
                        }
                        tunnel_client::TunnelError::ConfigError(reason) => {
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
    domain: Option<String>,
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
            custom_domain: domain,
        }),
        "tcp" => Ok(ProtocolConfig::Tcp {
            local_port: port,
            remote_port,
        }),
        "tls" => Ok(ProtocolConfig::Tls {
            local_port: port,
            subdomain,
            remote_port,
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
