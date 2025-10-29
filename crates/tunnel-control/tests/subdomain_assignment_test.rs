/// Integration test for subdomain assignment
/// Tests both user-provided and auto-generated subdomains
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tracing::info;
use tunnel_control::{PendingRequests, TunnelConnectionManager, TunnelHandler};
use tunnel_proto::{Protocol, TunnelConfig, TunnelMessage};
use tunnel_router::{RouteKey, RouteRegistry};
use tunnel_transport::{
    TransportConnection, TransportConnector, TransportListener, TransportStream,
};
use tunnel_transport_quic::{QuicConfig, QuicConnector, QuicListener};

// Initialize rustls crypto provider once at module load
use std::sync::OnceLock;
static CRYPTO_PROVIDER_INIT: OnceLock<()> = OnceLock::new();

fn init_crypto_provider() {
    CRYPTO_PROVIDER_INIT.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

// Helper to create unique server config for each test to avoid cert file conflicts
fn create_test_server_config(test_name: &str) -> Arc<QuicConfig> {
    use std::env;
    use std::fs;
    use std::io::Write;

    // Create temp directory for test-specific certs
    let temp_dir = env::temp_dir().join(format!("tunnel-test-{}", test_name));
    fs::create_dir_all(&temp_dir).unwrap();

    let cert_path = temp_dir.join("cert.pem");
    let key_path = temp_dir.join("key.pem");

    // Generate self-signed cert
    let cert_data = tunnel_cert::generate_self_signed_cert().unwrap();

    // Write cert and key to test-specific paths
    let mut cert_file = fs::File::create(&cert_path).unwrap();
    cert_file.write_all(cert_data.pem_cert.as_bytes()).unwrap();

    let mut key_file = fs::File::create(&key_path).unwrap();
    key_file.write_all(cert_data.pem_key.as_bytes()).unwrap();

    Arc::new(
        QuicConfig::server_default(cert_path.to_str().unwrap(), key_path.to_str().unwrap())
            .unwrap(),
    )
}

/// Test user-provided subdomain
#[tokio::test(flavor = "multi_thread")]
async fn test_user_provided_subdomain() {
    init_crypto_provider();

    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    info!("🧪 TEST: User-provided subdomain");

    // Setup server
    let registry = Arc::new(RouteRegistry::new());
    let connection_manager = Arc::new(TunnelConnectionManager::new());
    let pending_requests = Arc::new(PendingRequests::new());

    let handler = Arc::new(TunnelHandler::new(
        connection_manager.clone(),
        registry.clone(),
        None,
        "localhost".to_string(),
        pending_requests,
    ));

    let server_config = create_test_server_config("user_provided_subdomain");
    let listener = QuicListener::new("127.0.0.1:0".parse().unwrap(), server_config).unwrap();
    let server_addr = listener.local_addr().unwrap();

    let handler_clone = handler.clone();
    let _server_task = tokio::spawn(async move {
        while let Ok((conn, peer_addr)) = listener.accept().await {
            let handler = handler_clone.clone();
            tokio::spawn(async move {
                handler.handle_connection(Arc::new(conn), peer_addr).await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect as client with user-provided subdomain
    let client_config = Arc::new(QuicConfig::client_insecure());
    let connector = QuicConnector::new(client_config).unwrap();
    let connection = connector.connect(server_addr, "localhost").await.unwrap();
    let connection = Arc::new(connection);

    let tunnel_id = "test-user-subdomain";
    let user_subdomain = "myapp";

    let mut control_stream = connection.open_stream().await.unwrap();

    let connect_msg = TunnelMessage::Connect {
        tunnel_id: tunnel_id.to_string(),
        auth_token: "test-token".to_string(),
        protocols: vec![Protocol::Http {
            subdomain: Some(user_subdomain.to_string()),
        }],
        config: TunnelConfig::default(),
    };

    control_stream.send_message(&connect_msg).await.unwrap();

    // Wait for Connected response
    let response = timeout(Duration::from_secs(3), control_stream.recv_message())
        .await
        .expect("Timeout waiting for Connected")
        .expect("Failed to read message")
        .expect("Empty message");

    match response {
        TunnelMessage::Connected { endpoints, .. } => {
            assert_eq!(endpoints.len(), 1, "Should have 1 endpoint");

            let endpoint = &endpoints[0];
            match &endpoint.protocol {
                Protocol::Http { subdomain } => {
                    let assigned_subdomain =
                        subdomain.as_ref().expect("Subdomain should be assigned");
                    assert_eq!(
                        assigned_subdomain, user_subdomain,
                        "Should use user-provided subdomain"
                    );
                    info!(
                        "✅ Server assigned user-provided subdomain: {}",
                        assigned_subdomain
                    );
                }
                _ => panic!("Expected HTTP protocol"),
            }

            assert_eq!(
                endpoint.public_url,
                format!("https://{}.localhost", user_subdomain),
                "Public URL should match user-provided subdomain"
            );
            info!("✅ Public URL correct: {}", endpoint.public_url);
        }
        other => panic!("Expected Connected, got {:?}", other),
    }

    // Verify route was registered with correct subdomain
    let route_key = RouteKey::HttpHost(format!("{}.localhost", user_subdomain));
    assert!(
        registry.exists(&route_key),
        "Route should be registered with user-provided subdomain"
    );
    info!("✅ Route registered with user-provided subdomain");

    info!("✅ TEST PASSED: User-provided subdomain works correctly");
}

/// Test auto-generated subdomain
#[tokio::test(flavor = "multi_thread")]
async fn test_auto_generated_subdomain() {
    init_crypto_provider();

    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    info!("🧪 TEST: Auto-generated subdomain");

    // Setup server
    let registry = Arc::new(RouteRegistry::new());
    let connection_manager = Arc::new(TunnelConnectionManager::new());
    let pending_requests = Arc::new(PendingRequests::new());

    let handler = Arc::new(TunnelHandler::new(
        connection_manager.clone(),
        registry.clone(),
        None,
        "localhost".to_string(),
        pending_requests,
    ));

    let server_config = create_test_server_config("auto_generated_subdomain");
    let listener = QuicListener::new("127.0.0.1:0".parse().unwrap(), server_config).unwrap();
    let server_addr = listener.local_addr().unwrap();

    let handler_clone = handler.clone();
    let _server_task = tokio::spawn(async move {
        while let Ok((conn, peer_addr)) = listener.accept().await {
            let handler = handler_clone.clone();
            tokio::spawn(async move {
                handler.handle_connection(Arc::new(conn), peer_addr).await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect as client WITHOUT subdomain (None)
    let client_config = Arc::new(QuicConfig::client_insecure());
    let connector = QuicConnector::new(client_config).unwrap();
    let connection = connector.connect(server_addr, "localhost").await.unwrap();
    let connection = Arc::new(connection);

    let tunnel_id = "test-auto-subdomain";

    let mut control_stream = connection.open_stream().await.unwrap();

    let connect_msg = TunnelMessage::Connect {
        tunnel_id: tunnel_id.to_string(),
        auth_token: "test-token".to_string(),
        protocols: vec![Protocol::Http {
            subdomain: None, // ← Auto-generate
        }],
        config: TunnelConfig::default(),
    };

    control_stream.send_message(&connect_msg).await.unwrap();

    // Wait for Connected response
    let response = timeout(Duration::from_secs(3), control_stream.recv_message())
        .await
        .expect("Timeout waiting for Connected")
        .expect("Failed to read message")
        .expect("Empty message");

    match response {
        TunnelMessage::Connected { endpoints, .. } => {
            assert_eq!(endpoints.len(), 1, "Should have 1 endpoint");

            let endpoint = &endpoints[0];
            match &endpoint.protocol {
                Protocol::Http { subdomain } => {
                    let assigned_subdomain = subdomain
                        .as_ref()
                        .expect("Subdomain should be auto-generated");

                    // Auto-generated subdomain should not be empty
                    assert!(
                        !assigned_subdomain.is_empty(),
                        "Auto-generated subdomain should not be empty"
                    );

                    // Should be deterministic based on tunnel_id (usually 6-8 characters)
                    assert!(
                        assigned_subdomain.len() >= 4,
                        "Auto-generated subdomain should be reasonable length"
                    );

                    info!("✅ Server auto-generated subdomain: {}", assigned_subdomain);
                }
                _ => panic!("Expected HTTP protocol"),
            }

            // Public URL should be valid
            assert!(
                endpoint.public_url.starts_with("https://"),
                "Public URL should start with https://"
            );
            assert!(
                endpoint.public_url.contains(".localhost"),
                "Public URL should contain .localhost"
            );
            info!("✅ Public URL correct: {}", endpoint.public_url);
        }
        other => panic!("Expected Connected, got {:?}", other),
    }

    info!("✅ TEST PASSED: Auto-generated subdomain works correctly");
}

/// Test that same tunnel_id gets same auto-generated subdomain (deterministic)
#[tokio::test(flavor = "multi_thread")]
async fn test_deterministic_auto_generation() {
    init_crypto_provider();

    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    info!("🧪 TEST: Deterministic auto-generation");

    // Setup server
    let registry = Arc::new(RouteRegistry::new());
    let connection_manager = Arc::new(TunnelConnectionManager::new());
    let pending_requests = Arc::new(PendingRequests::new());

    let handler = Arc::new(TunnelHandler::new(
        connection_manager.clone(),
        registry.clone(),
        None,
        "localhost".to_string(),
        pending_requests,
    ));

    let server_config = create_test_server_config("deterministic_auto_generation");
    let listener = QuicListener::new("127.0.0.1:0".parse().unwrap(), server_config).unwrap();
    let server_addr = listener.local_addr().unwrap();

    let handler_clone = handler.clone();
    let _server_task = tokio::spawn(async move {
        while let Ok((conn, peer_addr)) = listener.accept().await {
            let handler = handler_clone.clone();
            tokio::spawn(async move {
                handler.handle_connection(Arc::new(conn), peer_addr).await;
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let tunnel_id = "same-tunnel-id";

    // Connect twice with same tunnel_id, expect same auto-generated subdomain
    let mut subdomains = Vec::new();

    for i in 0..2 {
        let client_config = Arc::new(QuicConfig::client_insecure());
        let connector = QuicConnector::new(client_config).unwrap();
        let connection = connector.connect(server_addr, "localhost").await.unwrap();
        let connection = Arc::new(connection);

        let mut control_stream = connection.open_stream().await.unwrap();

        let connect_msg = TunnelMessage::Connect {
            tunnel_id: tunnel_id.to_string(),
            auth_token: "test-token".to_string(),
            protocols: vec![Protocol::Http {
                subdomain: None, // Auto-generate
            }],
            config: TunnelConfig::default(),
        };

        control_stream.send_message(&connect_msg).await.unwrap();

        let response = timeout(Duration::from_secs(3), control_stream.recv_message())
            .await
            .expect("Timeout")
            .expect("Read error")
            .expect("Empty");

        if let TunnelMessage::Connected { endpoints, .. } = response {
            if let Protocol::Http { subdomain } = &endpoints[0].protocol {
                let subdomain_str = subdomain.as_ref().unwrap().clone();
                info!("Iteration {}: Generated subdomain: {}", i, subdomain_str);
                subdomains.push(subdomain_str);
            }
        }

        // Send disconnect
        control_stream
            .send_message(&TunnelMessage::Disconnect {
                reason: "Test".to_string(),
            })
            .await
            .ok();

        drop(control_stream);
        drop(connection);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Both subdomains should be identical (deterministic)
    assert_eq!(subdomains.len(), 2);
    assert_eq!(
        subdomains[0], subdomains[1],
        "Auto-generated subdomain should be deterministic for same tunnel_id"
    );
    info!(
        "✅ Auto-generated subdomain is deterministic: {}",
        subdomains[0]
    );

    info!("✅ TEST PASSED: Auto-generation is deterministic");
}
