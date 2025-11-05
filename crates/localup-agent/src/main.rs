//! LocalUp Agent - Reverse tunnel agent CLI
//!
//! This binary provides a command-line interface for running the LocalUp tunnel agent,
//! which forwards connections from a relay server to a specific target address.

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use tunnel_agent::{Agent, AgentConfig};
use uuid::Uuid;

/// LocalUp reverse tunnel agent - forwards connections to a specific target address
#[derive(Parser, Debug)]
#[command(name = "localup-agent")]
#[command(
    about = "LocalUp reverse tunnel agent - forwards connections to a specific target address"
)]
#[command(version)]
#[command(long_about = r#"
LocalUp Agent connects to a relay server and forwards incoming requests
to a specific target address (host:port) in your private network.

EXAMPLES:
  # Start agent with basic configuration
  localup-agent --relay relay.example.com:4443 \
    --auth-token $TOKEN \
    --target-address 192.168.1.100:8080

  # Start agent using config file
  localup-agent --config agent-config.yaml

  # Start agent with custom log level
  localup-agent --config agent-config.yaml --log-level debug

ENVIRONMENT VARIABLES:
  LOCALUP_RELAY          Relay server address
  LOCALUP_AUTH_TOKEN     Authentication token (JWT)
  LOCALUP_AGENT_ID       Agent identifier
  LOCALUP_TARGET_ADDRESS Target address to forward to (host:port)
"#)]
struct Args {
    /// Relay server address (e.g., relay.example.com:4443)
    #[arg(long, env = "LOCALUP_RELAY")]
    relay: Option<String>,

    /// Authentication token (JWT)
    #[arg(long, env = "LOCALUP_AUTH_TOKEN")]
    auth_token: Option<String>,

    /// Target address to forward connections to (e.g., 192.168.1.100:8080)
    #[arg(long, env = "LOCALUP_TARGET_ADDRESS")]
    target_address: Option<String>,

    /// Agent ID (auto-generated if not specified)
    #[arg(long, env = "LOCALUP_AGENT_ID")]
    agent_id: Option<String>,

    /// Configuration file (YAML)
    #[arg(long, short = 'c')]
    config: Option<PathBuf>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Skip certificate verification (insecure, for development only)
    #[arg(long)]
    insecure: bool,
}

/// Configuration file format
#[derive(Debug, Serialize, Deserialize)]
struct ConfigFile {
    /// Relay server configuration
    relay: RelayConfig,

    /// Agent configuration
    #[serde(default)]
    agent: AgentConfigFile,
}

#[derive(Debug, Serialize, Deserialize)]
struct RelayConfig {
    /// Relay server address
    address: String,

    /// Environment variable name for auth token
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_token_env: Option<String>,

    /// Direct auth token (prefer using auth_token_env)
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_token: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AgentConfigFile {
    /// Agent ID
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,

    /// Target address to forward connections to (host:port)
    #[serde(skip_serializing_if = "Option::is_none")]
    target_address: Option<String>,
}

/// Setup logging with the specified log level
fn setup_logging(log_level: &str) -> Result<()> {
    let filter = EnvFilter::try_new(log_level)
        .with_context(|| format!("Invalid log level: {}", log_level))?;

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(true).with_thread_ids(false))
        .with(filter)
        .init();

    Ok(())
}

/// Load configuration from YAML file
fn load_config_file(path: &PathBuf) -> Result<ConfigFile> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let config: ConfigFile = serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

    Ok(config)
}

/// Merge CLI args with config file, giving precedence to CLI args
fn build_agent_config(args: Args) -> Result<AgentConfig> {
    let insecure = args.insecure;
    let (relay_addr, auth_token, mut target_address, agent_id) =
        if let Some(config_path) = &args.config {
            info!("Loading configuration from: {}", config_path.display());
            let config_file = load_config_file(config_path)?;

            // Get auth token from env var if specified
            let auth_token = if let Some(env_var) = &config_file.relay.auth_token_env {
                std::env::var(env_var)
                    .with_context(|| format!("Environment variable {} not set", env_var))?
            } else if let Some(token) = config_file.relay.auth_token {
                token
            } else {
                anyhow::bail!("No auth token specified in config file");
            };

            (
                config_file.relay.address,
                auth_token,
                config_file.agent.target_address,
                config_file.agent.id,
            )
        } else {
            // No config file, use CLI args
            (String::new(), String::new(), None, None)
        };

    // CLI args override config file
    let relay_addr = args.relay.unwrap_or(relay_addr);
    let auth_token = args.auth_token.unwrap_or(auth_token);

    if args.target_address.is_some() {
        target_address = args.target_address;
    }

    let agent_id = args.agent_id.or(agent_id).unwrap_or_else(|| {
        let id = format!("agent-{}", Uuid::new_v4());
        info!("Auto-generated agent ID: {}", id);
        id
    });

    // Validate configuration
    if relay_addr.is_empty() {
        anyhow::bail!("Relay address is required (use --relay or config file)");
    }

    if auth_token.is_empty() {
        anyhow::bail!(
            "Auth token is required (use --auth-token, environment variable, or config file)"
        );
    }

    let target_address = target_address.ok_or_else(|| {
        anyhow::anyhow!("Target address is required (use --target-address or config file)")
    })?;

    // Validate relay address format
    validate_address(&relay_addr, "relay")?;

    // Validate target address format
    validate_address(&target_address, "target")?;

    Ok(AgentConfig {
        agent_id,
        relay_addr,
        auth_token,
        target_address,
        local_address: None, // localup-agent does not support local listening
        insecure,
    })
}

/// Validate address format (should be host:port)
fn validate_address(addr: &str, addr_type: &str) -> Result<()> {
    if !addr.contains(':') {
        anyhow::bail!(
            "Invalid {} address format: '{}' (expected format: host:port)",
            addr_type,
            addr
        );
    }

    // Try to parse the port
    let parts: Vec<&str> = addr.rsplitn(2, ':').collect();
    if parts.len() != 2 {
        anyhow::bail!(
            "Invalid {} address format: '{}' (expected format: host:port)",
            addr_type,
            addr
        );
    }

    // Validate host is not empty (parts[1] because rsplitn reverses)
    if parts[1].is_empty() {
        anyhow::bail!(
            "Invalid {} address format: '{}' (host cannot be empty)",
            addr_type,
            addr
        );
    }

    // Validate port
    parts[0]
        .parse::<u16>()
        .with_context(|| format!("Invalid port in {} address: {}", addr_type, addr))?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let args = Args::parse();

    // Setup logging first
    setup_logging(&args.log_level)?;

    info!("LocalUp Agent starting...");

    // Build agent configuration
    let config = build_agent_config(args).context("Failed to build agent configuration")?;

    // Log configuration (but not the auth token)
    info!("Agent ID: {}", config.agent_id);
    info!("Relay: {}", config.relay_addr);
    info!("Target address: {}", config.target_address);

    // Create and start the agent
    let mut agent = Agent::new(config).context("Failed to create agent")?;

    info!("Starting agent...");

    // Setup Ctrl+C handler
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    // Start the agent
    let agent_task = tokio::spawn(async move {
        if let Err(e) = agent.start().await {
            error!("Agent error: {}", e);
            return Err(e);
        }
        Ok(())
    });

    // Wait for Ctrl+C or agent error
    tokio::select! {
        _ = &mut ctrl_c => {
            info!("Received Ctrl+C, shutting down...");
        }
        result = agent_task => {
            match result {
                Ok(Ok(())) => {
                    info!("Agent stopped normally");
                }
                Ok(Err(e)) => {
                    error!("Agent error: {:#}", e);
                    return Err(e.into());
                }
                Err(e) => {
                    error!("Agent task panicked: {}", e);
                    return Err(e.into());
                }
            }
        }
    }

    info!("Agent stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_address() {
        // Valid addresses
        assert!(validate_address("relay.example.com:4443", "relay").is_ok());
        assert!(validate_address("localhost:8080", "relay").is_ok());
        assert!(validate_address("192.168.1.1:9000", "target").is_ok());
        assert!(validate_address("192.168.1.100:8080", "target").is_ok());

        // Invalid addresses
        assert!(validate_address("relay.example.com", "relay").is_err());
        assert!(validate_address("relay.example.com:", "relay").is_err());
        assert!(validate_address("relay.example.com:abc", "relay").is_err());
        assert!(validate_address(":4443", "relay").is_err());
        assert!(validate_address("", "target").is_err());
    }
}
