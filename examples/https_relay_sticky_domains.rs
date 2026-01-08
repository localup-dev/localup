//! Example: HTTPS Relay with Sticky Domain Allocation
//!
//! This example demonstrates a custom DomainProvider that assigns "sticky" subdomains
//! based on client identity + port combination. The same client always gets the same
//! subdomain, even after reconnections.
//!
//! **How Sticky Domains Work:**
//! 1. Client connects with auth token (client identity)
//! 2. Relay generates subdomain based on: hash(client_id + port)
//! 3. Subdomain is persistent across reconnects for same client+port pair
//! 4. Format: "sticky-{client_id}-{port}" or hash-based identifier
//!
//! **Use Cases:**
//! - DNS records that don't change on client reconnection
//! - Load balancers expecting stable hostnames
//! - Integration with reverse proxies
//! - Monitoring systems with persistent URLs
//!
//! Run this example:
//! ```bash
//! cargo run --example https_relay_sticky_domains
//! ```

use axum::{routing::get, Router};
use localup_lib::{
    async_trait, generate_self_signed_cert, generate_token, DomainContext, DomainProvider,
    DomainProviderError, ExitNodeConfig, HttpAuthConfig, HttpsRelayBuilder, InMemoryTunnelStorage,
    ProtocolConfig, SelfSignedCertificateProvider, TunnelClient, TunnelConfig,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ============================================================================
// STICKY DOMAIN PROVIDER IMPLEMENTATION
// ============================================================================

/// Domain provider that assigns sticky subdomains based on client + port
///
/// When the same client (by token/ID) connects to the same port multiple times,
/// it always receives the same subdomain. This ensures DNS records and load
/// balancer configurations remain stable even across reconnections.
///
/// Architecture:
/// - Tracks mapping: (client_id, port) -> subdomain
/// - Generates stable, deterministic subdomains
/// - Falls back to auto-generated if client_id unavailable
struct StickyDomainProvider {
    /// Map of (client_id, port) -> assigned_subdomain
    /// Persists across reconnections
    sticky_map: Arc<Mutex<HashMap<String, String>>>,
    /// Fallback counter for clients without identity
    counter: Arc<Mutex<u64>>,
    /// Reserved subdomains to prevent conflicts
    reserved: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl StickyDomainProvider {
    fn new() -> Self {
        Self {
            sticky_map: Arc::new(Mutex::new(HashMap::new())),
            counter: Arc::new(Mutex::new(0)),
            reserved: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }

    /// Create a sticky subdomain from client ID and port
    /// Format: "sticky-{client_id}-{port}"
    #[allow(dead_code)]
    fn create_sticky_domain(client_id: &str, port: u16) -> String {
        // Use client_id and port to create deterministic subdomain
        // Sanitize client_id to DNS-safe characters
        let safe_id = client_id
            .chars()
            .take(10) // Limit length
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>()
            .trim_matches('-')
            .to_lowercase();

        format!("sticky-{}-{}", safe_id, port)
    }

    /// Get or create a sticky domain for a client+port pair
    ///
    /// Returns:
    /// - Existing subdomain if this client+port has connected before
    /// - New sticky subdomain if first time (stored for future use)
    /// - Fallback auto-generated if no client_id
    #[allow(dead_code)]
    fn get_or_create_sticky_domain(
        &self,
        client_id: Option<&str>,
        port: u16,
    ) -> Result<String, DomainProviderError> {
        if let Some(client_id) = client_id {
            let key = format!("{}:{}", client_id, port);

            // Check if we've seen this client+port before
            {
                let map = self.sticky_map.lock().unwrap();
                if let Some(existing) = map.get(&key) {
                    return Ok(existing.clone());
                }
            }

            // Create new sticky domain for this client+port
            let subdomain = Self::create_sticky_domain(client_id, port);

            // Store in map for future connections
            {
                let mut map = self.sticky_map.lock().unwrap();
                map.insert(key, subdomain.clone());
            }

            Ok(subdomain)
        } else {
            // No client ID available, fall back to counter
            let mut counter = self.counter.lock().unwrap();
            *counter += 1;
            Ok(format!("sticky-client-{}", counter))
        }
    }

    /// Show current sticky domain mappings (for debugging)
    fn show_mappings(&self) {
        let map = self.sticky_map.lock().unwrap();
        if map.is_empty() {
            println!("   (no sticky domains assigned yet)");
        } else {
            for (key, subdomain) in map.iter() {
                println!("   {} → {}", key, subdomain);
            }
        }
    }
}

#[async_trait]
impl DomainProvider for StickyDomainProvider {
    async fn generate_subdomain(
        &self,
        context: &DomainContext,
    ) -> Result<String, DomainProviderError> {
        // Use client_id + port for sticky assignment if available
        if let (Some(client_id), Some(local_port)) = (&context.client_id, context.local_port) {
            self.get_or_create_sticky_domain(Some(client_id), local_port)
        } else {
            // Fallback: create a numbered sticky domain
            let mut counter = self.counter.lock().unwrap();
            *counter += 1;
            Ok(format!("sticky-fallback-{}", counter))
        }
    }

    async fn generate_public_url(
        &self,
        _context: &DomainContext,
        subdomain: Option<&str>,
        _port: Option<u16>,
        protocol: &str,
        public_domain: &str,
    ) -> Result<String, DomainProviderError> {
        match protocol {
            "https" | "http" => subdomain
                .map(|s| format!("{}://{}.{}", protocol, s, public_domain))
                .ok_or_else(|| DomainProviderError::DomainError("Subdomain required".into())),
            _ => Err(DomainProviderError::DomainError(
                "Only HTTP/HTTPS supported".into(),
            )),
        }
    }

    async fn is_available(&self, subdomain: &str) -> Result<bool, DomainProviderError> {
        Ok(!self.reserved.lock().unwrap().contains(subdomain))
    }

    async fn reserve(&self, subdomain: &str) -> Result<(), DomainProviderError> {
        self.reserved.lock().unwrap().insert(subdomain.to_string());
        Ok(())
    }

    async fn release(&self, subdomain: &str) -> Result<(), DomainProviderError> {
        self.reserved.lock().unwrap().remove(subdomain);
        Ok(())
    }

    /// Sticky domains: allow manual selection (but recommend auto)
    fn allow_manual_subdomain(&self) -> bool {
        false // Force sticky assignment for consistency
    }

    /// Reject manual subdomains - use sticky assignment
    fn validate_subdomain(&self, _subdomain: &str) -> Result<(), DomainProviderError> {
        Err(DomainProviderError::InvalidSubdomain(
            "Manual subdomain selection is disabled. Sticky domains are auto-assigned based on client+port."
                .to_string(),
        ))
    }
}

// ============================================================================
// MAIN APPLICATION
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let _ = rustls::crypto::ring::default_provider().install_default();

    println!("🚀 Sticky Domain Provider Example");
    println!("==================================\n");

    // Step 1: Create sticky domain provider
    println!("📋 Creating sticky domain provider");
    println!("   Subdomains persist based on: client_id + port");
    println!("   Format: sticky-{{client_id}}-{{port}}\n");

    let domain_provider = Arc::new(StickyDomainProvider::new());

    println!("ℹ️  Policy Configuration:");
    println!(
        "   - Manual subdomain selection: {}",
        domain_provider.allow_manual_subdomain()
    );
    println!("   - Assignment strategy: Sticky (based on client + port)");
    println!("   - Persistence: Across reconnections\n");

    // Step 2: Generate certificates
    println!("📝 Step 1: Generating certificates...");
    let cert = generate_self_signed_cert()?;
    cert.save_to_files("cert.pem", "key.pem")?;
    println!("✅ Ready\n");

    // Step 3: Build relay with sticky domain provider
    println!("📝 Step 2: Building HTTPS relay with sticky domains...");
    let relay = HttpsRelayBuilder::new("127.0.0.1:8443", "cert.pem", "key.pem")?
        .control_plane("127.0.0.1:4443")?
        .jwt_secret(b"example-secret-key")
        .storage(Arc::new(InMemoryTunnelStorage::new()))
        .domain_provider(domain_provider.clone())
        .certificate_provider(Arc::new(SelfSignedCertificateProvider))
        .build()?;

    println!("✅ Relay configured with sticky domain assignment\n");

    let mut relay_handle = tokio::spawn(async move { relay.run().await });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Step 4: Generate auth token
    println!("📝 Step 3: Generating authentication tokens...");
    let auth_token_client1 = generate_token("client-app-1", b"example-secret-key", 24)?;
    let _auth_token_client2 = generate_token("client-app-2", b"example-secret-key", 24)?;
    println!("✅ Token 1 for client-app-1");
    println!("✅ Token 2 for client-app-2 (for demonstration only)\n");

    // Step 5: Create local HTTP server
    println!("📝 Step 4: Starting local HTTP server...");
    let app = Router::new()
        .route("/", get(|| async { "✅ Hello from sticky domain app!" }))
        .route("/status", get(|| async { "Service is running" }))
        .into_make_service();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;
    let local_port = local_addr.port();
    println!("✅ Server running on {}\n", local_addr);

    tokio::spawn(async move {
        let server = axum::serve(listener, app);
        let _ = server.await;
    });

    // Step 6: Connect first client
    println!("📝 Step 5: Connecting clients with sticky domains...\n");
    println!("CLIENT 1: Connecting with auth_token for 'client-app-1'");

    let tunnel_config_1 = TunnelConfig {
        local_host: "127.0.0.1".to_string(),
        protocols: vec![ProtocolConfig::Https {
            local_port,
            subdomain: None, // Sticky assignment based on token
            custom_domain: None,
        }],
        auth_token: auth_token_client1,
        exit_node: ExitNodeConfig::Custom("127.0.0.1:4443".to_string()),
        failover: false,
        connection_timeout: std::time::Duration::from_secs(5),
        preferred_transport: None,
        http_auth: HttpAuthConfig::None,
        ip_allowlist: Vec::new(),
    };

    match TunnelClient::connect(tunnel_config_1).await {
        Ok(client) => {
            if let Some(url) = client.public_url() {
                println!("✅ CLIENT 1 connected!");
                println!("   Public URL: {}\n", url);

                println!("📊 Sticky Domain Benefits:");
                println!("===========================");
                println!("   ✅ Same client always gets same subdomain");
                println!("   ✅ No DNS updates on reconnection");
                println!("   ✅ Stable for load balancers and proxies");
                println!("   ✅ Works with monitoring/alerting systems\n");

                println!("🔄 How It Works:");
                println!("=================");
                println!("   When CLIENT 1 reconnects:");
                println!("   - Token identifies client as 'client-app-1'");
                println!("   - Port is {} (same local port)", local_port);
                println!(
                    "   - Relay looks up: ('client-app-1', {}) → {}",
                    local_port,
                    url.split("://").nth(1).unwrap_or("?")
                );
                println!("   - Returns SAME subdomain (sticky!)");
                println!("   - DNS records don't need updating\n");

                println!("📍 Current Sticky Mappings:");
                println!("============================");
                domain_provider.show_mappings();
                println!();

                println!("💡 Use Cases:");
                println!("==============");
                println!("   1. Cloud Load Balancers");
                println!("      - Backend hostname stability");
                println!("      - No DNS cache invalidation");
                println!();
                println!("   2. Reverse Proxies");
                println!("      - Consistent routing rules");
                println!("      - Predictable upstream URLs");
                println!();
                println!("   3. Monitoring Systems");
                println!("      - Persistent alert endpoints");
                println!("      - Stable webhook URLs");
                println!();
                println!("   4. CI/CD Integration");
                println!("      - Fixed deployment URLs");
                println!("      - Reproducible builds");
                println!();
                println!("🔐 Client Identification:");
                println!("=========================");
                println!("   Sticky assignment uses:");
                println!("   - Auth token payload (extracted client_id)");
                println!("   - Local port number");
                println!("   - Combination = Unique, persistent key\n");

                println!("Press Ctrl+C to stop...\n");
            }

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
            eprintln!("❌ Failed to connect: {}", e);
        }
    }

    println!("✅ Example completed!\n");
    Ok(())
}

// ============================================================================
// NOTES FOR PRODUCTION USE
// ============================================================================

/*
Sticky Domain Provider Implementation Notes:

1. CLIENT IDENTIFICATION
   - Currently uses token subject/claim as client_id
   - Alternative: Use client certificate CN or API key
   - Ensure client_id is stable across sessions

2. PERSISTENCE
   - Current implementation: In-memory HashMap
   - For production: Store in database
   - Load mappings on relay startup
   - Example table:
     CREATE TABLE sticky_domains (
       client_id VARCHAR(255),
       port INT,
       subdomain VARCHAR(63),
       created_at TIMESTAMP,
       PRIMARY KEY(client_id, port)
     );

3. CLEANUP
   - Remove stale mappings periodically
   - Track last_accessed timestamp
   - Delete entries > 30 days idle (configurable)

4. VALIDATION
   - Ensure client_id is consistent with token
   - Validate token before assigning sticky domain
   - Audit trail: log domain assignments

5. SECURITY CONSIDERATIONS
   - Client cannot request specific subdomains
   - Prevents subdomain squatting
   - Client only gets deterministic domain
   - Hash-based variant available if needed

6. HASH-BASED VARIANT
   Use hash instead of readable client_id:

   let hash = format!("{:x}", md5::compute(
       format!("{}:{}", client_id, port)
   ));
   format!("sticky-{}", &hash[..12])

   Results in: sticky-a3f8c2d9e1b4

7. SCALING
   - If multiple relay instances:
     → Use shared database for sticky_map
     → All instances look up same domain
   - If local-only (single instance):
     → In-memory HashMap sufficient
     → Clear on restart (acceptable?)
*/
