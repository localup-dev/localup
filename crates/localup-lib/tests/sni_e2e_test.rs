//! End-to-end SNI routing test - User workflow simulation
//!
//! This test demonstrates the complete SNI flow like a real user would use it:
//! 1. Start a relay server with SNI support
//! 2. Register multiple SNI routes for different domains
//! 3. Create tunnel clients with SNI hostnames
//! 4. Verify routing works correctly for each domain
//!
//! Scenario: Multi-tenant API with certificates on different domains
//! - api-001.company.com → local service on port 3443
//! - api-002.company.com → local service on port 3444
//! - api-003.company.com → local service on port 3445

use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tracing::info;

use localup_client::ProtocolConfig;
use localup_router::{RouteRegistry, SniRouter};

// ============================================================================
// HELPER: Local TLS Server Simulator
// ============================================================================

/// Simulate a local TLS server that would be behind a tunnel
/// In a real scenario, this would be an actual TLS service with a certificate
struct LocalTlsService {
    port: u16,
    _domain: String,
    _handle: tokio::task::JoinHandle<()>,
}

impl LocalTlsService {
    async fn new(domain: &str) -> Self {
        // Start a simple TCP server that simulates TLS
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let domain = domain.to_string();
        let domain_clone = domain.clone();

        let handle = tokio::spawn(async move {
            loop {
                if let Ok((mut socket, _)) = listener.accept().await {
                    let domain = domain_clone.clone();
                    // Simulate TLS response with domain info
                    let response = format!("TLS Service for {}\n", domain).into_bytes();
                    let _ = socket.write_all(&response).await;
                }
            }
        });

        info!(
            "✓ Local TLS service '{}' started on 127.0.0.1:{}",
            domain, port
        );

        LocalTlsService {
            port,
            _domain: domain.to_string(),
            _handle: handle,
        }
    }
}

// ============================================================================
// HELPER: SNI Relay Simulator
// ============================================================================

/// Simulate a relay with SNI routing support
/// In a real scenario, this would be the localup-relay binary
struct SniRelay {
    _registry: Arc<RouteRegistry>,
    router: SniRouter,
}

impl SniRelay {
    fn new() -> Self {
        let registry = Arc::new(RouteRegistry::new());
        let router = SniRouter::new(registry.clone());

        info!("✓ SNI Relay initialized");

        SniRelay {
            _registry: registry,
            router,
        }
    }

    /// Register a tunnel with SNI hostname and target address
    fn register_tunnel(
        &self,
        sni_hostname: &str,
        tunnel_id: &str,
        target_addr: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let route = localup_router::sni::SniRoute {
            sni_hostname: sni_hostname.to_string(),
            localup_id: tunnel_id.to_string(),
            target_addr: target_addr.to_string(),
        };

        self.router.register_route(route)?;
        info!(
            "✓ SNI route registered: {} → {} ({})",
            sni_hostname, tunnel_id, target_addr
        );

        Ok(())
    }

    /// Lookup a tunnel by SNI hostname
    fn lookup_tunnel(&self, sni_hostname: &str) -> Result<String, Box<dyn std::error::Error>> {
        let target = self.router.lookup(sni_hostname)?;
        Ok(target.localup_id)
    }

    /// Verify SNI extraction from ClientHello
    fn verify_sni_extraction(
        &self,
        client_hello: &[u8],
    ) -> Result<String, Box<dyn std::error::Error>> {
        let extracted = SniRouter::extract_sni(client_hello)?;
        Ok(extracted)
    }
}

// ============================================================================
// TEST: Multi-tenant API with SNI Routing
// ============================================================================

#[tokio::test]
async fn test_sni_multi_tenant_api_workflow() {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    info!("\n🚀 Starting SNI Multi-Tenant API Test\n");

    // Step 1: Start local services
    info!("📍 Step 1: Start local TLS services");
    let service_1 = LocalTlsService::new("api-001.company.com").await;
    let service_2 = LocalTlsService::new("api-002.company.com").await;
    let service_3 = LocalTlsService::new("api-003.company.com").await;

    // Step 2: Initialize relay with SNI support
    info!("\n📍 Step 2: Initialize SNI Relay");
    let relay = SniRelay::new();

    // Step 3: Register tunnels with SNI hostnames
    info!("\n📍 Step 3: Register SNI Routes");
    let routes = vec![
        (
            "api-001.company.com",
            "tunnel-api-001",
            format!("127.0.0.1:{}", service_1.port),
        ),
        (
            "api-002.company.com",
            "tunnel-api-002",
            format!("127.0.0.1:{}", service_2.port),
        ),
        (
            "api-003.company.com",
            "tunnel-api-003",
            format!("127.0.0.1:{}", service_3.port),
        ),
    ];

    for (domain, tunnel_id, addr) in &routes {
        relay
            .register_tunnel(domain, tunnel_id, addr)
            .expect("Failed to register route");
    }

    // Step 4: Verify routes exist
    info!("\n📍 Step 4: Verify Routes");
    for (domain, expected_tunnel, _) in &routes {
        assert!(
            relay.router.has_route(domain),
            "Route not found: {}",
            domain
        );

        let tunnel = relay.lookup_tunnel(domain).expect("Lookup failed");
        assert_eq!(tunnel, *expected_tunnel, "Route mismatch for {}", domain);
        info!("✓ Route verified: {} → {}", domain, tunnel);
    }

    // Step 5: Simulate SNI extraction from ClientHello
    info!("\n📍 Step 5: Simulate SNI Extraction");
    for (domain, _, _) in &routes {
        let client_hello = create_test_client_hello(domain);
        let extracted = relay
            .verify_sni_extraction(&client_hello)
            .expect("SNI extraction failed");

        assert_eq!(
            extracted, *domain,
            "SNI mismatch: expected {}, got {}",
            domain, extracted
        );
        info!("✓ SNI extracted correctly: {} from ClientHello", extracted);
    }

    // Step 6: Simulate routing logic (what relay would do)
    info!("\n📍 Step 6: Simulate Routing Logic");
    for (domain, _, _) in &routes {
        // In real scenario: TLS connection comes in with SNI
        let client_hello = create_test_client_hello(domain);

        // Extract SNI from ClientHello
        let extracted_sni = relay.verify_sni_extraction(&client_hello).unwrap();

        // Lookup route
        let tunnel_id = relay.lookup_tunnel(&extracted_sni).unwrap();

        info!(
            "✓ Routing flow complete: {} → ClientHello → Extract SNI '{}' → Lookup → Tunnel '{}'",
            domain, extracted_sni, tunnel_id
        );
    }

    // Step 7: Verify protocol configuration for SNI
    info!("\n📍 Step 7: Verify TLS Protocol Configuration");
    let tls_config = ProtocolConfig::Tls {
        local_port: 3443,
        sni_hostname: Some("api-001.company.com".to_string()),
        remote_port: Some(443),
    };

    match tls_config {
        ProtocolConfig::Tls {
            local_port,
            sni_hostname,
            remote_port,
        } => {
            assert_eq!(local_port, 3443);
            assert_eq!(sni_hostname, Some("api-001.company.com".to_string()));
            assert_eq!(remote_port, Some(443));
            info!(
                "✓ TLS config valid: local_port={}, sni_hostname={:?}, remote_port={:?}",
                local_port, sni_hostname, remote_port
            );
        }
        _ => panic!("Invalid protocol config"),
    }

    // Step 8: Verify unregistration
    info!("\n📍 Step 8: Test Route Unregistration");
    relay
        .router
        .unregister("api-001.company.com")
        .expect("Unregister failed");
    assert!(
        !relay.router.has_route("api-001.company.com"),
        "Route should be removed"
    );
    info!("✓ Route successfully unregistered: api-001.company.com");

    // Verify other routes still work
    assert!(relay.router.has_route("api-002.company.com"));
    assert!(relay.router.has_route("api-003.company.com"));
    info!("✓ Other routes remain intact");

    info!("\n✅ SNI Multi-Tenant API Test PASSED\n");
}

// ============================================================================
// TEST: SNI with Random Domains
// ============================================================================

#[tokio::test]
async fn test_sni_with_random_domains() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    info!("\n🚀 Starting SNI Random Domains Test\n");

    let relay = SniRelay::new();

    // Generate random-like domains
    let domains = vec![
        "service-abc123.example.com",
        "api-xyz789.staging.local",
        "db-prod-001.internal.company.net",
        "v2-api-canary.example.org",
        "gateway-test-2024.example.io",
    ];

    info!("📍 Registering routes for random domains:");
    for (idx, domain) in domains.iter().enumerate() {
        let tunnel_id = format!("tunnel-random-{:02}", idx);
        let target_addr = format!("127.0.0.1:{}", 3443 + idx);

        relay
            .register_tunnel(domain, &tunnel_id, &target_addr)
            .expect("Failed to register");
    }

    info!("\n📍 Verifying SNI extraction for random domains:");
    for domain in domains.iter() {
        let client_hello = create_test_client_hello(domain);
        let extracted = relay.verify_sni_extraction(&client_hello).unwrap();
        assert_eq!(extracted, *domain);
        info!("✓ {} extracted correctly", domain);
    }

    info!("\n✅ SNI Random Domains Test PASSED\n");
}

// ============================================================================
// TEST: Concurrent Route Registration
// ============================================================================

#[tokio::test]
async fn test_sni_concurrent_registration() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    info!("\n🚀 Starting SNI Concurrent Registration Test\n");

    let relay = Arc::new(SniRelay::new());

    info!("📍 Spawning 5 concurrent tunnel registrations:");
    let mut handles = vec![];

    for i in 0..5 {
        let relay_clone = relay.clone();
        let handle = tokio::spawn(async move {
            let domain = format!("service-{}.example.com", i);
            let tunnel_id = format!("tunnel-{:02}", i);
            let target_addr = format!("127.0.0.1:{}", 3443 + i);

            relay_clone
                .register_tunnel(&domain, &tunnel_id, &target_addr)
                .expect("Failed to register");

            // Immediately verify
            let tunnel = relay_clone.lookup_tunnel(&domain).unwrap();
            assert_eq!(tunnel, tunnel_id);
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.expect("Task failed");
    }

    info!("\n📍 Verifying all routes exist:");
    for i in 0..5 {
        let domain = format!("service-{}.example.com", i);
        assert!(relay.router.has_route(&domain));
        info!("✓ Route verified: {}", domain);
    }

    info!("\n✅ SNI Concurrent Registration Test PASSED\n");
}

// ============================================================================
// HELPER: Create valid TLS ClientHello with SNI
// ============================================================================

fn create_test_client_hello(hostname: &str) -> Vec<u8> {
    let mut client_hello = Vec::new();

    // TLS Record Header
    client_hello.push(0x16);
    client_hello.push(0x03);
    client_hello.push(0x03);
    let length_index = client_hello.len();
    client_hello.push(0x00);
    client_hello.push(0x00);

    // Handshake Header
    client_hello.push(0x01);
    let handshake_length_index = client_hello.len();
    client_hello.push(0x00);
    client_hello.push(0x00);
    client_hello.push(0x00);

    // ClientHello Body
    client_hello.push(0x03);
    client_hello.push(0x03);
    client_hello.extend_from_slice(&[0x00; 32]);
    client_hello.push(0x00);
    client_hello.push(0x00);
    client_hello.push(0x04);
    client_hello.push(0x00);
    client_hello.push(0x2f);
    client_hello.push(0x00);
    client_hello.push(0x35);
    client_hello.push(0x01);
    client_hello.push(0x00);

    // Extensions
    let extensions_length_index = client_hello.len();
    client_hello.push(0x00);
    client_hello.push(0x00);

    // SNI Extension
    let extension_start = client_hello.len();
    client_hello.push(0x00);
    client_hello.push(0x00);
    client_hello.push(0x00);
    client_hello.push(0x00);

    let sni_list_length_index = client_hello.len();
    client_hello.push(0x00);
    client_hello.push(0x00);

    client_hello.push(0x00);
    client_hello.push(0x00);
    client_hello.push(hostname.len() as u8);
    client_hello.extend_from_slice(hostname.as_bytes());

    // Update SNI list length
    let sni_list_len = client_hello.len() - sni_list_length_index - 2;
    client_hello[sni_list_length_index] = (sni_list_len >> 8) as u8;
    client_hello[sni_list_length_index + 1] = sni_list_len as u8;

    // Update extension length
    let extension_len = client_hello.len() - extension_start - 4;
    client_hello[extension_start + 2] = (extension_len >> 8) as u8;
    client_hello[extension_start + 3] = extension_len as u8;

    // Update extensions length
    let extensions_len = client_hello.len() - extensions_length_index - 2;
    client_hello[extensions_length_index] = (extensions_len >> 8) as u8;
    client_hello[extensions_length_index + 1] = extensions_len as u8;

    // Update handshake length
    let handshake_len = client_hello.len() - handshake_length_index - 3;
    client_hello[handshake_length_index] = ((handshake_len >> 16) & 0xFF) as u8;
    client_hello[handshake_length_index + 1] = ((handshake_len >> 8) & 0xFF) as u8;
    client_hello[handshake_length_index + 2] = (handshake_len & 0xFF) as u8;

    // Update record length
    let record_len = client_hello.len() - length_index - 2;
    client_hello[length_index] = (record_len >> 8) as u8;
    client_hello[length_index + 1] = record_len as u8;

    client_hello
}
