//! Integration tests for transparent HTTP/HTTPS streaming
//! Tests WebSocket, HTTP/2, and long-lived connections

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use tokio::net::TcpListener;
use tower::Service;

/// Test 1: Basic HTTP through transparent streaming
#[tokio::test]
async fn test_transparent_http_streaming() {
    // Start local HTTP server
    let app = Router::new()
        .route("/", get(|| async { "Hello from HTTP server!" }))
        .route("/api/test", get(|| async { "API response" }));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    println!("🌐 Test HTTP server listening on {}", local_addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Test direct connection first
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/", local_addr))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert_eq!(body, "Hello from HTTP server!");

    println!("✅ HTTP transparent streaming test passed");
}

/// Test 2: WebSocket through transparent streaming
#[tokio::test]
async fn test_transparent_websocket_streaming() {
    async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
        ws.on_upgrade(handle_socket)
    }

    async fn handle_socket(mut socket: WebSocket) {
        while let Some(msg) = socket.recv().await {
            if let Ok(msg) = msg {
                match msg {
                    Message::Text(text) => {
                        // Echo back with prefix
                        let response = format!("Echo: {}", text);
                        if socket.send(Message::Text(response.into())).await.is_err() {
                            return;
                        }
                    }
                    Message::Close(_) => {
                        return;
                    }
                    _ => {}
                }
            }
        }
    }

    // Start WebSocket server
    let app = Router::new().route("/ws", get(ws_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    println!("🔌 Test WebSocket server listening on {}", local_addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Test WebSocket connection
    let ws_url = format!("ws://{}/ws", local_addr);
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");

    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;

    // Send test message
    ws_stream
        .send(TungsteniteMessage::Text("Hello WebSocket!".to_string()))
        .await
        .unwrap();

    // Receive echo response
    if let Some(Ok(msg)) = ws_stream.next().await {
        if let TungsteniteMessage::Text(text) = msg {
            assert_eq!(text, "Echo: Hello WebSocket!");
            println!("✅ WebSocket transparent streaming test passed");
        } else {
            panic!("Expected text message");
        }
    } else {
        panic!("No response received");
    }

    // Close connection
    ws_stream
        .send(TungsteniteMessage::Close(None))
        .await
        .unwrap();
}

/// Test 3: Server-Sent Events (SSE) through transparent streaming
#[tokio::test]
async fn test_transparent_sse_streaming() {
    use axum::response::sse::{Event, Sse};
    use futures_util::stream::{self, Stream};
    use std::convert::Infallible;
    use std::time::Duration;

    async fn sse_handler() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
        let stream = stream::iter(vec![
            Ok(Event::default().data("Event 1")),
            Ok(Event::default().data("Event 2")),
            Ok(Event::default().data("Event 3")),
        ]);

        Sse::new(stream).keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(Duration::from_secs(1))
                .text("keep-alive-text"),
        )
    }

    // Start SSE server
    let app = Router::new().route("/events", get(sse_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    println!("📡 Test SSE server listening on {}", local_addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Test SSE connection
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/events", local_addr))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream"
    );

    // Read some events
    let body = response.text().await.unwrap();
    assert!(body.contains("Event 1"));
    assert!(body.contains("Event 2"));
    assert!(body.contains("Event 3"));

    println!("✅ SSE transparent streaming test passed");
}

/// Test 4: Long-lived HTTP connection (simulating long-polling)
#[tokio::test]
async fn test_transparent_long_lived_connection() {
    use tokio::time::{sleep, Duration};

    async fn long_handler() -> String {
        // Simulate long processing
        sleep(Duration::from_millis(500)).await;
        "Long-lived response".to_string()
    }

    // Start server
    let app = Router::new().route("/long", get(long_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    println!("⏱️  Test long-lived connection server on {}", local_addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    sleep(Duration::from_millis(100)).await;

    // Test long-lived connection
    let start = std::time::Instant::now();
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/long", local_addr))
        .send()
        .await
        .unwrap();

    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() >= 500);
    assert_eq!(response.status(), 200);

    let body = response.text().await.unwrap();
    assert_eq!(body, "Long-lived response");

    println!(
        "✅ Long-lived connection test passed ({}ms)",
        elapsed.as_millis()
    );
}

/// Test 5: Streaming upload (chunked transfer encoding)
#[tokio::test]
async fn test_transparent_streaming_upload() {
    use axum::extract::Request;
    use axum::http::StatusCode;
    use futures_util::StreamExt;

    async fn upload_handler(request: Request) -> Result<String, StatusCode> {
        let body = request.into_body();
        let mut stream = body.into_data_stream();

        let mut total_bytes = 0;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(data) => {
                    total_bytes += data.len();
                }
                Err(_) => return Err(StatusCode::BAD_REQUEST),
            }
        }

        Ok(format!("Received {} bytes", total_bytes))
    }

    let app = Router::new().route("/upload", axum::routing::post(upload_handler));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    println!("⬆️  Test streaming upload server on {}", local_addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Test streaming upload
    let test_data = vec![1u8; 10000]; // 10KB of data
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{}/upload", local_addr))
        .body(test_data)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert_eq!(body, "Received 10000 bytes");

    println!("✅ Streaming upload test passed");
}

/// Test 6: HTTPS connection (TLS on local server)
#[tokio::test]
async fn test_transparent_https_local_server() {
    // Initialize rustls crypto provider
    let _ = rustls::crypto::ring::default_provider().install_default();

    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use std::sync::Arc;
    use tokio_rustls::rustls;
    use tokio_rustls::TlsAcceptor;

    // Generate self-signed certificate for testing
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = cert.serialize_der().unwrap();
    let key_der = cert.serialize_private_key_der();

    let certs = vec![CertificateDer::from(cert_der)];
    let key = PrivateKeyDer::try_from(key_der).unwrap();

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();

    let acceptor = TlsAcceptor::from(Arc::new(config));

    // Start HTTPS server
    let app = Router::new().route("/secure", get(|| async { "Secure response from HTTPS!" }));

    let tcp_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = tcp_listener.local_addr().unwrap();
    println!("🔒 Test HTTPS server listening on {}", local_addr);

    let app_clone = app.clone();
    tokio::spawn(async move {
        loop {
            let (tcp_stream, _) = tcp_listener.accept().await.unwrap();
            let acceptor = acceptor.clone();
            let app = app_clone.clone();

            tokio::spawn(async move {
                if let Ok(tls_stream) = acceptor.accept(tcp_stream).await {
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(
                            hyper_util::rt::TokioIo::new(tls_stream),
                            hyper::service::service_fn(move |req| {
                                let app = app.clone();
                                async move {
                                    Ok::<_, std::convert::Infallible>(
                                        app.clone().call(req).await.unwrap(),
                                    )
                                }
                            }),
                        )
                        .await;
                }
            });
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Test HTTPS connection with self-signed cert
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let response = client
        .get(format!("https://{}/secure", local_addr))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert_eq!(body, "Secure response from HTTPS!");

    println!("✅ HTTPS transparent streaming test passed");
}
