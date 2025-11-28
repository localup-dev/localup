//! Example: TCP Relay with Control Plane and TunnelClient
//!
//! This example demonstrates a complete TCP tunnel system:
//! 1. Start a relay server with QUIC control plane (RelayBuilder)
//! 2. Create a local TCP echo server
//! 3. Connect a TunnelClient to register with the control plane
//! 4. Route traffic through the relay to the local server
//!
//! Architecture:
//! - Data Plane: TCP ports dynamically allocated by control plane
//! - Control Plane: QUIC on 127.0.0.1:4443 (TunnelClient registration and dynamic port allocation)
//! - Local Server: TCP echo on dynamic port (your actual application)
//!
//! Run this example:
//! ```bash
//! cargo run --example tcp_relay
//! ```
//!
//! Expected output:
//! - QUIC control plane listening on 127.0.0.1:4443
//! - TunnelClient connects and gets allocated a TCP port
//! - TCP proxy server spawns on the allocated port
//!
//! Test in another terminal:
//! ```bash
//! # Look at the output to find the allocated port number
//! nc localhost <ALLOCATED_PORT>
//! ```

use localup_lib::{
    generate_token, ExitNodeConfig, InMemoryTunnelStorage, ProtocolConfig,
    SelfSignedCertificateProvider, SimpleCounterDomainProvider, SimplePortAllocator,
    TcpRelayBuilder, TunnelClient, TunnelConfig,
};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize Rustls crypto provider (required before using TLS)
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("🚀 TCP Relay Example");
    println!("====================\n");

    // Step 1: Start a local TCP echo server
    println!("📝 Step 1: Starting local TCP echo server on localhost:6000...");
    let echo_listener = TcpListener::bind("127.0.0.1:6000").await?;
    let echo_port = echo_listener.local_addr()?.port();

    tokio::spawn(async move {
        loop {
            match echo_listener.accept().await {
                Ok((mut socket, addr)) => {
                    tokio::spawn(async move {
                        let mut buf = [0; 1024];
                        loop {
                            match socket.read(&mut buf).await {
                                Ok(0) => {
                                    println!("  [Echo] Client {} disconnected", addr);
                                    break;
                                }
                                Ok(n) => {
                                    let message = String::from_utf8_lossy(&buf[..n]);
                                    println!("  [Echo] Received: {}", message.trim());
                                    if let Err(e) = socket.write_all(&buf[..n]).await {
                                        eprintln!("  [Echo] Write error: {}", e);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    eprintln!("  [Echo] Read error: {}", e);
                                    break;
                                }
                            }
                        }
                    });
                }
                Err(e) => eprintln!("  [Echo] Accept error: {}", e),
            }
        }
    });

    println!("✅ Echo server started\n");

    // Step 2: Build the TCP relay with control plane using TcpRelayBuilder
    println!("📝 Step 2: Building TCP relay with control plane...");
    let relay = TcpRelayBuilder::new()
        .tcp_port_range(10000, Some(20000)) // Allocate ports 10000-20000
        .control_plane("127.0.0.1:4443")?
        .jwt_secret(b"example-secret-key")
        // Configure relay behavior with trait-based customization
        .storage(Arc::new(InMemoryTunnelStorage::new())) // In-memory tunnel storage
        .domain_provider(Arc::new(SimpleCounterDomainProvider::new())) // Simple domain naming
        .certificate_provider(Arc::new(SelfSignedCertificateProvider)) // Self-signed certs
        .port_allocator(Arc::new(SimplePortAllocator::with_range(
            10000,
            Some(20000),
        ))) // Custom port range
        .build()?;

    println!("✅ Relay configuration created");
    println!("   - Control Plane (QUIC): 127.0.0.1:4443");
    println!("   - TCP Ports: 10000-20000 (dynamically allocated)");
    println!("   - Storage: In-memory (trait-based, customizable)");
    println!("   - Authentication: JWT enabled\n");

    // Spawn relay in background
    let mut relay_handle = tokio::spawn(async move { relay.run().await });

    // Give relay time to initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Step 3: Generate authentication token
    println!("📝 Step 3: Generating authentication token...");
    let auth_token = generate_token("tcp-echo", b"example-secret-key", 24)?;
    println!("✅ Token generated\n");

    // Step 4: Connect TunnelClient to relay
    println!("📝 Step 4: Connecting TunnelClient to relay...");

    let tunnel_config = TunnelConfig {
        local_host: "127.0.0.1".to_string(),
        protocols: vec![ProtocolConfig::Tcp {
            local_port: echo_port,
            remote_port: None, // Control plane will allocate a port dynamically
        }],
        auth_token,
        exit_node: ExitNodeConfig::Custom("127.0.0.1:4443".to_string()),
        failover: false,
        connection_timeout: std::time::Duration::from_secs(5),
        preferred_transport: None,
    };

    match TunnelClient::connect(tunnel_config).await {
        Ok(client) => {
            println!("✅ TunnelClient connected!");
            println!("   Local echo server port: {}", echo_port);
            println!("   Control Plane: 127.0.0.1:4443");
            println!("   TCP proxy port range: 10000-20000");

            // Extract the allocated port from the public URL
            let allocated_port = client
                .public_url()
                .and_then(|url| url.split(':').next_back().map(|p| p.to_string()))
                .unwrap_or_else(|| "unknown".to_string());

            println!("   Allocated port: {}\n", allocated_port);

            // Step 5: Testing instructions
            println!("🧪 Testing the tunnel:");
            println!("=======================");
            println!("In another terminal, connect to the relay with:");
            println!();
            println!(
                "  nc localhost {}  # or: telnet localhost {}",
                allocated_port, allocated_port
            );
            println!();
            println!("Type any message, and it will be echoed back:");
            println!("  > Hello");
            println!("  Hello");
            println!();
            println!(
                "The tunnel routes the connection through the relay to your local echo server."
            );
            println!();
            println!("📊 Tunnel Flow:");
            println!("===============");
            println!(
                "Client TCP  →  Relay (port {})  →  Echo Server Port {}",
                allocated_port, echo_port
            );
            println!("(public)       (dynamically allocated)      (local/private)");
            println!();
            println!("Press Ctrl+C to stop...\n");

            // Race between client.wait() and relay_handle completion
            // Whichever completes first will stop the example
            tokio::select! {
                result = client.wait() => {
                    if let Err(e) = result {
                        eprintln!("❌ Client error: {}", e);
                    }
                }
                result = &mut relay_handle => {
                    if let Err(e) = result {
                        eprintln!("❌ Relay error: {}", e);
                    }
                }
            }

            println!("✅ Example completed!");
        }
        Err(e) => {
            eprintln!("❌ Failed to connect TunnelClient: {}", e);
            eprintln!("Make sure the relay is running with control plane support");
            return Err(Box::new(e) as Box<dyn std::error::Error>);
        }
    }

    Ok(())
}
