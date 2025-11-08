//! Example: Custom Domain Provider with Company-Specific Naming Rules
//!
//! This example demonstrates how to implement a custom DomainProvider that enforces
//! company-specific naming conventions. Instead of allowing arbitrary subdomains,
//! users must follow your organization's rules.
//!
//! **Custom Policy Rules:**
//! - All subdomains must start with company prefix (e.g., "acme-")
//! - Format: "acme-{service-name}"
//! - Examples: "acme-api", "acme-db", "acme-frontend"
//! - Invalid: "my-api", "ACME-api", "acme_api"
//!
//! Run this example:
//! ```bash
//! cargo run --example https_relay_custom_domain_provider
//! ```

use axum::{routing::get, Router};
use localup_lib::{
    async_trait, generate_self_signed_cert, generate_token, DomainContext, DomainProvider,
    DomainProviderError, ExitNodeConfig, HttpsRelayBuilder, InMemoryTunnelStorage, ProtocolConfig,
    SelfSignedCertificateProvider, TunnelClient, TunnelConfig,
};
use std::sync::{Arc, Mutex};

// ============================================================================
// CUSTOM DOMAIN PROVIDER IMPLEMENTATION
// ============================================================================

/// Company-prefixed domain provider
///
/// Enforces that all subdomains follow the pattern: "{prefix}-{service-name}"
/// Example: "acme-api", "acme-database", "acme-frontend"
struct CompanyPrefixedDomainProvider {
    /// Company prefix (e.g., "acme", "myco", "startup")
    prefix: String,
    /// Counter for auto-generated subdomains
    counter: Arc<Mutex<u64>>,
    /// Set of reserved subdomains to prevent conflicts
    reserved: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl CompanyPrefixedDomainProvider {
    /// Create a new domain provider with the given company prefix
    fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            counter: Arc::new(Mutex::new(0)),
            reserved: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }
}

#[async_trait]
impl DomainProvider for CompanyPrefixedDomainProvider {
    async fn generate_subdomain(
        &self,
        _context: &DomainContext,
    ) -> Result<String, DomainProviderError> {
        let mut counter = self.counter.lock().unwrap();
        *counter += 1;
        Ok(format!("{}-service-{}", self.prefix, counter))
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
                "Only HTTP and HTTPS protocols supported".into(),
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

    /// Allow manual subdomain selection (but with validation)
    fn allow_manual_subdomain(&self) -> bool {
        true
    }

    /// Validate that subdomain follows company naming convention
    fn validate_subdomain(&self, subdomain: &str) -> Result<(), DomainProviderError> {
        // First, run default validation (length, character restrictions, etc.)
        <dyn DomainProvider>::validate_subdomain(self, subdomain)?;

        // Then, check company prefix
        let expected_prefix = format!("{}-", self.prefix);
        if !subdomain.starts_with(&expected_prefix) {
            return Err(DomainProviderError::InvalidSubdomain(format!(
                "Subdomain must start with '{}' (e.g., '{}-api', '{}-db')",
                expected_prefix, self.prefix, self.prefix
            )));
        }

        // Ensure there's a service name after the prefix
        let remaining = &subdomain[expected_prefix.len()..];
        if remaining.is_empty() {
            return Err(DomainProviderError::InvalidSubdomain(format!(
                "Subdomain must have a service name after '{}' (e.g., '{}-myservice')",
                expected_prefix, self.prefix
            )));
        }

        Ok(())
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

    println!("🚀 Custom Domain Provider Example");
    println!("=================================\n");

    // Step 1: Create custom domain provider with company prefix
    println!("📋 Creating custom domain provider");
    println!("   Company prefix: 'acme'");
    println!("   Format: acme-{{service-name}}\n");

    let company_prefix = "acme";
    let domain_provider = Arc::new(CompanyPrefixedDomainProvider::new(company_prefix));

    // Show policy info
    println!("ℹ️  Policy Configuration:");
    println!(
        "   - Allow manual subdomains: {}",
        domain_provider.allow_manual_subdomain()
    );
    println!("   - Validation: Company prefix required\n");

    // Demonstrate validation
    println!("🧪 Testing Subdomain Validation:");
    println!("   Valid examples:");
    for subdomain in &[
        "acme-api",
        "acme-database",
        "acme-frontend",
        "acme-v2-backend",
    ] {
        match domain_provider.validate_subdomain(subdomain) {
            Ok(()) => println!("     ✅ '{}'", subdomain),
            Err(e) => println!("     ❌ '{}': {}", subdomain, e),
        }
    }

    println!("\n   Invalid examples:");
    for subdomain in &["my-api", "api", "ACME-api", "acme_api", "acme-"] {
        match domain_provider.validate_subdomain(subdomain) {
            Ok(()) => println!("     ✅ '{}' (unexpected)", subdomain),
            Err(e) => println!("     ❌ '{}': {}", subdomain, e),
        }
    }
    println!();

    // Step 2: Generate certificates
    println!("📝 Step 1: Generating certificates...");
    let cert = generate_self_signed_cert()?;
    cert.save_to_files("cert.pem", "key.pem")?;
    println!("✅ Ready\n");

    // Step 3: Build relay with custom domain provider
    println!("📝 Step 2: Building HTTPS relay with custom provider...");
    let relay = HttpsRelayBuilder::new("127.0.0.1:8443", "cert.pem", "key.pem")?
        .control_plane("127.0.0.1:4443")?
        .jwt_secret(b"example-secret-key")
        .storage(Arc::new(InMemoryTunnelStorage::new()))
        .domain_provider(domain_provider.clone()) // Use custom provider
        .certificate_provider(Arc::new(SelfSignedCertificateProvider))
        .build()?;

    println!("✅ Relay configured with company naming rules\n");

    let mut relay_handle = tokio::spawn(async move { relay.run().await });
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Step 4: Generate auth token
    println!("📝 Step 3: Generating authentication token...");
    let auth_token = generate_token("acme-demo", b"example-secret-key", 24)?;
    println!("✅ Token generated\n");

    // Step 5: Create local HTTP server
    println!("📝 Step 4: Starting local HTTP server...");
    let app = Router::new()
        .route(
            "/",
            get(|| async { "✅ Hello from ACME Company Application!" }),
        )
        .into_make_service();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;
    let local_port = local_addr.port();
    println!("✅ Server running on {}\n", local_addr);

    tokio::spawn(async move {
        let server = axum::serve(listener, app);
        let _ = server.await;
    });

    // Step 6: Connect tunnel with company-compliant subdomain
    println!("📝 Step 5: Connecting tunnel with company-compliant subdomain...");

    // Note: In a real scenario, the user would specify this:
    //   localup add --subdomain acme-api
    // or let the relay auto-generate:
    //   localup add  # relay assigns "acme-service-1"

    // For this demo, let the relay auto-generate a subdomain
    let tunnel_config = TunnelConfig {
        local_host: "127.0.0.1".to_string(),
        protocols: vec![ProtocolConfig::Https {
            local_port,
            subdomain: None, // Let relay auto-generate: "acme-service-1"
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
                println!("✅ Tunnel connected!\n");
                println!("📊 Tunnel Information:");
                println!("   Public URL: {}", url);
                println!("   Local Server: {}", local_addr);
                println!();
                println!("🎯 Company Naming Rules in Action:");
                println!("===================================");
                println!();
                println!("Allowed subdomain patterns:");
                println!("  ✅ acme-api           (use for API servers)");
                println!("  ✅ acme-db            (use for databases)");
                println!("  ✅ acme-frontend      (use for web apps)");
                println!("  ✅ acme-monitoring    (use for monitoring systems)");
                println!();
                println!("Rejected patterns:");
                println!("  ❌ my-api             (missing company prefix)");
                println!("  ❌ acme                (missing service name)");
                println!("  ❌ ACME-api            (uppercase not allowed)");
                println!("  ❌ acme_api            (underscore not allowed in names)");
                println!();
                println!("💼 Use Cases:");
                println!("  - Multi-team SaaS platform");
                println!("  - Company naming standards enforcement");
                println!("  - Integration with DNS management");
                println!("  - Compliance and audit requirements");
                println!();
                println!("📖 Implementation:");
                println!("  - Extend DomainProvider trait");
                println!("  - Override allow_manual_subdomain()");
                println!("  - Override validate_subdomain() with custom rules");
                println!("  - Default validation still applies (length, chars, etc.)");
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

    println!("✅ Example completed!\n");
    Ok(())
}
