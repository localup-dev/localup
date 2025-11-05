//! LocalUp Agent-Server CLI
//!
//! Standalone server for reverse tunnels without requiring a separate relay.

use clap::Parser;
use ipnet::IpNet;
use localup_agent_server::{AccessControl, AgentServer, AgentServerConfig, PortRange};
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
                  Examples:\n  \
                  # Allow any address and port\n  \
                  localup-agent-server --listen 0.0.0.0:4443 --cert server.crt --key server.key\n\n  \
                  # Only allow private networks and specific ports\n  \
                  localup-agent-server \\\n    \
                  --listen 0.0.0.0:4443 \\\n    \
                  --cert server.crt --key server.key \\\n    \
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

    /// TLS certificate path
    #[arg(long, default_value = "server.crt", env = "LOCALUP_CERT")]
    cert: String,

    /// TLS key path
    #[arg(long, default_value = "server.key", env = "LOCALUP_KEY")]
    key: String,

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
    tracing::info!("Certificate: {}", cli.cert);
    tracing::info!("Key: {}", cli.key);

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

    // Create access control
    let access_control = AccessControl::new(cli.allowed_cidrs, cli.allowed_ports);

    // Create server config
    let config = AgentServerConfig {
        listen_addr: cli.listen,
        cert_path: cli.cert,
        key_path: cli.key,
        access_control,
        jwt_secret: cli.jwt_secret,
    };

    // Create and run server
    let server = AgentServer::new(config)?;
    server.run().await?;

    Ok(())
}
