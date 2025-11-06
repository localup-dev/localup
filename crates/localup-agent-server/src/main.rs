//! LocalUp Agent-Server CLI
//!
//! Standalone server for reverse tunnels without requiring a separate relay.

use clap::Parser;
use ipnet::IpNet;
use localup_agent_server::{AccessControl, AgentServer, AgentServerConfig, PortRange, RelayConfig};
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(
    name = "localup-agent-server",
    about = "Standalone agent-server for reverse tunnels",
    version,
    long_about = "Agent-server combines relay and agent functionality in a single binary.\n\
                  Perfect for VPN scenarios where you want to expose internal services\n\
                  without running a separate relay.\n\n\
                  TLS certificates are auto-generated if not provided (stored in ~/.localup/).\n\n\
                  Examples:\n  \
                  # Auto-generate certificates and allow any address/port\n  \
                  localup-agent-server --listen 0.0.0.0:4443\n\n  \
                  # Use custom certificates\n  \
                  localup-agent-server --listen 0.0.0.0:4443 --cert server.crt --key server.key\n\n  \
                  # Only allow private networks and specific ports\n  \
                  localup-agent-server \\\n    \
                  --listen 0.0.0.0:4443 \\\n    \
                  --allow-cidr 10.0.0.0/8 \\\n    \
                  --allow-cidr 192.168.0.0/16 \\\n    \
                  --allow-port 22 \\\n    \
                  --allow-port 80-443 \\\n    \
                  --allow-port 5432"
)]
struct Cli {
    /// Listen address for QUIC server
    #[arg(
        short = 'l',
        long,
        default_value = "0.0.0.0:4443",
        env = "LOCALUP_LISTEN"
    )]
    listen: SocketAddr,

    /// TLS certificate path (optional, auto-generated if not provided)
    #[arg(long, env = "LOCALUP_CERT")]
    cert: Option<String>,

    /// TLS key path (optional, auto-generated if not provided)
    #[arg(long, env = "LOCALUP_KEY")]
    key: Option<String>,

    /// Allowed CIDR ranges (can be specified multiple times)
    /// If not specified, all addresses are allowed
    #[arg(long = "allow-cidr", value_name = "CIDR")]
    allowed_cidrs: Vec<IpNet>,

    /// Allowed port ranges (can be specified multiple times)
    /// Format: single port (e.g., "22") or range (e.g., "80-443")
    /// If not specified, all ports are allowed
    #[arg(long = "allow-port", value_name = "PORT", value_parser = parse_port_range)]
    allowed_ports: Vec<PortRange>,

    /// JWT secret for authentication (optional)
    #[arg(long, env = "LOCALUP_JWT_SECRET")]
    jwt_secret: Option<String>,

    /// Relay server address to connect to (optional)
    /// If set, this server will register itself with the relay
    /// Format: IP:PORT or hostname:PORT
    /// Example: relay.example.com:4443
    #[arg(long, env = "LOCALUP_RELAY_ADDR")]
    relay_addr: Option<String>,

    /// Server ID on the relay (required if relay_addr is set)
    /// This is the ID that clients will use to connect to this server through the relay
    /// Example: my-internal-server
    #[arg(long, env = "LOCALUP_RELAY_ID")]
    relay_id: Option<String>,

    /// Authentication token for relay server (optional)
    #[arg(long, env = "LOCALUP_RELAY_TOKEN")]
    relay_token: Option<String>,

    /// Target address for relay forwarding (required if relay_addr is set)
    /// This is the backend service address that the relay will route traffic to
    /// Example: 127.0.0.1:5432 for PostgreSQL, 192.168.1.100:8080 for a web service
    #[arg(long, env = "LOCALUP_TARGET_ADDRESS")]
    target_address: Option<String>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

fn parse_port_range(s: &str) -> Result<PortRange, String> {
    s.parse::<PortRange>()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let filter = if cli.verbose {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "localup_agent_server=debug,tunnel_agent=debug".into())
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "localup_agent_server=info,tunnel_agent=info".into())
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Print configuration
    tracing::info!("🚀 Starting LocalUp Agent-Server");
    tracing::info!("Listen: {}", cli.listen);

    if let Some(ref cert) = cli.cert {
        tracing::info!("Certificate: {} (custom)", cert);
    } else {
        tracing::info!("Certificate: (auto-generated)");
    }

    if let Some(ref key) = cli.key {
        tracing::info!("Key: {} (custom)", key);
    } else {
        tracing::info!("Key: (auto-generated)");
    }

    if cli.allowed_cidrs.is_empty() {
        tracing::warn!("⚠️  No CIDR restrictions - allowing ALL IP addresses");
    } else {
        tracing::info!("Allowed CIDRs:");
        for cidr in &cli.allowed_cidrs {
            tracing::info!("  - {}", cidr);
        }
    }

    if cli.allowed_ports.is_empty() {
        tracing::warn!("⚠️  No port restrictions - allowing ALL ports");
    } else {
        tracing::info!("Allowed ports:");
        for range in &cli.allowed_ports {
            if range.start == range.end {
                tracing::info!("  - {}", range.start);
            } else {
                tracing::info!("  - {}-{}", range.start, range.end);
            }
        }
    }

    if cli.jwt_secret.is_some() {
        tracing::info!("✅ JWT authentication enabled");
    } else {
        tracing::warn!("⚠️  No JWT authentication - allowing all clients");
    }

    // Parse relay configuration if provided
    let relay_config = if let Some(relay_addr_str) = &cli.relay_addr {
        let relay_id = match &cli.relay_id {
            Some(id) => id.clone(),
            None => {
                return Err(anyhow::anyhow!(
                    "Relay ID (--relay-id) is required when relay address (--relay-addr) is set"
                ));
            }
        };

        let target_address = match &cli.target_address {
            Some(addr) => addr.clone(),
            None => {
                return Err(anyhow::anyhow!(
                    "Target address (--target-address) is required when relay address (--relay-addr) is set.\n\
                    This is the backend service address the relay should route traffic to.\n\
                    Example: 127.0.0.1:5432 (PostgreSQL), 192.168.1.100:8080 (Web service)"
                ));
            }
        };

        match relay_addr_str.parse::<SocketAddr>() {
            Ok(relay_addr) => {
                tracing::info!("🔄 Relay server enabled: {}", relay_addr);
                tracing::info!("Server ID on relay: {}", relay_id);
                tracing::info!("Backend target address: {}", target_address);
                if cli.relay_token.is_some() {
                    tracing::info!("✅ Relay authentication enabled");
                } else {
                    tracing::warn!("⚠️  No relay authentication token");
                }
                Some(RelayConfig {
                    relay_addr,
                    server_id: relay_id,
                    target_address,
                    relay_token: cli.relay_token,
                })
            }
            Err(e) => {
                tracing::error!("Failed to parse relay address '{}': {}", relay_addr_str, e);
                return Err(anyhow::anyhow!("Invalid relay address: {}", e));
            }
        }
    } else if cli.relay_id.is_some() {
        return Err(anyhow::anyhow!(
            "Relay address (--relay-addr) is required when relay ID (--relay-id) is set"
        ));
    } else {
        None
    };

    // Create access control
    let access_control = AccessControl::new(cli.allowed_cidrs, cli.allowed_ports);

    // Create server config
    let config = AgentServerConfig {
        listen_addr: cli.listen,
        cert_path: cli.cert,
        key_path: cli.key,
        access_control,
        jwt_secret: cli.jwt_secret,
        relay_config,
    };

    // Create and run server
    let server = AgentServer::new(config)?;
    server.run().await?;

    Ok(())
}
