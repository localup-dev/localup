//! Reverse tunnel client example
//!
//! This example demonstrates how to use the ReverseTunnelClient to connect to
//! remote services through an agent via the relay server.
//!
//! Usage:
//!   cargo run --example reverse_localup_example -- \
//!     --relay-addr 127.0.0.1:4443 \
//!     --remote-address 192.168.1.100:8080 \
//!     --agent-id my-agent \
//!     --local-bind 127.0.0.1:8888

use localup_client::{ReverseTunnelClient, ReverseTunnelConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Parse command-line arguments (simple parsing for demo)
    let args: Vec<String> = std::env::args().collect();

    let relay_addr = args
        .iter()
        .position(|a| a == "--relay-addr")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:4443".to_string());

    let remote_address = args
        .iter()
        .position(|a| a == "--remote-address")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "192.168.1.100:8080".to_string());

    let agent_id = args
        .iter()
        .position(|a| a == "--agent-id")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "default-agent".to_string());

    let local_bind = args
        .iter()
        .position(|a| a == "--local-bind")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let auth_token = args
        .iter()
        .position(|a| a == "--auth-token")
        .and_then(|i| args.get(i + 1))
        .cloned();

    // Create reverse tunnel configuration
    let mut config =
        ReverseTunnelConfig::new(relay_addr.clone(), remote_address.clone(), agent_id.clone())
            .with_insecure(true); // Use insecure mode for development

    if let Some(token) = auth_token {
        config = config.with_auth_token(token);
    }

    if let Some(bind) = local_bind {
        config = config.with_local_bind_address(bind);
    }

    println!("🚀 Connecting to reverse tunnel:");
    println!("   Relay: {}", relay_addr);
    println!("   Remote: {}", remote_address);
    println!("   Agent: {}", agent_id);

    // Connect to reverse tunnel
    let client = ReverseTunnelClient::connect(config).await?;

    println!("\n✅ Reverse tunnel established!");
    println!("   Tunnel ID: {}", client.localup_id());
    println!("   Local address: {}", client.local_addr());
    println!("   Remote address: {}", client.remote_address());
    println!("   Agent: {}", client.agent_id());
    println!("\n📡 Listening for connections...");
    println!(
        "   Connect to {} to reach {}",
        client.local_addr(),
        remote_address
    );
    println!("\nPress Ctrl+C to stop\n");

    // Setup Ctrl+C handler
    let client_clone = client;
    let handle = tokio::spawn(async move { client_clone.wait().await });

    tokio::select! {
        result = handle => {
            match result {
                Ok(Ok(())) => println!("\n✅ Tunnel closed gracefully"),
                Ok(Err(e)) => eprintln!("\n❌ Tunnel error: {}", e),
                Err(e) => eprintln!("\n❌ Task error: {}", e),
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\n🛑 Shutting down...");
        }
    }

    Ok(())
}
