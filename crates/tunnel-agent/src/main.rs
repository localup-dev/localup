//! LocalUp Agent - Reverse tunnel agent for forwarding traffic to a specific remote address
//!
//! The agent connects to a relay server and forwards incoming requests to a single
//! specific target address (e.g., "192.168.1.100:8080").
//!
//! # Example Usage
//!
//! ```bash
//! # Run agent with default localhost relay
//! localup-agent --token YOUR_TOKEN --target-address "192.168.1.100:8080"
//!
//! # Run agent with custom relay
//! localup-agent \
//!   --relay relay.example.com:4443 \
//!   --token YOUR_TOKEN \
//!   --target-address "10.0.0.5:3000"
//!
//! # Run agent in insecure mode (development only)
//! localup-agent --insecure --token YOUR_TOKEN --target-address "localhost:8080"
//! ```

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use tracing::{error, info};
use tunnel_agent::{Agent, AgentConfig};

/// LocalUp Agent - Reverse tunnel agent for secure access to a specific private address
#[derive(Parser, Debug)]
#[command(
    name = "localup-agent",
    about = "Reverse tunnel agent for forwarding traffic to a specific remote address",
    version,
    long_about = "The LocalUp agent connects to a relay server and forwards incoming requests \
                  to a single specific target address (e.g., '192.168.1.100:8080'). \
                  This provides secure access to a private service through the relay."
)]
struct Args {
    /// Agent ID (auto-generated if not provided)
    #[arg(long, env = "LOCALUP_AGENT_ID")]
    agent_id: Option<String>,

    /// Relay server address (host:port)
    #[arg(long, env = "LOCALUP_RELAY_ADDR", default_value = "localhost:4443")]
    relay: String,

    /// Authentication token for the relay
    #[arg(long, env = "LOCALUP_AUTH_TOKEN")]
    token: String,

    /// Target address to forward traffic to (host:port)
    ///
    /// This agent will ONLY forward traffic to this specific address.
    /// Example: "192.168.1.100:8080" or "localhost:3000"
    #[arg(long, env = "LOCALUP_TARGET_ADDRESS")]
    target_address: String,

    /// Skip TLS certificate verification (INSECURE - development only)
    ///
    /// WARNING: This makes the connection vulnerable to man-in-the-middle attacks.
    /// Only use for local development with self-signed certificates.
    #[arg(long, env = "LOCALUP_INSECURE")]
    insecure: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, env = "RUST_LOG", default_value = "info")]
    log_level: String,

    /// Configuration file (YAML format)
    ///
    /// If provided, configuration from the file is merged with CLI arguments.
    /// CLI arguments take precedence over file configuration.
    #[arg(long, env = "LOCALUP_CONFIG_FILE")]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse arguments
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(&args.log_level)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    info!("Starting LocalUp Agent");

    // Load configuration
    let config = load_config(args)?;

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
        tracing::warn!("⚠️  Running in INSECURE mode - certificate verification is DISABLED");
        tracing::warn!("⚠️  This should ONLY be used for local development");
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

/// Load configuration from CLI args and optional config file
fn load_config(args: Args) -> Result<AgentConfig> {
    // TODO: If config file is provided, load and merge with CLI args
    // For now, just use CLI args

    let agent_id = args
        .agent_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    Ok(AgentConfig {
        agent_id,
        relay_addr: args.relay,
        auth_token: args.token,
        target_address: args.target_address,
        insecure: args.insecure,
    })
}
