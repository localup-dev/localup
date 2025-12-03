//! Example: TLS/SNI Relay with Multi-Tenant Support
//!
//! This example demonstrates:
//! 1. Creating a TLS/SNI relay using TlsRelayBuilder
//! 2. Setting up multiple TLS backend services with proper certificates
//! 3. SNI-based routing with TLS passthrough
//! 4. Testing with localho.st subdomains (a domain that resolves to 127.0.0.1)
//!
//! Architecture:
//! - Relay accepts TLS connections on port 443 and extracts SNI
//! - Backend services (API, DB) are TLS servers with domain-specific certificates
//! - Relay routes based on SNI hostname and forwards encrypted traffic
//! - No decryption happens at relay (true passthrough)
//!
//! Run this example in one terminal:
//! ```bash
//! cargo run --example tls_relay
//! ```
//!
//! Then in another terminal, test with:
//! ```bash
//! openssl s_client -connect localho.st:8443 -servername api.localho.st </dev/null
//! ```

use localup_lib::{
    generate_self_signed_cert, generate_self_signed_cert_with_domains, generate_token,
    ExitNodeConfig, HttpAuthConfig, InMemoryTunnelStorage, ProtocolConfig,
    SelfSignedCertificateProvider, SimpleCounterDomainProvider, TlsRelayBuilder, TunnelClient,
    TunnelConfig,
};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize Rustls crypto provider (required before using TLS)
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("🚀 TLS/SNI Relay Example");
    println!("========================\n");

    // Step 1: Generate relay certificate for accepting connections
    println!("📝 Step 1: Generating self-signed certificates for TLS services...");

    // Generate certificate for relay (accepts connections on port 443)
    println!("  → Generating relay certificate...");
    let relay_cert = generate_self_signed_cert()?;
    relay_cert.save_to_files("relay_cert.pem", "relay_key.pem")?;
    println!("✅ Relay certificate generated: relay_cert.pem, relay_key.pem");
    println!("     CN: Tunnel Development Certificate");
    println!("     SANs: localhost, 127.0.0.1, ::1");

    // Generate certificate for API backend service with domain-specific SAN/CN
    println!("  → Generating API service certificate for api.localho.st...");
    let api_cert = generate_self_signed_cert_with_domains("api.localho.st", &["localho.st"])?;
    api_cert.save_to_files("api_cert.pem", "api_key.pem")?;
    println!("✅ API certificate generated: api_cert.pem, api_key.pem");
    println!("     CN: api.localho.st");
    println!("     SANs: localho.st, *.localho.st, localhost, 127.0.0.1");

    // Generate certificate for DB backend service with domain-specific SAN/CN
    println!("  → Generating DB service certificate for db.localho.st...");
    let db_cert = generate_self_signed_cert_with_domains("db.localho.st", &["localho.st"])?;
    db_cert.save_to_files("db_cert.pem", "db_key.pem")?;
    println!("✅ DB certificate generated: db_cert.pem, db_key.pem");
    println!("     CN: db.localho.st");
    println!("     SANs: localho.st, *.localho.st, localhost, 127.0.0.1\n");

    println!("⚠️  TLS Passthrough Architecture:");
    println!("    - Relay accepts TLS on port 443");
    println!("    - Backend services also speak TLS (on ports 5443, 5444)");
    println!("    - Relay forwards encrypted traffic without decryption");
    println!("    - SNI extraction happens at TLS layer\n");

    // Step 2: Set up route registry
    println!("📝 Step 2: Setting up route registry...");
    println!("✅ Route registry created\n");

    // Step 3: Start local TLS services
    println!("📝 Step 3: Starting local TLS services...");

    // Create TLS acceptor for API service
    // Note: We create ServerConfig once and wrap in Arc to handle non-Clone types like PrivateKeyDer
    println!("  → Starting API service on localhost:5443 (with TLS)...");
    let api_listener = TcpListener::bind("127.0.0.1:5443").await?;
    let api_port = api_listener.local_addr()?.port();

    let api_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![api_cert.cert_der.clone()], api_cert.key_der)?;
    let api_acceptor = Arc::new(TlsAcceptor::from(Arc::new(api_config)));

    tokio::spawn(async move {
        loop {
            match api_listener.accept().await {
                Ok((socket, addr)) => {
                    let acceptor = api_acceptor.clone();
                    tokio::spawn(async move {
                        match acceptor.accept(socket).await {
                            Ok(mut tls_stream) => {
                                let mut buf = [0; 1024];
                                if (tls_stream.read(&mut buf).await).is_ok() {
                                    let msg = format!("[API] Connection from {}\n", addr);
                                    let _ = tls_stream.write_all(msg.as_bytes()).await;
                                }
                            }
                            Err(e) => eprintln!("  [API] TLS error: {}", e),
                        }
                    });
                }
                Err(e) => eprintln!("  [API] Accept error: {}", e),
            }
        }
    });

    // Create TLS acceptor for DB service
    println!("  → Starting DB service on localhost:5444 (with TLS)...");
    let db_listener = TcpListener::bind("127.0.0.1:5444").await?;
    let db_port = db_listener.local_addr()?.port();

    let db_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![db_cert.cert_der.clone()], db_cert.key_der)?;
    let db_acceptor = Arc::new(TlsAcceptor::from(Arc::new(db_config)));

    tokio::spawn(async move {
        loop {
            match db_listener.accept().await {
                Ok((socket, addr)) => {
                    let acceptor = db_acceptor.clone();
                    tokio::spawn(async move {
                        match acceptor.accept(socket).await {
                            Ok(mut tls_stream) => {
                                let mut buf = [0; 1024];
                                if (tls_stream.read(&mut buf).await).is_ok() {
                                    let msg = format!("[DB] Connection from {}\n", addr);
                                    let _ = tls_stream.write_all(msg.as_bytes()).await;
                                }
                            }
                            Err(e) => eprintln!("  [DB] TLS error: {}", e),
                        }
                    });
                }
                Err(e) => eprintln!("  [DB] Accept error: {}", e),
            }
        }
    });

    println!("✅ Local TLS services started (both with TLS)\n");

    // Step 4: Build TLS relay with control plane using TlsRelayBuilder
    println!("📝 Step 4: Building TLS/SNI relay with control plane...");
    let relay = TlsRelayBuilder::new("127.0.0.1:8443")?
        .control_plane("127.0.0.1:4443")?
        .jwt_secret(b"example-secret-key")
        // Configure relay behavior with trait-based customization
        .storage(Arc::new(InMemoryTunnelStorage::new())) // In-memory tunnel storage
        .domain_provider(Arc::new(SimpleCounterDomainProvider::new())) // Simple domain naming
        .certificate_provider(Arc::new(SelfSignedCertificateProvider)) // Self-signed certs
        .build()?;

    println!("✅ Relay configuration created");
    println!("   - Data Plane (TLS/SNI): 127.0.0.1:8443");
    println!("   - Control Plane (QUIC): 127.0.0.1:4443");
    println!("   - Storage: In-memory (trait-based, customizable)");
    println!("   - Authentication: JWT enabled");
    println!("   - Routes: Registered by TunnelClient connections\n");

    // Spawn relay in background
    let mut relay_handle = tokio::spawn(async move { relay.run().await });

    // Give relay time to initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Step 5: Generate authentication token
    println!("📝 Step 5: Generating authentication token...");
    let auth_token = generate_token("tls-service", b"example-secret-key", 24)?;
    println!("✅ Token generated\n");

    // Step 6: Connect TunnelClient for API service
    println!("📝 Step 6: Connecting TunnelClient for API service...");

    let tunnel_config = TunnelConfig {
        local_host: "127.0.0.1".to_string(),
        protocols: vec![ProtocolConfig::Tls {
            local_port: api_port,
            sni_hostname: Some("api.localho.st".to_string()),
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
            println!("✅ TunnelClient connected!");
            println!("   Local API port: {}", api_port);
            println!("   Local DB port: {}", db_port);
            println!("   Registered SNI hostname: api.localho.st");
            println!("   Control Plane: 127.0.0.1:4443\n");

            // Step 7: Testing instructions
            println!("🧪 Testing the TLS/SNI relay:");
            println!("=============================");
            println!("Note: localho.st is a domain that resolves to 127.0.0.1 (localhost).");
            println!("This allows testing SNI routing with proper domain names.\n");
            println!("In another terminal, test SNI routing with:\n");
            println!("  # Test API service (routes to localhost:{})", api_port);
            println!("  openssl s_client -connect localho.st:8443 -servername api.localho.st </dev/null\n");
            println!("  # Test DB service (would need separate TunnelClient)");
            println!("  # For now, this example demonstrates routing for API service");
            println!("The relay extracts the SNI hostname from the TLS ClientHello");
            println!("and routes to the appropriate backend service.\n");
            println!("🔒 Security note:");
            println!("  - The relay never decrypts TLS traffic (passthrough mode)");
            println!("  - Each service keeps its own certificates");
            println!("  - End-to-end encryption is maintained\n");
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
