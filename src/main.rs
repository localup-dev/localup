//! LocalUp CLI - Simple tunnel management tool
//!
//! Connect your local services to remote relays with automatic reconnection.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::time::Duration;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use tunnel_agent::{Agent, AgentConfig};

/// LocalUp - Tunnel your local services through remote relays
#[derive(Parser, Debug)]
#[command(name = "localup")]
#[command(about = "LocalUp - Tunnel your local services through remote relays")]
#[command(version)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Connect to a relay and forward traffic to a remote address
    #[command(long_about = r#"
Connect to a relay server and forward incoming traffic to a specific
remote address. Automatically reconnects if the connection drops.

EXAMPLES:
  # Connect to relay and forward to local PostgreSQL
  localup connect --relay 127.0.0.1:4443 \
    --token $AGENT_TOKEN \
    --agent-id "prod-db-agent" \
    --remote-address "127.0.0.1:5432"

  # Expose local port that forwards to remote address
  localup connect --relay 127.0.0.1:4443 \
    --token $TOKEN \
    --local-address "0.0.0.0:5433" \
    --remote-address "192.168.1.100:5432"

ENVIRONMENT VARIABLES:
  LOCALUP_RELAY          Relay server address
  LOCALUP_TOKEN          Authentication token
  LOCALUP_AGENT_ID       Agent identifier
  LOCALUP_LOCAL_ADDRESS  Local address to bind (optional)
  LOCALUP_REMOTE_ADDRESS Target address to forward to
    "#)]
    Connect {
        /// Relay server address (e.g., relay.example.com:4443)
        #[arg(long, env = "LOCALUP_RELAY")]
        relay: String,

        /// Authentication token
        #[arg(long, env = "LOCALUP_TOKEN")]
        token: String,

        /// Agent ID (auto-generated if not specified)
        #[arg(long, env = "LOCALUP_AGENT_ID")]
        agent_id: Option<String>,

        /// Local address to bind and listen (e.g., 0.0.0.0:5433)
        /// If specified, incoming connections will be forwarded to remote-address
        #[arg(long, env = "LOCALUP_LOCAL_ADDRESS")]
        local_address: Option<String>,

        /// Remote address to forward connections to (e.g., 127.0.0.1:5432)
        #[arg(long, env = "LOCALUP_REMOTE_ADDRESS")]
        remote_address: String,

        /// Skip certificate verification (insecure, for development only)
        #[arg(long)]
        insecure: bool,

        /// Maximum reconnection attempts (0 = infinite)
        #[arg(long, default_value = "0")]
        max_reconnect_attempts: usize,

        /// Initial reconnection delay in seconds
        #[arg(long, default_value = "1")]
        reconnect_delay: u64,

        /// Maximum reconnection delay in seconds
        #[arg(long, default_value = "60")]
        max_reconnect_delay: u64,
    },
}

/// Setup logging with the specified log level
fn setup_logging(verbose: bool) {
    let log_level = if verbose { "debug" } else { "info" };

    let filter = EnvFilter::try_new(log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(true).with_thread_ids(false))
        .with(filter)
        .init();
}

/// Connect to relay with automatic reconnection
async fn connect_with_reconnect(
    config: AgentConfig,
    max_attempts: usize,
    initial_delay: Duration,
    max_delay: Duration,
) -> Result<()> {
    let mut attempt = 0;
    let mut current_delay = initial_delay;

    // Create agent once for local listener startup
    let agent = Agent::new(config.clone()).context("Failed to create agent")?;

    // Start local listener if configured (once, persists across reconnects)
    // The listener needs the agent's running flag to stay true
    let listener_handle = agent
        .start_local_listener()
        .await
        .context("Failed to start local listener")?;

    // If listener is configured, mark agent as "running" to keep listener alive
    if listener_handle.is_some() {
        // Set running flag to true so the listener task doesn't exit immediately
        // The listener stays alive independently of relay connection status
        let mut running = agent.running.lock().await;
        *running = true;
        drop(running);

        info!("Local listener is now listening for connections");
    }

    loop {
        attempt += 1;

        if max_attempts > 0 && attempt > max_attempts {
            error!("Maximum reconnection attempts ({}) reached", max_attempts);
            anyhow::bail!("Failed to connect after {} attempts", max_attempts);
        }

        info!(
            "Connection attempt {} (max: {})",
            attempt,
            if max_attempts == 0 {
                "∞".to_string()
            } else {
                max_attempts.to_string()
            }
        );

        // Create new agent for each connection attempt (but reuse the local listener)
        let mut agent = Agent::new(config.clone()).context("Failed to create agent")?;

        // Try to start the agent (relay connection only)
        match agent.start().await {
            Ok(()) => {
                // Agent stopped normally (e.g., Ctrl+C)
                info!("Agent stopped normally");
                return Ok(());
            }
            Err(e) => {
                error!("Agent error: {:#}", e);

                // Check if we should retry
                if max_attempts > 0 && attempt >= max_attempts {
                    return Err(e.into());
                }

                // Wait before reconnecting
                warn!(
                    "Reconnecting in {} seconds... (attempt {} of {})",
                    current_delay.as_secs(),
                    attempt + 1,
                    if max_attempts == 0 {
                        "∞".to_string()
                    } else {
                        max_attempts.to_string()
                    }
                );

                tokio::time::sleep(current_delay).await;

                // Exponential backoff with max cap
                current_delay = std::cmp::min(current_delay * 2, max_delay);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    setup_logging(cli.verbose);

    match cli.command {
        Commands::Connect {
            relay,
            token,
            agent_id,
            local_address,
            remote_address,
            insecure,
            max_reconnect_attempts,
            reconnect_delay,
            max_reconnect_delay,
        } => {
            info!("LocalUp starting...");

            // Generate agent ID if not provided
            let agent_id = agent_id.unwrap_or_else(|| {
                let id = format!("agent-{}", uuid::Uuid::new_v4());
                info!("Auto-generated agent ID: {}", id);
                id
            });

            // Build agent configuration
            let config = AgentConfig {
                agent_id: agent_id.clone(),
                relay_addr: relay.clone(),
                auth_token: token,
                target_address: remote_address.clone(),
                local_address: local_address.clone(),
                insecure,
            };

            // Log configuration
            info!("Agent ID: {}", agent_id);
            info!("Relay: {}", relay);
            if let Some(ref local_addr) = local_address {
                info!("Local address: {} (forwarding to remote)", local_addr);
            }
            info!("Remote address: {}", remote_address);
            if insecure {
                warn!("⚠️  Certificate verification disabled (insecure mode)");
            }

            // Setup Ctrl+C handler
            let ctrl_c = tokio::signal::ctrl_c();
            tokio::pin!(ctrl_c);

            // Start connection with reconnection
            let connect_task = tokio::spawn(connect_with_reconnect(
                config,
                max_reconnect_attempts,
                Duration::from_secs(reconnect_delay),
                Duration::from_secs(max_reconnect_delay),
            ));

            // Wait for Ctrl+C or connection error
            tokio::select! {
                _ = &mut ctrl_c => {
                    info!("Received Ctrl+C, shutting down...");
                }
                result = connect_task => {
                    match result {
                        Ok(Ok(())) => {
                            info!("Connection stopped normally");
                        }
                        Ok(Err(e)) => {
                            error!("Connection error: {:#}", e);
                            return Err(e);
                        }
                        Err(e) => {
                            error!("Connection task panicked: {}", e);
                            return Err(e.into());
                        }
                    }
                }
            }

            info!("LocalUp stopped");
            Ok(())
        }
    }
}
