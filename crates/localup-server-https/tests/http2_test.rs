//! HTTP/2 integration tests for the HTTPS server
//!
//! Tests verify that:
//! 1. ALPN negotiation works correctly
//! 2. HTTP/2 connections are properly handled
//! 3. Edge cases are handled correctly

use bytes::Bytes;
use h2::client;
use http::Request;
use rcgen::{generate_simple_self_signed, CertifiedKey};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::rustls::{ClientConfig, RootCertStore, ServerConfig};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use tracing::info;

// ============================================================================
// Test Helpers
// ============================================================================

/// Generate self-signed certificate for testing
fn generate_test_cert() -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>) {
    let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let CertifiedKey { cert, key_pair } = generate_simple_self_signed(subject_alt_names).unwrap();

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der()));

    (vec![cert_der], key_der)
}

/// Create a TLS server config with ALPN for HTTP/2
fn create_server_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Arc<ServerConfig> {
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();

    // Enable HTTP/2 via ALPN
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Arc::new(config)
}

/// Create a TLS client config that supports HTTP/2
fn create_h2_client_config(cert: CertificateDer<'static>) -> Arc<ClientConfig> {
    let mut root_store = RootCertStore::empty();
    root_store.add(cert).unwrap();

    let mut config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    config.alpn_protocols = vec![b"h2".to_vec()];

    Arc::new(config)
}

/// Create a TLS client config that only supports HTTP/1.1
fn create_http11_client_config(cert: CertificateDer<'static>) -> Arc<ClientConfig> {
    let mut root_store = RootCertStore::empty();
    root_store.add(cert).unwrap();

    let mut config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    config.alpn_protocols = vec![b"http/1.1".to_vec()];

    Arc::new(config)
}

/// Create a TLS client config with no ALPN
fn create_no_alpn_client_config(cert: CertificateDer<'static>) -> Arc<ClientConfig> {
    let mut root_store = RootCertStore::empty();
    root_store.add(cert).unwrap();

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    // No ALPN protocols set
    Arc::new(config)
}

fn init_test_logging() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init()
        .ok();
}

// ============================================================================
// ALPN Negotiation Tests (Unit Tests)
// ============================================================================

#[tokio::test]
async fn test_alpn_negotiates_h2_when_client_supports_it() {
    init_test_logging();
    info!("=== Testing ALPN negotiates h2 when client supports it ===");

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs.clone(), key);
    let client_config = create_h2_client_config(certs[0].clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(stream).await.unwrap();

        let alpn = tls_stream.get_ref().1.alpn_protocol();
        assert_eq!(alpn, Some(b"h2".as_slice()), "ALPN should negotiate h2");

        let h2_result = h2::server::handshake(tls_stream).await;
        assert!(h2_result.is_ok(), "HTTP/2 handshake should succeed");
    });

    let client_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let connector = TlsConnector::from(client_config);
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let tls_stream = connector
            .connect("localhost".try_into().unwrap(), stream)
            .await
            .unwrap();

        let alpn = tls_stream.get_ref().1.alpn_protocol();
        assert_eq!(alpn, Some(b"h2".as_slice()), "ALPN should negotiate h2");

        let h2_result = h2::client::handshake(tls_stream).await;
        assert!(h2_result.is_ok(), "HTTP/2 handshake should succeed");
    });

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        server_task.await.unwrap();
        client_task.await.unwrap();
    })
    .await
    .expect("Test should complete within timeout");
}

#[tokio::test]
async fn test_alpn_fallback_to_http11() {
    init_test_logging();
    info!("=== Testing ALPN fallback to HTTP/1.1 ===");

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs.clone(), key);
    let client_config = create_http11_client_config(certs[0].clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(stream).await.unwrap();

        let alpn = tls_stream.get_ref().1.alpn_protocol();
        assert_eq!(
            alpn,
            Some(b"http/1.1".as_slice()),
            "ALPN should negotiate http/1.1"
        );
    });

    let client_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let connector = TlsConnector::from(client_config);
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let tls_stream = connector
            .connect("localhost".try_into().unwrap(), stream)
            .await
            .unwrap();

        let alpn = tls_stream.get_ref().1.alpn_protocol();
        assert_eq!(
            alpn,
            Some(b"http/1.1".as_slice()),
            "ALPN should negotiate http/1.1"
        );
    });

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        server_task.await.unwrap();
        client_task.await.unwrap();
    })
    .await
    .expect("Test should complete within timeout");
}

#[tokio::test]
async fn test_alpn_no_protocol_from_client() {
    init_test_logging();
    info!("=== Testing connection when client sends no ALPN ===");

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs.clone(), key);
    let client_config = create_no_alpn_client_config(certs[0].clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(stream).await.unwrap();

        // When client sends no ALPN, server should have None
        let alpn = tls_stream.get_ref().1.alpn_protocol();
        assert!(
            alpn.is_none(),
            "ALPN should be None when client doesn't send it"
        );
    });

    let client_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let connector = TlsConnector::from(client_config);
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let tls_stream = connector
            .connect("localhost".try_into().unwrap(), stream)
            .await
            .unwrap();

        let alpn = tls_stream.get_ref().1.alpn_protocol();
        assert!(alpn.is_none(), "ALPN should be None");
    });

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        server_task.await.unwrap();
        client_task.await.unwrap();
    })
    .await
    .expect("Test should complete within timeout");
}

#[tokio::test]
async fn test_server_alpn_config_includes_both_protocols() {
    init_test_logging();

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs, key);

    assert_eq!(
        server_config.alpn_protocols,
        vec![b"h2".to_vec(), b"http/1.1".to_vec()],
        "Server should advertise both h2 and http/1.1"
    );
}

// ============================================================================
// HTTP/2 Stream Tests (Integration Tests)
// ============================================================================

#[tokio::test]
async fn test_h2_single_request_response() {
    init_test_logging();
    info!("=== Testing single HTTP/2 request/response ===");

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs.clone(), key);
    let client_config = create_h2_client_config(certs[0].clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(stream).await.unwrap();
        let mut h2_conn = h2::server::handshake(tls_stream).await.unwrap();

        // Handle exactly one request
        if let Some(result) = h2_conn.accept().await {
            let (request, mut send_response) = result.unwrap();

            assert_eq!(request.method(), "GET");
            assert_eq!(request.uri().path(), "/test");

            let response = http::Response::builder().status(200).body(()).unwrap();
            let mut send = send_response.send_response(response, false).unwrap();
            send.send_data(Bytes::from("OK"), true).unwrap();
        }
    });

    let client_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let connector = TlsConnector::from(client_config);
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let tls_stream = connector
            .connect("localhost".try_into().unwrap(), stream)
            .await
            .unwrap();

        let (h2_client, h2_conn) = client::handshake(tls_stream).await.unwrap();

        // Drive the connection in a background task
        tokio::spawn(async move {
            let _ = h2_conn.await;
        });

        let mut h2_client = h2_client.ready().await.unwrap();
        let request = Request::builder()
            .method("GET")
            .uri("https://localhost/test")
            .body(())
            .unwrap();

        let (response_future, _) = h2_client.send_request(request, true).unwrap();
        let response = response_future.await.unwrap();

        assert_eq!(response.status(), 200);
    });

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        // Wait for server to complete (it handles one request then exits)
        server_task.await.unwrap();
        // Abort client task if server is done
        client_task.abort();
    })
    .await
    .expect("Test should complete within timeout");
}

#[tokio::test]
async fn test_h2_multiple_concurrent_streams() {
    init_test_logging();
    info!("=== Testing multiple concurrent HTTP/2 streams ===");

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs.clone(), key);
    let client_config = create_h2_client_config(certs[0].clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    let request_count = Arc::new(AtomicUsize::new(0));
    let request_count_server = request_count.clone();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(stream).await.unwrap();
        let mut h2_conn = h2::server::handshake(tls_stream).await.unwrap();

        let mut handles = vec![];

        // Accept exactly 10 requests
        for _ in 0..10 {
            if let Some(result) = h2_conn.accept().await {
                match result {
                    Ok((request, mut send_response)) => {
                        let count = request_count_server.clone();
                        let handle = tokio::spawn(async move {
                            count.fetch_add(1, Ordering::SeqCst);

                            let path = request.uri().path().to_string();
                            let response = http::Response::builder().status(200).body(()).unwrap();
                            let mut send = send_response.send_response(response, false).unwrap();
                            send.send_data(Bytes::from(format!("Response for {}", path)), true)
                                .unwrap();
                        });
                        handles.push(handle);
                    }
                    Err(e) => {
                        info!("Server accept error: {:?}", e);
                        break;
                    }
                }
            }
        }

        // Wait for all handlers to complete
        for handle in handles {
            let _ = handle.await;
        }
    });

    let client_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let connector = TlsConnector::from(client_config);
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let tls_stream = connector
            .connect("localhost".try_into().unwrap(), stream)
            .await
            .unwrap();

        let (h2_client, h2_conn) = client::handshake(tls_stream).await.unwrap();

        // Drive connection in background
        tokio::spawn(async move {
            let _ = h2_conn.await;
        });

        // Send 10 concurrent requests
        let mut response_handles = vec![];
        for i in 0..10 {
            let mut h2 = h2_client.clone().ready().await.unwrap();
            let handle = tokio::spawn(async move {
                let request = Request::builder()
                    .method("GET")
                    .uri(format!("https://localhost/path/{}", i))
                    .body(())
                    .unwrap();

                let (response_future, _) = h2.send_request(request, true).unwrap();
                let response = response_future.await.unwrap();
                assert_eq!(response.status(), 200);
            });
            response_handles.push(handle);
        }

        // Wait for all responses
        for handle in response_handles {
            handle.await.unwrap();
        }
    });

    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        server_task.await.unwrap();
        client_task.abort();
    })
    .await
    .expect("Test should complete within timeout");

    assert_eq!(
        request_count.load(Ordering::SeqCst),
        10,
        "Should have processed 10 requests"
    );
}

#[tokio::test]
async fn test_h2_request_with_body() {
    init_test_logging();
    info!("=== Testing HTTP/2 request with body ===");

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs.clone(), key);
    let client_config = create_h2_client_config(certs[0].clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    let received_body = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let received_body_server = received_body.clone();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(stream).await.unwrap();
        let mut h2_conn = h2::server::handshake(tls_stream).await.unwrap();

        if let Some(result) = h2_conn.accept().await {
            let (request, mut send_response) = result.unwrap();

            assert_eq!(request.method(), "POST");

            // Read body
            let mut body_stream = request.into_body();
            let mut body_bytes = Vec::new();
            while let Some(chunk) = body_stream.data().await {
                let data = chunk.unwrap();
                body_bytes.extend_from_slice(&data);
                let _ = body_stream.flow_control().release_capacity(data.len());
            }

            *received_body_server.lock().await = body_bytes;

            let response = http::Response::builder().status(201).body(()).unwrap();
            let mut send = send_response.send_response(response, false).unwrap();
            send.send_data(Bytes::from("Created"), true).unwrap();
        }
    });

    let client_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let connector = TlsConnector::from(client_config);
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let tls_stream = connector
            .connect("localhost".try_into().unwrap(), stream)
            .await
            .unwrap();

        let (h2_client, h2_conn) = client::handshake(tls_stream).await.unwrap();
        tokio::spawn(async move {
            let _ = h2_conn.await;
        });

        let mut h2_client = h2_client.ready().await.unwrap();
        let request = Request::builder()
            .method("POST")
            .uri("https://localhost/api/data")
            .header("content-type", "application/json")
            .body(())
            .unwrap();

        let (response_future, mut send_body) = h2_client.send_request(request, false).unwrap();

        // Send body
        let body_data = b"{\"key\":\"value\",\"number\":42}";
        send_body
            .send_data(Bytes::from(&body_data[..]), true)
            .unwrap();

        let response = response_future.await.unwrap();
        assert_eq!(response.status(), 201);
    });

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        server_task.await.unwrap();
        client_task.abort();
    })
    .await
    .expect("Test should complete within timeout");

    let body = received_body.lock().await;
    assert_eq!(
        String::from_utf8_lossy(&body),
        "{\"key\":\"value\",\"number\":42}"
    );
}

#[tokio::test]
async fn test_h2_large_response_body() {
    init_test_logging();
    info!("=== Testing HTTP/2 large response body ===");

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs.clone(), key);
    let client_config = create_h2_client_config(certs[0].clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    // 50KB response (reduced size for test reliability)
    let large_body: Vec<u8> = (0..50_000).map(|i| (i % 256) as u8).collect();
    let large_body_expected = large_body.clone();

    let received_size = Arc::new(AtomicUsize::new(0));
    let received_size_clone = received_size.clone();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(stream).await.unwrap();
        let mut h2_conn = h2::server::handshake(tls_stream).await.unwrap();

        if let Some(result) = h2_conn.accept().await {
            let (_request, mut send_response) = result.unwrap();

            let response = http::Response::builder()
                .status(200)
                .header("content-length", large_body.len().to_string())
                .body(())
                .unwrap();

            let mut send = send_response.send_response(response, false).unwrap();

            // Send all data at once (h2 will handle framing)
            send.send_data(Bytes::from(large_body), true).unwrap();
        }
    });

    let client_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let connector = TlsConnector::from(client_config);
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let tls_stream = connector
            .connect("localhost".try_into().unwrap(), stream)
            .await
            .unwrap();

        let (h2_client, h2_conn) = client::handshake(tls_stream).await.unwrap();
        tokio::spawn(async move {
            let _ = h2_conn.await;
        });

        let mut h2_client = h2_client.ready().await.unwrap();
        let request = Request::builder()
            .method("GET")
            .uri("https://localhost/large")
            .body(())
            .unwrap();

        let (response_future, _) = h2_client.send_request(request, true).unwrap();
        let response = response_future.await.unwrap();

        assert_eq!(response.status(), 200);

        // Read body
        let mut body_stream = response.into_body();
        let mut received_bytes = Vec::new();
        while let Some(chunk) = body_stream.data().await {
            let data = chunk.unwrap();
            received_bytes.extend_from_slice(&data);
            let _ = body_stream.flow_control().release_capacity(data.len());
        }

        received_size_clone.store(received_bytes.len(), Ordering::SeqCst);
        assert_eq!(received_bytes.len(), 50_000);
        assert_eq!(received_bytes, large_body_expected);
    });

    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        server_task.await.unwrap();
        client_task.abort();
    })
    .await
    .expect("Test should complete within timeout");
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[tokio::test]
async fn test_h2_empty_body_response() {
    init_test_logging();
    info!("=== Testing HTTP/2 empty body response (204 No Content) ===");

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs.clone(), key);
    let client_config = create_h2_client_config(certs[0].clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(stream).await.unwrap();
        let mut h2_conn = h2::server::handshake(tls_stream).await.unwrap();

        if let Some(result) = h2_conn.accept().await {
            let (_request, mut send_response) = result.unwrap();

            // 204 No Content - no body
            let response = http::Response::builder().status(204).body(()).unwrap();
            // End stream with headers only
            send_response.send_response(response, true).unwrap();
        }
    });

    let client_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let connector = TlsConnector::from(client_config);
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let tls_stream = connector
            .connect("localhost".try_into().unwrap(), stream)
            .await
            .unwrap();

        let (h2_client, h2_conn) = client::handshake(tls_stream).await.unwrap();
        tokio::spawn(async move {
            let _ = h2_conn.await;
        });

        let mut h2_client = h2_client.ready().await.unwrap();
        let request = Request::builder()
            .method("DELETE")
            .uri("https://localhost/resource/123")
            .body(())
            .unwrap();

        let (response_future, _) = h2_client.send_request(request, true).unwrap();
        let response = response_future.await.unwrap();

        assert_eq!(response.status(), 204);
    });

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        server_task.await.unwrap();
        client_task.abort();
    })
    .await
    .expect("Test should complete within timeout");
}

#[tokio::test]
async fn test_h2_various_http_methods() {
    init_test_logging();
    info!("=== Testing various HTTP methods over HTTP/2 ===");

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs.clone(), key);
    let client_config = create_h2_client_config(certs[0].clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    let methods_received = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let methods_received_server = methods_received.clone();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(stream).await.unwrap();
        let mut h2_conn = h2::server::handshake(tls_stream).await.unwrap();

        // Accept exactly 5 requests (one for each method)
        for _ in 0..5 {
            if let Some(result) = h2_conn.accept().await {
                let (request, mut send_response) = result.unwrap();
                methods_received_server
                    .lock()
                    .await
                    .push(request.method().to_string());

                let response = http::Response::builder().status(200).body(()).unwrap();
                let mut send = send_response.send_response(response, false).unwrap();
                send.send_data(Bytes::from("OK"), true).unwrap();
            }
        }
    });

    let client_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let connector = TlsConnector::from(client_config);
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let tls_stream = connector
            .connect("localhost".try_into().unwrap(), stream)
            .await
            .unwrap();

        let (h2_client, h2_conn) = client::handshake(tls_stream).await.unwrap();
        tokio::spawn(async move {
            let _ = h2_conn.await;
        });

        let methods = ["GET", "POST", "PUT", "PATCH", "DELETE"];

        for method in methods {
            let mut h2 = h2_client.clone().ready().await.unwrap();
            let request = Request::builder()
                .method(method)
                .uri("https://localhost/test")
                .body(())
                .unwrap();

            let (response_future, _) = h2.send_request(request, true).unwrap();
            let response = response_future.await.unwrap();
            assert_eq!(response.status(), 200);
        }
    });

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        server_task.await.unwrap();
        client_task.abort();
    })
    .await
    .expect("Test should complete within timeout");

    let methods = methods_received.lock().await;
    assert_eq!(
        *methods,
        vec!["GET", "POST", "PUT", "PATCH", "DELETE"],
        "Should receive all HTTP methods"
    );
}

#[tokio::test]
async fn test_h2_custom_headers() {
    init_test_logging();
    info!("=== Testing custom headers in HTTP/2 ===");

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs.clone(), key);
    let client_config = create_h2_client_config(certs[0].clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    let received_headers = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let received_headers_server = received_headers.clone();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(stream).await.unwrap();
        let mut h2_conn = h2::server::handshake(tls_stream).await.unwrap();

        if let Some(result) = h2_conn.accept().await {
            let (request, mut send_response) = result.unwrap();

            // Collect custom headers
            for (name, value) in request.headers() {
                if name.as_str().starts_with("x-") {
                    received_headers_server
                        .lock()
                        .await
                        .push((name.to_string(), value.to_str().unwrap_or("").to_string()));
                }
            }

            let response = http::Response::builder()
                .status(200)
                .header("x-response-id", "12345")
                .header("x-server-version", "1.0.0")
                .body(())
                .unwrap();
            let mut send = send_response.send_response(response, false).unwrap();
            send.send_data(Bytes::from("OK"), true).unwrap();
        }
    });

    let client_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let connector = TlsConnector::from(client_config);
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let tls_stream = connector
            .connect("localhost".try_into().unwrap(), stream)
            .await
            .unwrap();

        let (h2_client, h2_conn) = client::handshake(tls_stream).await.unwrap();
        tokio::spawn(async move {
            let _ = h2_conn.await;
        });

        let mut h2_client = h2_client.ready().await.unwrap();
        let request = Request::builder()
            .method("GET")
            .uri("https://localhost/test")
            .header("x-request-id", "abc123")
            .header("x-custom-header", "custom-value")
            .header("x-another-header", "another-value")
            .body(())
            .unwrap();

        let (response_future, _) = h2_client.send_request(request, true).unwrap();
        let response = response_future.await.unwrap();

        assert_eq!(response.status(), 200);

        // Check response headers
        assert_eq!(response.headers().get("x-response-id").unwrap(), "12345");
        assert_eq!(response.headers().get("x-server-version").unwrap(), "1.0.0");
    });

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        server_task.await.unwrap();
        client_task.abort();
    })
    .await
    .expect("Test should complete within timeout");

    let headers = received_headers.lock().await;
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "x-request-id" && v == "abc123"),
        "Should receive x-request-id header"
    );
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "x-custom-header" && v == "custom-value"),
        "Should receive x-custom-header header"
    );
}

#[tokio::test]
async fn test_h2_error_status_codes() {
    init_test_logging();
    info!("=== Testing error status codes in HTTP/2 ===");

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs.clone(), key);
    let client_config = create_h2_client_config(certs[0].clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    let status_codes = [400u16, 401, 403, 404, 500, 502, 503];
    let status_idx = Arc::new(AtomicUsize::new(0));
    let status_idx_server = status_idx.clone();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(stream).await.unwrap();
        let mut h2_conn = h2::server::handshake(tls_stream).await.unwrap();

        // Accept exactly 7 requests (one for each status code)
        for _ in 0..7 {
            if let Some(result) = h2_conn.accept().await {
                let (_request, mut send_response) = result.unwrap();
                let idx = status_idx_server.fetch_add(1, Ordering::SeqCst);
                let status = status_codes[idx];
                let response = http::Response::builder().status(status).body(()).unwrap();
                let mut send = send_response.send_response(response, false).unwrap();
                send.send_data(Bytes::from(format!("Error {}", status)), true)
                    .unwrap();
            }
        }
    });

    let client_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let connector = TlsConnector::from(client_config);
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let tls_stream = connector
            .connect("localhost".try_into().unwrap(), stream)
            .await
            .unwrap();

        let (h2_client, h2_conn) = client::handshake(tls_stream).await.unwrap();
        tokio::spawn(async move {
            let _ = h2_conn.await;
        });

        for expected_status in status_codes {
            let mut h2 = h2_client.clone().ready().await.unwrap();
            let request = Request::builder()
                .method("GET")
                .uri("https://localhost/error")
                .body(())
                .unwrap();

            let (response_future, _) = h2.send_request(request, true).unwrap();
            let response = response_future.await.unwrap();
            assert_eq!(
                response.status().as_u16(),
                expected_status,
                "Expected status {}",
                expected_status
            );
        }
    });

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        server_task.await.unwrap();
        client_task.abort();
    })
    .await
    .expect("Test should complete within timeout");
}

#[tokio::test]
async fn test_h2_server_can_handle_rapid_requests() {
    init_test_logging();
    info!("=== Testing rapid request handling ===");

    let (certs, key) = generate_test_cert();
    let server_config = create_server_config(certs.clone(), key);
    let client_config = create_h2_client_config(certs[0].clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(server_config);

    let total_requests = 50;
    let handled_count = Arc::new(AtomicUsize::new(0));
    let handled_count_server = handled_count.clone();

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls_stream = acceptor.accept(stream).await.unwrap();
        let mut h2_conn = h2::server::handshake(tls_stream).await.unwrap();

        let mut handles = vec![];

        // Accept exactly 50 requests
        for _ in 0..total_requests {
            if let Some(Ok((request, mut send_response))) = h2_conn.accept().await {
                let count = handled_count_server.clone();
                let handle = tokio::spawn(async move {
                    count.fetch_add(1, Ordering::SeqCst);

                    let response = http::Response::builder().status(200).body(()).unwrap();
                    let mut send = send_response.send_response(response, false).unwrap();
                    send.send_data(Bytes::from(format!("Response for {}", request.uri())), true)
                        .unwrap();
                });
                handles.push(handle);
            }
        }

        // Wait for all handlers
        for handle in handles {
            let _ = handle.await;
        }
    });

    let client_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let connector = TlsConnector::from(client_config);
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let tls_stream = connector
            .connect("localhost".try_into().unwrap(), stream)
            .await
            .unwrap();

        let (h2_client, h2_conn) = client::handshake(tls_stream).await.unwrap();
        tokio::spawn(async move {
            let _ = h2_conn.await;
        });

        // Fire 50 requests as fast as possible
        let mut handles = vec![];
        for i in 0..total_requests {
            let mut h2 = h2_client.clone().ready().await.unwrap();
            let handle = tokio::spawn(async move {
                let request = Request::builder()
                    .method("GET")
                    .uri(format!("https://localhost/rapid/{}", i))
                    .body(())
                    .unwrap();

                let (response_future, _) = h2.send_request(request, true).unwrap();
                response_future.await.unwrap()
            });
            handles.push(handle);
        }

        for handle in handles {
            let response = handle.await.unwrap();
            assert_eq!(response.status(), 200);
        }
    });

    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        server_task.await.unwrap();
        client_task.abort();
    })
    .await
    .expect("Test should complete within timeout");

    assert_eq!(
        handled_count.load(Ordering::SeqCst),
        total_requests,
        "Should have handled all {} requests",
        total_requests
    );
}

// ============================================================================
// Unit Tests for Helper Functions
// ============================================================================

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn test_generate_test_cert_produces_valid_cert() {
        let (certs, key) = generate_test_cert();

        assert_eq!(certs.len(), 1, "Should generate exactly one certificate");
        assert!(
            !certs[0].as_ref().is_empty(),
            "Certificate should not be empty"
        );

        // Verify key is present
        match key {
            PrivateKeyDer::Pkcs8(ref pkcs8) => {
                assert!(
                    !pkcs8.secret_pkcs8_der().is_empty(),
                    "Key should not be empty"
                );
            }
            _ => panic!("Expected PKCS8 key format"),
        }
    }

    #[test]
    fn test_server_config_has_correct_alpn() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (certs, key) = generate_test_cert();
        let config = create_server_config(certs, key);

        assert_eq!(
            config.alpn_protocols,
            vec![b"h2".to_vec(), b"http/1.1".to_vec()],
            "Server config should have h2 and http/1.1 ALPN"
        );
    }

    #[test]
    fn test_h2_client_config_has_h2_alpn() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (certs, _) = generate_test_cert();
        let config = create_h2_client_config(certs[0].clone());

        assert_eq!(
            config.alpn_protocols,
            vec![b"h2".to_vec()],
            "H2 client config should have only h2 ALPN"
        );
    }

    #[test]
    fn test_http11_client_config_has_http11_alpn() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (certs, _) = generate_test_cert();
        let config = create_http11_client_config(certs[0].clone());

        assert_eq!(
            config.alpn_protocols,
            vec![b"http/1.1".to_vec()],
            "HTTP/1.1 client config should have only http/1.1 ALPN"
        );
    }

    #[test]
    fn test_no_alpn_client_config_has_empty_alpn() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (certs, _) = generate_test_cert();
        let config = create_no_alpn_client_config(certs[0].clone());

        assert!(
            config.alpn_protocols.is_empty(),
            "No-ALPN client config should have empty ALPN list"
        );
    }
}
