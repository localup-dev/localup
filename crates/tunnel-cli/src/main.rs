//! Tunnel CLI - Command-line interface for creating tunnels

use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use tunnel_client::{ExitNodeConfig, MetricsServer, ProtocolConfig, TunnelClient, TunnelConfig};

/// Tunnel CLI - Expose local servers to the internet
#[derive(Parser, Debug)]
#[command(name = "tunnel")]
#[command(about = "Expose local servers through secure tunnels", long_about = None)]
#[command(version)]
struct Cli {
    /// Local port to expose
    #[arg(short, long)]
    port: u16,

    /// Protocol to use (http, https, tcp, tls)
    #[arg(long, default_value = "http")]
    protocol: String,

    /// Authentication token / JWT secret
    #[arg(short, long, env = "TUNNEL_AUTH_TOKEN")]
    token: String,

    /// Subdomain for HTTP/HTTPS tunnels
    #[arg(short, long)]
    subdomain: Option<String>,

    /// Custom domain for HTTPS tunnels
    #[arg(long)]
    domain: Option<String>,

    /// Relay server address in format host:port (e.g., "localhost:8080" or "relay.example.com:8080")
    /// Supports both hostnames and IP addresses. If not specified, uses automatic relay selection.
    #[arg(short, long, env)]
    relay: Option<String>,

    /// Remote port for TCP/TLS tunnels
    #[arg(long)]
    remote_port: Option<u16>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Port for metrics web dashboard (default: 9090)
    #[arg(long, default_value = "9090")]
    metrics_port: u16,

    /// Disable metrics collection and web dashboard
    #[arg(long)]
    no_metrics: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize rustls crypto provider (required for QUIC/TLS)
    rustls::crypto::CryptoProvider::install_default(rustls::crypto::ring::default_provider())
        .unwrap();

    let cli = Cli::parse();

    // Initialize logging
    init_logging(&cli.log_level)?;

    info!("🚀 Tunnel CLI starting...");
    info!("Protocol: {}", cli.protocol);
    info!("Local port: {}", cli.port);

    // Parse protocol configuration
    let protocol = match cli.protocol.to_lowercase().as_str() {
        "http" => ProtocolConfig::Http {
            local_port: cli.port,
            subdomain: cli.subdomain.clone(),
        },
        "https" => ProtocolConfig::Https {
            local_port: cli.port,
            subdomain: cli.subdomain.clone(),
            custom_domain: cli.domain.clone(),
        },
        "tcp" => ProtocolConfig::Tcp {
            local_port: cli.port,
            remote_port: cli.remote_port,
        },
        "tls" => ProtocolConfig::Tls {
            local_port: cli.port,
            subdomain: cli.subdomain.clone(),
            remote_port: cli.remote_port,
        },
        _ => {
            error!(
                "Invalid protocol: {}. Valid options: http, https, tcp, tls",
                cli.protocol
            );
            std::process::exit(1);
        }
    };

    // Parse exit node configuration
    let exit_node = if let Some(relay_addr) = cli.relay {
        info!("Using custom relay: {}", relay_addr);

        // Validate the address format (supports both IP:port and hostname:port)
        if !relay_addr.contains(':') {
            return Err(anyhow::anyhow!(
                "Invalid relay address: {}. Expected format: host:port or ip:port",
                relay_addr
            ));
        }

        // Try to parse as SocketAddr (IP:port) first
        if relay_addr.parse::<SocketAddr>().is_err() {
            // If that fails, validate it looks like hostname:port
            let parts: Vec<&str> = relay_addr.split(':').collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!(
                    "Invalid relay address: {}. Expected format: host:port",
                    relay_addr
                ));
            }
            if parts[1].parse::<u16>().is_err() {
                return Err(anyhow::anyhow!(
                    "Invalid port in relay address: {}",
                    relay_addr
                ));
            }
            // Hostname:port format is valid
        }

        ExitNodeConfig::Custom(relay_addr)
    } else {
        info!("Using automatic relay selection");
        ExitNodeConfig::Auto
    };

    // Build tunnel configuration
    let config = TunnelConfig::builder()
        .local_host("localhost".to_string())
        .protocol(protocol)
        .auth_token(cli.token.clone())
        .exit_node(exit_node)
        .build()
        .map_err(|e| anyhow::anyhow!("Configuration error: {}", e))?;

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
                    println!("📍 Local:  http://localhost:{}", cli.port);
                    println!("🌐 Public: {}", url);
                    println!();
                }

                // Start metrics server if enabled (only once)
                if !cli.no_metrics && !metrics_server_started {
                    let metrics = client.metrics().clone();
                    let endpoints = client.endpoints().to_vec();
                    let metrics_addr: SocketAddr = format!("127.0.0.1:{}", cli.metrics_port)
                        .parse()
                        .expect("Invalid metrics port");

                    // Local upstream URL for replay functionality
                    let local_upstream = format!("http://localhost:{}", cli.port);

                    tokio::spawn(async move {
                        let server =
                            MetricsServer::new(metrics_addr, metrics, endpoints, local_upstream);
                        if let Err(e) = server.run().await {
                            error!("Metrics server error: {}", e);
                        }
                    });

                    println!(
                        "📊 Metrics dashboard: http://127.0.0.1:{}",
                        cli.metrics_port
                    );
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
                        // The control stream task will send Disconnect, wait for Disconnect Ack,
                        // and then exit, which will cause wait_task to complete
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
                use tunnel_client::TunnelError;
                if e.is_non_recoverable() {
                    error!("🚫 Non-recoverable error detected.");

                    // Provide specific guidance based on error type
                    match &e {
                        TunnelError::AuthenticationFailed(reason) => {
                            error!("   Authentication failed: {}", reason);
                            error!("   Token provided: {}", cli.token);
                            error!("   Please check your authentication token and try again.");
                        }
                        TunnelError::ConfigError(reason) => {
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

                // No maximum retry limit for recoverable errors - use exponential backoff indefinitely
                // Continue to next iteration for retry
            }
        }
    }

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
