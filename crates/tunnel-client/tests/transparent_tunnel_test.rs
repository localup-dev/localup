//! End-to-end test for transparent streaming through the tunnel
//! Tests that HttpStreamConnect/HttpStreamData/HttpStreamClose work correctly

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing::info;
use tunnel_proto::TunnelMessage;
use tunnel_transport::{
    TransportConnection, TransportConnector, TransportListener, TransportStream,
};
use tunnel_transport_quic::{QuicConfig, QuicConnector, QuicListener};

/// Test HTTP transparent streaming through tunnel
#[tokio::test(flavor = "multi_thread")]
async fn test_http_transparent_streaming_through_tunnel() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init()
        .ok();

    info!("=== Testing HTTP Transparent Streaming Through Tunnel ===");

    // 1. Start local HTTP server
    let app = Router::new().route("/test", get(|| async { "Hello from local server!" }));

    let local_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = local_listener.local_addr().unwrap();
    info!("Local HTTP server started on {}", local_addr);

    tokio::spawn(async move {
        axum::serve(local_listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // 2. Start QUIC tunnel relay (simulating exit node)
    let server_config = Arc::new(QuicConfig::server_self_signed().unwrap());
    let relay_listener = QuicListener::new("127.0.0.1:0".parse().unwrap(), server_config).unwrap();
    let relay_addr = relay_listener.local_addr().unwrap();
    info!("Tunnel relay started on {}", relay_addr);

    // 3. Simulate exit node sending HttpStreamConnect
    let relay_task: tokio::task::JoinHandle<Result<usize, String>> = tokio::spawn(async move {
        info!("Relay: waiting for tunnel client connection...");
        let (connection, peer_addr) = relay_listener.accept().await.unwrap();
        info!("Relay: accepted tunnel client from {}", peer_addr);

        // Wait for control stream setup
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Open a stream for HTTP request (simulating external HTTP request)
        let mut stream = connection.open_stream().await.unwrap();
        info!("Relay: opened stream for HTTP request");

        // Send HttpStreamConnect with raw HTTP request
        let http_request = "GET /test HTTP/1.1\r\n\
             Host: example.com\r\n\
             Connection: close\r\n\
             \r\n"
            .to_string();

        let connect_msg = TunnelMessage::HttpStreamConnect {
            stream_id: 42,
            host: "example.com".to_string(),
            initial_data: http_request.as_bytes().to_vec(),
        };

        stream.send_message(&connect_msg).await.unwrap();
        info!(
            "Relay: sent HttpStreamConnect with {} bytes",
            http_request.len()
        );

        // Wait for response data
        info!("Relay: waiting for response...");
        let response = stream.recv_message().await.unwrap().unwrap();

        match response {
            TunnelMessage::HttpStreamData { data, .. } => {
                let response_str = String::from_utf8_lossy(&data);
                info!(
                    "Relay: received response ({} bytes):\n{}",
                    data.len(),
                    response_str
                );

                // Verify it's a valid HTTP response
                assert!(response_str.contains("HTTP/1.1 200 OK"));
                assert!(response_str.contains("Hello from local server!"));

                Ok(data.len())
            }
            other => {
                panic!("Expected HttpStreamData, got {:?}", other);
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // 4. Connect tunnel client
    let client_config = Arc::new(QuicConfig::client_insecure());
    let connector = QuicConnector::new(client_config).unwrap();

    info!("Tunnel client connecting to relay at {}", relay_addr);
    let connection = connector.connect(relay_addr, "localhost").await.unwrap();
    info!("Tunnel client connected");

    // 5. Client accepts stream and handles HttpStreamConnect
    let client_task: tokio::task::JoinHandle<Result<(), String>> = tokio::spawn(async move {
        info!("Client: waiting for streams from relay...");

        // Accept the HTTP stream from relay
        let mut stream = connection.accept_stream().await.unwrap().unwrap();
        info!("Client: accepted stream");

        // Receive HttpStreamConnect
        let msg = stream.recv_message().await.unwrap().unwrap();
        info!("Client: received message: {:?}", msg);

        match msg {
            TunnelMessage::HttpStreamConnect {
                stream_id,
                host,
                initial_data,
            } => {
                info!(
                    "Client: received HttpStreamConnect for {} ({} bytes)",
                    host,
                    initial_data.len()
                );

                // Connect to local HTTP server
                let mut local_socket = tokio::net::TcpStream::connect(local_addr).await.unwrap();
                info!("Client: connected to local server at {}", local_addr);

                // Forward the HTTP request to local server
                use tokio::io::AsyncWriteExt;
                local_socket.write_all(&initial_data).await.unwrap();
                info!("Client: forwarded request to local server");

                // Read response from local server
                use tokio::io::AsyncReadExt;
                let mut response_buffer = vec![0u8; 4096];
                let n = local_socket.read(&mut response_buffer).await.unwrap();
                response_buffer.truncate(n);

                info!("Client: received {} bytes from local server", n);

                // Send response back through tunnel
                let data_msg = TunnelMessage::HttpStreamData {
                    stream_id,
                    data: response_buffer,
                };

                stream.send_message(&data_msg).await.unwrap();
                info!("Client: sent response back through tunnel");

                // Give relay time to receive the message before dropping the stream
                tokio::time::sleep(Duration::from_millis(100)).await;

                Ok(())
            }
            other => {
                panic!("Client: expected HttpStreamConnect, got {:?}", other);
            }
        }
    });

    // Wait for both tasks to complete
    let (relay_result, client_result) = tokio::join!(relay_task, client_task);

    match (relay_result, client_result) {
        (Ok(Ok(bytes_received)), Ok(Ok(()))) => {
            info!(
                "✅ Test PASSED: Transparent streaming worked! Relay received {} bytes",
                bytes_received
            );
        }
        (relay_res, client_res) => {
            panic!(
                "Test failed - Relay: {:?}, Client: {:?}",
                relay_res, client_res
            );
        }
    }
}

/// Test WebSocket transparent streaming through tunnel
#[tokio::test(flavor = "multi_thread")]
async fn test_websocket_transparent_streaming_through_tunnel() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init()
        .ok();

    info!("=== Testing WebSocket Transparent Streaming Through Tunnel ===");

    // 1. Start local WebSocket server
    async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
        ws.on_upgrade(handle_socket)
    }

    async fn handle_socket(mut socket: WebSocket) {
        while let Some(msg) = socket.recv().await {
            if let Ok(msg) = msg {
                match msg {
                    Message::Text(text) => {
                        let response = format!("Echo: {}", text);
                        if socket.send(Message::Text(response.into())).await.is_err() {
                            return;
                        }
                    }
                    Message::Close(_) => return,
                    _ => {}
                }
            }
        }
    }

    let app = Router::new().route("/ws", get(ws_handler));
    let local_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = local_listener.local_addr().unwrap();
    info!("Local WebSocket server started on {}", local_addr);

    tokio::spawn(async move {
        axum::serve(local_listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // 2. Start QUIC tunnel relay
    let server_config = Arc::new(QuicConfig::server_self_signed().unwrap());
    let relay_listener = QuicListener::new("127.0.0.1:0".parse().unwrap(), server_config).unwrap();
    let relay_addr = relay_listener.local_addr().unwrap();
    info!("Tunnel relay started on {}", relay_addr);

    // 3. Simulate exit node forwarding WebSocket upgrade
    let relay_task: tokio::task::JoinHandle<Result<(), String>> = tokio::spawn(async move {
        info!("Relay: waiting for tunnel client...");
        let (connection, _) = relay_listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        let mut stream = connection.open_stream().await.unwrap();

        // WebSocket upgrade request
        let ws_upgrade = "GET /ws HTTP/1.1\r\n\
             Host: example.com\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\n\
             \r\n"
            .to_string();

        let connect_msg = TunnelMessage::HttpStreamConnect {
            stream_id: 43,
            host: "example.com".to_string(),
            initial_data: ws_upgrade.as_bytes().to_vec(),
        };

        stream.send_message(&connect_msg).await.unwrap();
        info!("Relay: sent WebSocket upgrade request");

        // Receive 101 Switching Protocols
        let response = stream.recv_message().await.unwrap().unwrap();
        match response {
            TunnelMessage::HttpStreamData { data, .. } => {
                let response_str = String::from_utf8_lossy(&data);
                info!("Relay: received upgrade response:\n{}", response_str);
                assert!(response_str.contains("101 Switching Protocols"));
                // HTTP headers are case-insensitive (RFC 7230)
                assert!(response_str.to_lowercase().contains("upgrade: websocket"));
            }
            _ => panic!("Expected HttpStreamData with upgrade response"),
        }

        // Send WebSocket text frame
        // Simple WebSocket frame: FIN=1, opcode=1 (text), mask=1, payload="test"
        let ws_frame = vec![
            0x81, // FIN + text frame
            0x84, // Mask + payload len 4
            0x01, 0x02, 0x03, 0x04, // Masking key
            0x75, 0x67, 0x72, 0x71, // Masked "test"
        ];

        let data_msg = TunnelMessage::HttpStreamData {
            stream_id: 43,
            data: ws_frame,
        };
        stream.send_message(&data_msg).await.unwrap();
        info!("Relay: sent WebSocket text frame");

        // Receive echo response
        let response = stream.recv_message().await.unwrap().unwrap();
        match response {
            TunnelMessage::HttpStreamData { data, .. } => {
                info!(
                    "Relay: received WebSocket response frame ({} bytes)",
                    data.len()
                );
                assert!(!data.is_empty());
                Ok(())
            }
            _ => panic!("Expected HttpStreamData with WebSocket frame"),
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // 4. Connect tunnel client (same logic as HTTP test)
    let client_config = Arc::new(QuicConfig::client_insecure());
    let connector = QuicConnector::new(client_config).unwrap();
    let connection = connector.connect(relay_addr, "localhost").await.unwrap();

    let client_task: tokio::task::JoinHandle<Result<(), String>> = tokio::spawn(async move {
        let mut stream = connection.accept_stream().await.unwrap().unwrap();
        let msg = stream.recv_message().await.unwrap().unwrap();

        match msg {
            TunnelMessage::HttpStreamConnect {
                stream_id,
                initial_data,
                ..
            } => {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};

                let mut local_socket = tokio::net::TcpStream::connect(local_addr).await.unwrap();
                local_socket.write_all(&initial_data).await.unwrap();

                // Bidirectional proxy loop
                let (mut local_read, mut local_write) = local_socket.into_split();
                let (mut quic_send, mut quic_recv) = stream.split();

                let to_tunnel = tokio::spawn(async move {
                    let mut buffer = vec![0u8; 8192];
                    loop {
                        match local_read.read(&mut buffer).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let msg = TunnelMessage::HttpStreamData {
                                    stream_id,
                                    data: buffer[..n].to_vec(),
                                };
                                if quic_send.send_message(&msg).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });

                let from_tunnel = tokio::spawn(async move {
                    while let Ok(Some(TunnelMessage::HttpStreamData { data, .. })) =
                        quic_recv.recv_message().await
                    {
                        if local_write.write_all(&data).await.is_err() {
                            break;
                        }
                    }
                });

                tokio::time::sleep(Duration::from_secs(2)).await;
                drop(to_tunnel);
                drop(from_tunnel);

                Ok(())
            }
            _ => panic!("Expected HttpStreamConnect"),
        }
    });

    let (relay_result, client_result) = tokio::join!(relay_task, client_task);

    match (relay_result, client_result) {
        (Ok(Ok(())), Ok(Ok(()))) => {
            info!("✅ WebSocket transparent streaming test PASSED!");
        }
        (relay_res, client_res) => {
            panic!(
                "Test failed - Relay: {:?}, Client: {:?}",
                relay_res, client_res
            );
        }
    }
}
