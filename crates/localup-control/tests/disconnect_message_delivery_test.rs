//! Integration tests for Disconnect message delivery
//!
//! These tests verify that Disconnect messages are reliably delivered to clients
//! before the connection is closed, even in error scenarios like authentication failure.

use localup_auth::JwtValidator;
use localup_control::{PendingRequests, TunnelConnectionManager, TunnelHandler};
use localup_proto::{Protocol, TunnelConfig, TunnelMessage};
use localup_router::RouteRegistry;
use localup_transport::{
    TransportConnection, TransportConnector, TransportListener, TransportStream,
};
use localup_transport_quic::{QuicConfig, QuicConnector, QuicListener};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tracing::info;

// Initialize rustls crypto provider once at module load
use std::sync::OnceLock;
static CRYPTO_PROVIDER_INIT: OnceLock<()> = OnceLock::new();

fn init_crypto_provider() {
    CRYPTO_PROVIDER_INIT.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

// Helper to create unique server config for each test
fn create_test_server_config(test_name: &str) -> Arc<QuicConfig> {
    use std::env;
    use std::fs;
    use std::io::Write;

    let temp_dir = env::temp_dir().join(format!("localup-disconnect-test-{}", test_name));
    fs::create_dir_all(&temp_dir).unwrap();

    let cert_path = temp_dir.join("cert.pem");
    let key_path = temp_dir.join("key.pem");

    let cert_data = localup_cert::generate_self_signed_cert().unwrap();

    let mut cert_file = fs::File::create(&cert_path).unwrap();
    cert_file.write_all(cert_data.pem_cert.as_bytes()).unwrap();

    let mut key_file = fs::File::create(&key_path).unwrap();
    key_file.write_all(cert_data.pem_key.as_bytes()).unwrap();

    Arc::new(
        QuicConfig::server_default(cert_path.to_str().unwrap(), key_path.to_str().unwrap())
            .unwrap(),
    )
}

/// Create a handler with JWT authentication enabled
fn create_handler_with_jwt(jwt_secret: &str) -> (Arc<TunnelHandler>, Arc<RouteRegistry>) {
    let registry = Arc::new(RouteRegistry::new());
    let connection_manager = Arc::new(TunnelConnectionManager::new());
    let pending_requests = Arc::new(PendingRequests::new());
    let jwt_validator = Arc::new(JwtValidator::new(jwt_secret.as_bytes()));

    let handler = Arc::new(TunnelHandler::new(
        connection_manager,
        registry.clone(),
        Some(jwt_validator),
        "localhost".to_string(),
        pending_requests,
    ));

    (handler, registry)
}

/// Test that client receives Disconnect message on authentication failure
#[tokio::test(flavor = "multi_thread")]
async fn test_disconnect_on_auth_failure() {
    init_crypto_provider();

    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("🧪 TEST: Disconnect message delivery on auth failure");

    // Setup server with JWT authentication
    let (handler, _registry) = create_handler_with_jwt("test-secret-key");

    let server_config = create_test_server_config("auth_failure");
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

    // Connect as client with INVALID token
    let client_config = Arc::new(QuicConfig::client_insecure());
    let connector = QuicConnector::new(client_config).unwrap();
    let connection = connector.connect(server_addr, "localhost").await.unwrap();
    let connection = Arc::new(connection);

    let mut control_stream = connection.open_stream().await.unwrap();

    let connect_msg = TunnelMessage::Connect {
        localup_id: "test-tunnel".to_string(),
        auth_token: "invalid-token-that-will-fail".to_string(), // Invalid JWT
        protocols: vec![Protocol::Http {
            subdomain: Some("myapp".to_string()),
            custom_domain: None,
        }],
        config: TunnelConfig::default(),
    };

    control_stream.send_message(&connect_msg).await.unwrap();

    // Wait for response - should get Disconnect, not just connection closed
    let response = timeout(Duration::from_secs(5), control_stream.recv_message())
        .await
        .expect("Timeout waiting for response - client should receive Disconnect before close")
        .expect("Failed to read message");

    match response {
        Some(TunnelMessage::Disconnect { reason }) => {
            info!("✅ Received Disconnect message: {}", reason);
            assert!(
                reason.contains("Authentication failed") || reason.contains("JWT"),
                "Disconnect reason should mention authentication failure, got: {}",
                reason
            );
        }
        Some(other) => panic!("Expected Disconnect message, got: {:?}", other),
        None => panic!("Expected Disconnect message, but stream closed without message"),
    }

    info!("✅ TEST PASSED: Client receives Disconnect on auth failure");
}

/// Test that a SLOW client still receives the Disconnect message
/// This simulates network latency or a slow client that takes time to read
#[tokio::test(flavor = "multi_thread")]
async fn test_disconnect_delivery_to_slow_client() {
    init_crypto_provider();

    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("🧪 TEST: Disconnect delivery to slow client");

    // Setup server with JWT authentication
    let (handler, _registry) = create_handler_with_jwt("test-secret-key");

    let server_config = create_test_server_config("slow_client");
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

    // Connect as client
    let client_config = Arc::new(QuicConfig::client_insecure());
    let connector = QuicConnector::new(client_config).unwrap();
    let connection = connector.connect(server_addr, "localhost").await.unwrap();
    let connection = Arc::new(connection);

    let mut control_stream = connection.open_stream().await.unwrap();

    let connect_msg = TunnelMessage::Connect {
        localup_id: "slow-client-tunnel".to_string(),
        auth_token: "invalid-token".to_string(),
        protocols: vec![Protocol::Http {
            subdomain: Some("slowapp".to_string()),
            custom_domain: None,
        }],
        config: TunnelConfig::default(),
    };

    control_stream.send_message(&connect_msg).await.unwrap();

    // Simulate slow client - wait before reading response
    // The server should have already sent Disconnect and called finish()
    // QUIC should buffer this and deliver when we finally read
    info!("⏳ Simulating slow client - waiting 500ms before reading...");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Now read - should still get the Disconnect message
    let response = timeout(Duration::from_secs(5), control_stream.recv_message())
        .await
        .expect("Timeout - slow client should still receive buffered Disconnect")
        .expect("Failed to read message");

    match response {
        Some(TunnelMessage::Disconnect { reason }) => {
            info!("✅ Slow client received Disconnect: {}", reason);
            assert!(
                reason.contains("Authentication failed") || reason.contains("JWT"),
                "Disconnect reason should mention auth failure: {}",
                reason
            );
        }
        Some(other) => panic!("Expected Disconnect, got: {:?}", other),
        None => panic!("Expected Disconnect, but got None (connection closed without message)"),
    }

    info!("✅ TEST PASSED: Slow client receives Disconnect message");
}

/// Test that client receives Disconnect on invalid first message
#[tokio::test(flavor = "multi_thread")]
async fn test_disconnect_on_invalid_first_message() {
    init_crypto_provider();

    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("🧪 TEST: Disconnect on invalid first message");

    // Setup server (no JWT required for this test)
    let registry = Arc::new(RouteRegistry::new());
    let connection_manager = Arc::new(TunnelConnectionManager::new());
    let pending_requests = Arc::new(PendingRequests::new());

    let handler = Arc::new(TunnelHandler::new(
        connection_manager,
        registry,
        None,
        "localhost".to_string(),
        pending_requests,
    ));

    let server_config = create_test_server_config("invalid_first_message");
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

    // Connect and send an invalid first message (Ping instead of Connect)
    let client_config = Arc::new(QuicConfig::client_insecure());
    let connector = QuicConnector::new(client_config).unwrap();
    let connection = connector.connect(server_addr, "localhost").await.unwrap();
    let connection = Arc::new(connection);

    let mut control_stream = connection.open_stream().await.unwrap();

    // Send Ping as first message (invalid - should be Connect or AgentAuth)
    let invalid_msg = TunnelMessage::Ping { timestamp: 12345 };
    control_stream.send_message(&invalid_msg).await.unwrap();

    // Should receive Disconnect with "Invalid first message"
    let response = timeout(Duration::from_secs(5), control_stream.recv_message())
        .await
        .expect("Timeout waiting for Disconnect")
        .expect("Failed to read message");

    match response {
        Some(TunnelMessage::Disconnect { reason }) => {
            info!("✅ Received Disconnect: {}", reason);
            assert!(
                reason.contains("Invalid first message"),
                "Disconnect reason should mention invalid first message: {}",
                reason
            );
        }
        Some(other) => panic!("Expected Disconnect, got: {:?}", other),
        None => panic!("Expected Disconnect, but stream closed without message"),
    }

    info!("✅ TEST PASSED: Client receives Disconnect on invalid first message");
}

/// Test with moderately slow client (200ms delay) - realistic network latency
#[tokio::test(flavor = "multi_thread")]
async fn test_disconnect_delivery_moderate_delay_client() {
    init_crypto_provider();

    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("🧪 TEST: Disconnect delivery with moderate delay (200ms)");

    let (handler, _registry) = create_handler_with_jwt("test-secret");

    let server_config = create_test_server_config("moderate_delay_client");
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

    let client_config = Arc::new(QuicConfig::client_insecure());
    let connector = QuicConnector::new(client_config).unwrap();
    let connection = connector.connect(server_addr, "localhost").await.unwrap();
    let connection = Arc::new(connection);

    let mut control_stream = connection.open_stream().await.unwrap();

    let connect_msg = TunnelMessage::Connect {
        localup_id: "moderate-delay-tunnel".to_string(),
        auth_token: "bad-token".to_string(),
        protocols: vec![Protocol::Tcp { port: 0 }], // 0 means auto-allocate
        config: TunnelConfig::default(),
    };

    control_stream.send_message(&connect_msg).await.unwrap();

    // Moderate delay - simulates network latency or busy client
    info!("⏳ Moderate delay client - waiting 200ms before reading...");
    tokio::time::sleep(Duration::from_millis(200)).await;

    let response = timeout(Duration::from_secs(5), control_stream.recv_message())
        .await
        .expect("Timeout - client should still receive Disconnect after 200ms")
        .expect("Failed to read");

    match response {
        Some(TunnelMessage::Disconnect { reason }) => {
            info!("✅ Client received Disconnect after delay: {}", reason);
            assert!(reason.contains("Authentication failed") || reason.contains("JWT"));
        }
        Some(other) => panic!("Expected Disconnect, got: {:?}", other),
        None => panic!("Connection closed without Disconnect message"),
    }

    info!("✅ TEST PASSED: Moderate delay client receives Disconnect");
}

/// Test multiple concurrent clients with auth failures
#[tokio::test(flavor = "multi_thread")]
async fn test_disconnect_delivery_concurrent_clients() {
    init_crypto_provider();

    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("🧪 TEST: Disconnect delivery to multiple concurrent clients");

    let (handler, _registry) = create_handler_with_jwt("test-secret");

    let server_config = create_test_server_config("concurrent_clients");
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

    // Spawn multiple clients concurrently
    let num_clients = 5;
    let mut handles = Vec::new();

    for i in 0..num_clients {
        let server_addr = server_addr;
        let handle = tokio::spawn(async move {
            let client_config = Arc::new(QuicConfig::client_insecure());
            let connector = QuicConnector::new(client_config).unwrap();
            let connection = connector.connect(server_addr, "localhost").await.unwrap();
            let connection = Arc::new(connection);

            let mut control_stream = connection.open_stream().await.unwrap();

            let connect_msg = TunnelMessage::Connect {
                localup_id: format!("concurrent-client-{}", i),
                auth_token: format!("bad-token-{}", i),
                protocols: vec![Protocol::Http {
                    subdomain: Some(format!("app{}", i)),
                    custom_domain: None,
                }],
                config: TunnelConfig::default(),
            };

            control_stream.send_message(&connect_msg).await.unwrap();

            // Small varying delays (10-50ms) to simulate slight timing differences
            tokio::time::sleep(Duration::from_millis(10 * (i as u64 + 1))).await;

            let response = timeout(Duration::from_secs(5), control_stream.recv_message())
                .await
                .expect("Timeout")
                .expect("Read error");

            match response {
                Some(TunnelMessage::Disconnect { reason }) => {
                    assert!(reason.contains("Authentication failed") || reason.contains("JWT"));
                    Ok(i)
                }
                other => Err(format!("Client {} got unexpected: {:?}", i, other)),
            }
        });

        handles.push(handle);
    }

    // Wait for all clients and verify all received Disconnect
    let mut successes = 0;
    for handle in handles {
        match handle.await {
            Ok(Ok(client_id)) => {
                info!("✅ Client {} received Disconnect", client_id);
                successes += 1;
            }
            Ok(Err(e)) => panic!("Client failed: {}", e),
            Err(e) => panic!("Task panicked: {}", e),
        }
    }

    assert_eq!(
        successes, num_clients,
        "All {} clients should receive Disconnect",
        num_clients
    );

    info!(
        "✅ TEST PASSED: All {} concurrent clients received Disconnect",
        num_clients
    );
}
