//! Example: HTTPS Relay with Control Plane and TunnelClient
//!
//! This example demonstrates a complete tunnel system:
//! 1. Start a relay server with HTTPS data plane and QUIC control plane (RelayBuilder)
//! 2. Create a local HTTP server with Axum
//! 3. Connect a TunnelClient to register with the control plane
//! 4. Route traffic through the relay to the local server
//!
//! Architecture:
//! - Data Plane: HTTPS server on 127.0.0.1:8443 (accepts public HTTPS connections)
//! - Control Plane: QUIC on 127.0.0.1:4443 (TunnelClient registration and route management)
//! - Local Server: HTTP on dynamic port (your actual application)
//!
//! Run this example:
//! ```bash
//! cargo run --example https_relay
//! ```
//!
//! Expected output:
//! - HTTPS relay ready on 127.0.0.1:8443
//! - QUIC control plane listening on 127.0.0.1:4443
//! - TunnelClient connects and registers local server
//!
//! Test in another terminal:
//! ```bash
//! curl -k https://localho.st:8443/myapp
//! ```

use axum::{routing::get, Router};
use localup_lib::{
    generate_self_signed_cert, generate_token, ExitNodeConfig, HttpAuthConfig, HttpsRelayBuilder,
    InMemoryTunnelStorage, ProtocolConfig, SelfSignedCertificateProvider,
    SimpleCounterDomainProvider, TunnelClient, TunnelConfig,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing with logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Initialize Rustls crypto provider (required before using TLS)
    let _ = rustls::crypto::ring::default_provider().install_default();

    println!("🚀 HTTPS Relay Example with TunnelClient");
    println!("=========================================\n");

    // Step 1: Generate self-signed certificates
    println!("📝 Step 1: Generating self-signed certificates for relay...");
    let cert = generate_self_signed_cert()?;
    cert.save_to_files("cert.pem", "key.pem")?;
    println!("✅ Certificates generated: cert.pem, key.pem\n");

    // Step 2: Build the HTTPS relay with control plane using HttpsRelayBuilder
    println!("📝 Step 2: Building HTTPS relay with control plane...");
    let relay = HttpsRelayBuilder::new("127.0.0.1:8443", "cert.pem", "key.pem")?
        .control_plane("127.0.0.1:4443")?
        .jwt_secret(b"example-secret-key")
        // Configure relay behavior with trait-based customization
        .storage(Arc::new(InMemoryTunnelStorage::new())) // In-memory tunnel storage
        .domain_provider(Arc::new(SimpleCounterDomainProvider::new())) // Simple domain naming
        .certificate_provider(Arc::new(SelfSignedCertificateProvider)) // Self-signed certs
        .build()?;
    println!("✅ Relay configuration created");
    println!("   - Data Plane (HTTPS): 127.0.0.1:8443");
    println!("   - Control Plane (QUIC): 127.0.0.1:4443");
    println!("   - Storage: In-memory (trait-based, customizable)");
    println!("   - Authentication: JWT enabled\n");

    // Spawn relay in background
    let mut relay_handle = tokio::spawn(async move { relay.run().await });

    // Give relay time to initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Step 3: Create local Axum HTTP server
    println!("📝 Step 3: Starting local Axum HTTP server...");

    // Create router with routes
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/myapp", get(myapp_handler))
        .route("/{path}", get(catch_all_handler))
        .into_make_service();

    // Bind to dynamic port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;
    let local_port = local_addr.port();

    println!("✅ Local Axum HTTP server started on {}\n", local_addr);

    // Spawn Axum server
    tokio::spawn(async move {
        let server = axum::serve(listener, app);
        if let Err(e) = server.await {
            eprintln!("❌ Axum server error: {}", e);
        }
    });

    // Step 4: Generate authentication token
    println!("📝 Step 4: Generating authentication token...");
    let auth_token = generate_token("myapp", b"example-secret-key", 24)?;
    println!("✅ Token generated\n");

    // Step 5: Connect TunnelClient to relay
    println!("📝 Step 5: Connecting TunnelClient to relay...");

    let tunnel_config = TunnelConfig {
        local_host: "127.0.0.1".to_string(),
        protocols: vec![ProtocolConfig::Https {
            local_port,
            subdomain: None,
        }],
        auth_token,
        exit_node: ExitNodeConfig::Custom("127.0.0.1:4443".to_string()),
        failover: false,
        connection_timeout: std::time::Duration::from_secs(5),
        preferred_transport: None,
        http_auth: HttpAuthConfig::None,
    };

    match TunnelClient::connect(tunnel_config).await {
        Ok(client) => {
            if let Some(url) = client.public_url() {
                println!("✅ TunnelClient connected!");
                println!("   Public URL: {}\n", url);

                // Step 6: Testing instructions
                println!("🧪 Testing the tunnel:");
                println!("=======================");
                println!("In another terminal, test the tunnel with:");
                println!("  curl -k {}:8443/myapp", url);
                println!();
                println!("Expected response:");
                println!("  ✅ Hello from Axum server! (myapp path)");
                println!();
                println!("This example demonstrates:");
                println!("  1. RelayBuilder: Simple API for setting up relay servers");
                println!("  2. Axum: Local HTTP server (user's application)");
                println!("  3. TunnelClient: Registers the local server with the relay");
                println!("  4. End-to-end: Local app exposed through HTTPS relay\n");
                println!("Press Ctrl+C to stop...\n");
            }

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
        }
        Err(e) => {
            eprintln!("❌ Failed to connect TunnelClient: {}", e);
            eprintln!("\nNote: This example requires the relay's control plane to be running.");
            eprintln!("The TunnelClient needs a QUIC-based control plane on port 4443.");
            eprintln!("\nTo run a complete system, use the exit-node binary:");
            eprintln!("  cargo run -p localup-exit-node\n");
        }
    }

    println!("✅ Example completed!");
    Ok(())
}

/// Handler for root path
async fn root_handler() -> String {
    tracing::info!("✅ Handling root path request");
    "✅ Hello from Axum server! (root path)".to_string()
}

/// Handler for /myapp path
async fn myapp_handler() -> String {
    tracing::info!("✅ Handling /myapp request");
    "✅ Hello from Axum server! (myapp path)".to_string()
}

/// Catch-all handler for other paths
async fn catch_all_handler(axum::extract::Path(path): axum::extract::Path<String>) -> String {
    tracing::info!("📍 Catch-all handler for path: {}", path);
    format!("✅ Hello from Axum server! (path: /{})", path)
}
