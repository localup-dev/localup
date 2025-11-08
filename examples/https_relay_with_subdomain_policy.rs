//! Example: HTTPS Relay with Configurable Subdomain Policies
//!
//! This example demonstrates how to configure different subdomain selection policies:
//!
//! 1. **Allow Manual Selection** (SimpleCounterDomainProvider):
//!    - Users can specify custom subdomains or let the relay auto-generate them
//!    - Default behavior - most flexible for development
//!
//! 2. **Restrict to Auto-Generated Only** (RestrictedDomainProvider):
//!    - Only auto-generated subdomains are permitted
//!    - Useful for multi-tenant deployments with controlled subdomain allocation
//!    - Users cannot choose their own subdomains
//!
//! 3. **Custom Policy** (implement DomainProvider trait):
//!    - Implement your own validation rules
//!    - Example: enforce company-specific naming conventions
//!    - Example: require approved subdomains from a whitelist
//!
//! Run this example:
//! ```bash
//! # Try different relay configurations by uncommenting lines below
//! cargo run --example https_relay_with_subdomain_policy
//! ```

use axum::{routing::get, Router};
#[allow(unused_imports)]
use localup_lib::{
    generate_self_signed_cert, generate_token, DomainProvider, ExitNodeConfig, HttpsRelayBuilder,
    InMemoryTunnelStorage, ProtocolConfig, RestrictedDomainProvider, SelfSignedCertificateProvider,
    SimpleCounterDomainProvider, TunnelClient, TunnelConfig,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let _ = rustls::crypto::ring::default_provider().install_default();

    println!("🚀 HTTPS Relay with Subdomain Policy Configuration");
    println!("====================================================\n");

    // Choose which policy to use by uncommenting one of these:

    // ===== OPTION 1: Allow Manual Subdomain Selection (Default) =====
    println!("📋 Configuration: Allow Manual Subdomains");
    println!("   → Users can specify custom subdomains");
    println!("   → Or relay auto-generates if not provided\n");

    let domain_provider = Arc::new(SimpleCounterDomainProvider::new());
    let allow_manual = domain_provider.allow_manual_subdomain();
    println!("   allow_manual_subdomain: {}\n", allow_manual);

    // ===== OPTION 2: Restrict to Auto-Generated Only =====
    // Uncomment this block to use RestrictedDomainProvider instead:
    /*
    println!("📋 Configuration: Restrict to Auto-Generated Subdomains Only");
    println!("   → Only auto-generated subdomains are permitted");
    println!("   → User-specified subdomains are rejected\n");

    let domain_provider = Arc::new(RestrictedDomainProvider::new());
    let allow_manual = domain_provider.allow_manual_subdomain();
    println!("   allow_manual_subdomain: {}\n", allow_manual);
    */

    // ===== OPTION 3: Custom Policy =====
    // Uncomment this block to use a custom provider:
    /*
    println!("📋 Configuration: Custom Subdomain Policy");
    println!("   → Enforce company-specific naming rules\n");

    let domain_provider = Arc::new(CompanyPrefixedDomainProvider::new("acme"));
    println!("   Subdomains must start with 'acme-'\n");
    */

    // Step 1: Generate certificates
    println!("📝 Step 1: Generating self-signed certificates...");
    let cert = generate_self_signed_cert()?;
    cert.save_to_files("cert.pem", "key.pem")?;
    println!("✅ Certificates ready\n");

    // Step 2: Build relay with selected domain provider
    println!("📝 Step 2: Building HTTPS relay with subdomain policy...");
    let relay = HttpsRelayBuilder::new("127.0.0.1:8443", "cert.pem", "key.pem")?
        .control_plane("127.0.0.1:4443")?
        .jwt_secret(b"example-secret-key")
        .storage(Arc::new(InMemoryTunnelStorage::new()))
        .domain_provider(domain_provider.clone()) // Use selected policy here
        .certificate_provider(Arc::new(SelfSignedCertificateProvider))
        .build()?;

    println!("✅ Relay configured with subdomain policy\n");

    let mut relay_handle = tokio::spawn(async move { relay.run().await });
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Step 3: Generate auth token
    println!("📝 Step 3: Generating authentication token...");
    let auth_token = generate_token("demo-app", b"example-secret-key", 24)?;
    println!("✅ Token generated\n");

    // Step 4: Create local HTTP server
    println!("📝 Step 4: Starting local HTTP server...");
    let app = Router::new()
        .route("/", get(|| async { "✅ Hello from demo app!" }))
        .into_make_service();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;
    let local_port = local_addr.port();
    println!("✅ Server running on {}\n", local_addr);

    tokio::spawn(async move {
        let server = axum::serve(listener, app);
        let _ = server.await;
    });

    // Step 5: Demonstrate subdomain validation
    println!("📝 Step 5: Testing subdomain validation...");
    println!(
        "   Checking if manual subdomains are allowed: {}",
        allow_manual
    );
    println!();

    // Show what happens with different subdomain attempts
    let test_subdomains = vec!["my-app", "api-v2", "tunnel123"];

    for subdomain in test_subdomains {
        match domain_provider.validate_subdomain(subdomain) {
            Ok(()) => {
                println!("   ✅ '{}' is valid", subdomain);
            }
            Err(e) => {
                println!("   ❌ '{}' is invalid: {}", subdomain, e);
            }
        }
    }
    println!();

    // Step 6: Connect tunnel client
    println!("📝 Step 6: Connecting TunnelClient to relay...");

    // Use auto-generated subdomain (works with all policies)
    let tunnel_config = TunnelConfig {
        local_host: "127.0.0.1".to_string(),
        protocols: vec![ProtocolConfig::Https {
            local_port,
            subdomain: None, // Let relay auto-generate
            custom_domain: None,
        }],
        auth_token,
        exit_node: ExitNodeConfig::Custom("127.0.0.1:4443".to_string()),
        failover: false,
        connection_timeout: std::time::Duration::from_secs(5),
    };

    match TunnelClient::connect(tunnel_config).await {
        Ok(client) => {
            if let Some(url) = client.public_url() {
                println!("✅ TunnelClient connected!");
                println!("   Public URL: {}\n", url);

                println!("🧪 Subdomain Policy Summary:");
                println!("===========================");
                println!("   Allow manual selection: {}", allow_manual);
                if allow_manual {
                    println!("   Users can specify: --subdomain my-custom-app");
                } else {
                    println!("   Users cannot specify subdomains (auto-generated only)");
                }
                println!();
                println!("   Validation rules:");
                println!("   - 3-63 characters");
                println!("   - Alphanumeric and hyphens only");
                println!("   - No leading/trailing hyphens");
                println!();
                println!("💡 Configuration Options:");
                println!("   1. SimpleCounterDomainProvider (current)");
                println!("      - allow_manual_subdomain() = true");
                println!("      - Users: app specify custom subdomains");
                println!();
                println!("   2. RestrictedDomainProvider");
                println!("      - allow_manual_subdomain() = false");
                println!("      - Users: only auto-generated subdomains");
                println!();
                println!("   3. Custom Implementation");
                println!("      - Implement DomainProvider trait");
                println!("      - Custom validation and generation logic");
                println!();
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

    Ok(())
}

// ===== OPTIONAL: Custom Domain Provider Implementation =====
// Uncomment to use in OPTION 3 above

/*
/// Example custom domain provider with company prefix requirement
struct CompanyPrefixedDomainProvider {
    prefix: String,
    counter: std::sync::Arc<std::sync::Mutex<u64>>,
    reserved: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
}

impl CompanyPrefixedDomainProvider {
    fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            counter: std::sync::Arc::new(std::sync::Mutex::new(0)),
            reserved: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
        }
    }
}

#[async_trait::async_trait]
impl DomainProvider for CompanyPrefixedDomainProvider {
    async fn generate_subdomain(&self) -> Result<String, DomainProviderError> {
        let mut counter = self.counter.lock().unwrap();
        *counter += 1;
        Ok(format!("{}-{}", self.prefix, counter))
    }

    async fn generate_public_url(
        &self,
        subdomain: Option<&str>,
        _port: Option<u16>,
        protocol: &str,
        public_domain: &str,
    ) -> Result<String, DomainProviderError> {
        match protocol {
            "https" | "http" => {
                subdomain
                    .map(|s| format!("{}://{}.{}", protocol, s, public_domain))
                    .ok_or_else(|| ConfigError::DomainError("Subdomain required".into()))
            }
            _ => Err(DomainProviderError::DomainError("Unsupported protocol".into())),
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

    /// Allow manual selection, but with validation
    fn allow_manual_subdomain(&self) -> bool {
        true
    }

    /// Validate that subdomain follows company naming convention
    fn validate_subdomain(&self, subdomain: &str) -> Result<(), DomainProviderError> {
        // First, run default validation
        <dyn DomainProvider>::validate_subdomain(self, subdomain)?;

        // Then, check company prefix
        if !subdomain.starts_with(&format!("{}-", self.prefix)) {
            return Err(DomainProviderError::InvalidSubdomain(format!(
                "Subdomain must start with '{}-'",
                self.prefix
            )));
        }

        Ok(())
    }
}
*/
