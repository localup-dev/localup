//! End-to-end test for SNI routing with certificates on random domains
//!
//! This test verifies:
//! 1. SNI extraction from TLS ClientHello for various domains
//! 2. Route registration and lookup
//! 3. SNI-based tunnel routing with multiple simultaneous routes
//! 4. Concurrent access to SNI router
//! 5. Proper error handling for malformed ClientHellos

use localup_proto::IpFilter;
use localup_router::{RouteRegistry, SniRouter};
use std::sync::Arc;

#[test]
fn test_sni_extraction_and_routing() {
    // Test 1: Extract SNI from a real TLS ClientHello
    let client_hello = create_test_client_hello("api.example.com");

    let result = SniRouter::extract_sni(&client_hello);
    assert!(result.is_ok(), "Failed to extract SNI");
    assert_eq!(result.unwrap(), "api.example.com");
}

#[test]
fn test_sni_routing_workflow() {
    // Simulate a complete SNI routing workflow with multiple services
    let registry = Arc::new(RouteRegistry::new());
    let router = SniRouter::new(registry.clone());

    // Step 1: Register routes for multiple SNI hostnames
    let api_route = localup_router::sni::SniRoute {
        sni_hostname: "api.example.com".to_string(),
        localup_id: "tunnel-api-001".to_string(),
        target_addr: "127.0.0.1:3443".to_string(),
        ip_filter: IpFilter::new(),
    };

    let web_route = localup_router::sni::SniRoute {
        sni_hostname: "web.example.com".to_string(),
        localup_id: "tunnel-web-001".to_string(),
        target_addr: "127.0.0.1:3444".to_string(),
        ip_filter: IpFilter::new(),
    };

    let db_route = localup_router::sni::SniRoute {
        sni_hostname: "db.example.com".to_string(),
        localup_id: "tunnel-db-001".to_string(),
        target_addr: "127.0.0.1:3445".to_string(),
        ip_filter: IpFilter::new(),
    };

    router
        .register_route(api_route)
        .expect("Failed to register api route");
    router
        .register_route(web_route)
        .expect("Failed to register web route");
    router
        .register_route(db_route)
        .expect("Failed to register db route");

    // Step 2: Verify routes exist
    assert!(router.has_route("api.example.com"));
    assert!(router.has_route("web.example.com"));
    assert!(router.has_route("db.example.com"));
    assert!(!router.has_route("unknown.example.com"));

    // Step 3: Lookup routes
    let api_target = router
        .lookup("api.example.com")
        .expect("Failed to lookup api route");
    assert_eq!(api_target.localup_id, "tunnel-api-001");
    assert_eq!(api_target.target_addr, "127.0.0.1:3443");

    let web_target = router
        .lookup("web.example.com")
        .expect("Failed to lookup web route");
    assert_eq!(web_target.localup_id, "tunnel-web-001");
    assert_eq!(web_target.target_addr, "127.0.0.1:3444");

    let db_target = router
        .lookup("db.example.com")
        .expect("Failed to lookup db route");
    assert_eq!(db_target.localup_id, "tunnel-db-001");
    assert_eq!(db_target.target_addr, "127.0.0.1:3445");

    // Step 4: Verify unregistering works
    router
        .unregister("api.example.com")
        .expect("Failed to unregister");
    assert!(!router.has_route("api.example.com"));
    assert!(router.lookup("api.example.com").is_err());

    // Step 5: Verify other routes still work
    assert!(router.has_route("web.example.com"));
    assert!(router.has_route("db.example.com"));
}

#[test]
fn test_sni_extraction_with_multiple_random_domains() {
    // Test SNI extraction with various domain formats (simulating random domains)
    let test_cases = vec![
        "api.example.com",
        "web.example.com",
        "db.example.com",
        "v1-api.staging.example.com",
        "my-service-123.local",
        "localhost",
        "service-12345.example.org",
        "nested.sub.domain.example.net",
        "hyphenated-service-name.example.com",
        "numeric-123-service-456.example.io",
    ];

    for hostname in test_cases {
        let client_hello = create_test_client_hello(hostname);
        let result = SniRouter::extract_sni(&client_hello);

        assert!(
            result.is_ok(),
            "Failed to extract SNI for hostname: {}",
            hostname
        );
        assert_eq!(
            result.unwrap(),
            hostname,
            "Extracted SNI doesn't match expected hostname"
        );
    }
}

#[test]
fn test_sni_with_certificates_on_different_domains() {
    // Test routing when each tunnel has its own certificate/domain
    let registry = Arc::new(RouteRegistry::new());
    let router = SniRouter::new(registry);

    // Simulate tunnels with certificates on different domains
    let domains = vec![
        ("api-001.company.com", "127.0.0.1:3443"),
        ("api-002.company.com", "127.0.0.1:3444"),
        ("api-003.company.com", "127.0.0.1:3445"),
        ("service-a.internal.local", "127.0.0.1:3446"),
        ("service-b.internal.local", "127.0.0.1:3447"),
    ];

    // Register all routes
    for (idx, (domain, addr)) in domains.iter().enumerate() {
        let route = localup_router::sni::SniRoute {
            sni_hostname: domain.to_string(),
            localup_id: format!("tunnel-{:03}", idx),
            target_addr: addr.to_string(),
            ip_filter: IpFilter::new(),
        };
        router
            .register_route(route)
            .expect("Failed to register route");
    }

    // Verify all routes work and extract SNI correctly
    for (domain, expected_addr) in domains.iter() {
        let client_hello = create_test_client_hello(domain);
        let extracted_sni = SniRouter::extract_sni(&client_hello).expect("Failed to extract SNI");
        assert_eq!(&extracted_sni, domain);

        let target = router.lookup(domain).expect("Failed to lookup route");
        assert_eq!(target.target_addr, *expected_addr);
    }
}

#[test]
fn test_sni_extraction_without_sni_extension() {
    // ClientHello without SNI extension should fail
    let client_hello = vec![
        // TLS Record Header
        0x16, 0x03, 0x01, 0x00, 0x4A, // Handshake Header
        0x01, 0x00, 0x00, 0x46, // ClientHello Body (minimal, no extensions)
        0x03, 0x03, // Random (32 bytes)
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x1e, 0x1f, // Session ID length
        0x00, // Cipher suites length
        0x00, 0x02, // Cipher suite
        0x00, 0x2f, // Compression methods length
        0x01, // Compression method
        0x00, // Extensions length (no extensions)
        0x00, 0x00,
    ];

    let result = SniRouter::extract_sni(&client_hello);
    assert!(result.is_err(), "Should fail when SNI extension is missing");
}

#[test]
fn test_sni_malformed_client_hello() {
    // Test various malformed ClientHellos
    let malformed_cases = vec![
        vec![0x16, 0x03, 0x01], // Too short
        vec![0x16],             // Way too short
        vec![],                 // Empty
    ];

    for malformed in malformed_cases {
        let result = SniRouter::extract_sni(&malformed);
        assert!(
            result.is_err(),
            "Should reject malformed ClientHello: {:?}",
            malformed
        );
    }
}

#[test]
fn test_concurrent_sni_routing() {
    use std::thread;

    let registry = Arc::new(RouteRegistry::new());
    let router = Arc::new(SniRouter::new(registry));

    // Register multiple routes from different threads concurrently
    let mut handles = vec![];

    for i in 0..10 {
        let router_clone = router.clone();
        let handle = thread::spawn(move || {
            let hostname = format!("service-{}.example.com", i);
            let tunnel_id = format!("tunnel-{:03}", i);
            let target_addr = format!("127.0.0.1:{}", 3000 + i);

            let route = localup_router::sni::SniRoute {
                sni_hostname: hostname.clone(),
                localup_id: tunnel_id.clone(),
                target_addr: target_addr.clone(),
                ip_filter: IpFilter::new(),
            };

            router_clone.register_route(route).unwrap();

            // Verify immediately
            let result = router_clone.lookup(&hostname).unwrap();
            assert_eq!(result.localup_id, tunnel_id);
            assert_eq!(result.target_addr, target_addr);
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all routes are present
    for i in 0..10 {
        let hostname = format!("service-{}.example.com", i);
        assert!(router.has_route(&hostname));
    }
}

#[test]
fn test_sni_with_unicode_domains() {
    // Test SNI extraction with internationalized domain names (IDN)
    // These are typically punycode encoded in TLS, but let's test ASCII-compatible ones
    let domains = vec![
        "example.com",
        "test-domain.example.com",
        "multi-word-domain-123.example.com",
    ];

    for domain in domains {
        let client_hello = create_test_client_hello(domain);
        let result = SniRouter::extract_sni(&client_hello);
        assert!(result.is_ok(), "Failed to extract SNI for: {}", domain);
        assert_eq!(result.unwrap(), domain);
    }
}

#[test]
fn test_sni_route_persistence() {
    // Test that routes persist and survive multiple operations
    let registry = Arc::new(RouteRegistry::new());
    let router = SniRouter::new(registry);

    let route = localup_router::sni::SniRoute {
        sni_hostname: "persistent.example.com".to_string(),
        localup_id: "tunnel-persistent".to_string(),
        target_addr: "127.0.0.1:3443".to_string(),
        ip_filter: IpFilter::new(),
    };

    router.register_route(route).unwrap();

    // Verify multiple times
    for _ in 0..5 {
        assert!(router.has_route("persistent.example.com"));
        let target = router.lookup("persistent.example.com").unwrap();
        assert_eq!(target.localup_id, "tunnel-persistent");
    }

    // Unregister and verify gone
    router.unregister("persistent.example.com").unwrap();
    assert!(!router.has_route("persistent.example.com"));
}

// Helper function to create a TLS ClientHello with SNI
fn create_test_client_hello(hostname: &str) -> Vec<u8> {
    let mut client_hello = Vec::new();

    // TLS Record Header (5 bytes)
    client_hello.push(0x16); // Content type: Handshake
    client_hello.push(0x03); // Version TLS 1.2 (major)
    client_hello.push(0x03); // Version TLS 1.2 (minor)

    // Placeholder for record length
    let length_index = client_hello.len();
    client_hello.push(0x00);
    client_hello.push(0x00);

    // Handshake Header (4 bytes)
    client_hello.push(0x01); // Msg type: ClientHello
    let handshake_length_index = client_hello.len();
    client_hello.push(0x00);
    client_hello.push(0x00);
    client_hello.push(0x00);

    // ClientHello Protocol Version (2 bytes)
    client_hello.push(0x03); // TLS 1.2
    client_hello.push(0x03);

    // Random (32 bytes)
    client_hello.extend_from_slice(&[0x00; 32]);

    // Session ID length (1 byte)
    client_hello.push(0x00);

    // Cipher suites length (2 bytes)
    client_hello.push(0x00);
    client_hello.push(0x04);

    // Cipher suites (2 x 2 bytes)
    client_hello.push(0x00);
    client_hello.push(0x2f);
    client_hello.push(0x00);
    client_hello.push(0x35);

    // Compression methods length (1 byte)
    client_hello.push(0x01);

    // Compression method
    client_hello.push(0x00);

    // Extensions length placeholder
    let extensions_length_index = client_hello.len();
    client_hello.push(0x00);
    client_hello.push(0x00);

    // SNI Extension
    let extension_start = client_hello.len();
    client_hello.push(0x00); // Type: server_name
    client_hello.push(0x00);
    client_hello.push(0x00); // Length (will update)
    client_hello.push(0x00);

    // Server name list
    let sni_list_length_index = client_hello.len();
    client_hello.push(0x00);
    client_hello.push(0x00);

    // Server name entry
    client_hello.push(0x00); // Type: host_name
    client_hello.push(0x00); // Name length (high byte)
    client_hello.push(hostname.len() as u8); // Name length (low byte)
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
