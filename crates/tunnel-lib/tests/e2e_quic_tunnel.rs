//! End-to-end integration test for QUIC tunnel messaging
//!
//! This test verifies the bidirectional message flow over QUIC:
//! 1. Server: QUIC listener accepts client connection
//! 2. Client: Sends Connect message over control stream
//! 3. Server: Sends Connected response
//! 4. Server: Sends TcpConnect to client (simulating TCP proxy)
//! 5. Client: Sends TcpData response back
//! 6. Server: Receives TcpData successfully

use std::sync::Arc;
use std::time::Duration;
use tracing::info;

use tunnel_proto::{Endpoint, Protocol, TunnelMessage};
use tunnel_transport::{
    TransportConnection, TransportConnector, TransportListener, TransportStream,
};
use tunnel_transport_quic::{QuicConfig, QuicConnector, QuicListener};

#[tokio::test(flavor = "multi_thread")]
async fn test_quic_tunnel_message_flow() {
    // Initialize rustls crypto provider
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init()
        .ok();

    info!("=== Starting QUIC Tunnel Message Flow Test ===");

    // Step 1: Start QUIC listener (server side)
    let server_config = Arc::new(QuicConfig::server_self_signed().unwrap());
    let listener = QuicListener::new("127.0.0.1:0".parse().unwrap(), server_config).unwrap();
    let server_addr = listener.local_addr().unwrap();
    info!("Server: QUIC listener started on {}", server_addr);

    // Step 2: Accept connection in background and handle messages
    let server_task: tokio::task::JoinHandle<Result<usize, String>> = tokio::spawn(async move {
        info!("Server: waiting for connection...");
        let (connection, peer_addr) = listener.accept().await.unwrap();
        info!("Server: accepted connection from {}", peer_addr);

        // Accept control stream from client
        let mut control_stream = connection.accept_stream().await.unwrap().unwrap();
        info!("Server: accepted control stream");

        // Read Connect message
        let msg = control_stream.recv_message().await.unwrap().unwrap();
        info!("Server: received message: {:?}", msg);

        match msg {
            TunnelMessage::Connect { tunnel_id, .. } => {
                info!("Server: received Connect for tunnel {}", tunnel_id);

                // Send Connected response
                let connected_msg = TunnelMessage::Connected {
                    tunnel_id: tunnel_id.clone(),
                    endpoints: vec![Endpoint {
                        protocol: Protocol::Tcp { port: 8080 },
                        public_url: "tcp://localhost:17336".to_string(),
                        port: Some(17336),
                    }],
                };

                control_stream.send_message(&connected_msg).await.unwrap();
                info!("Server: sent Connected response");

                // Simulate TCP proxy sending TcpConnect to client
                tokio::time::sleep(Duration::from_millis(100)).await;

                let tcp_connect_msg = TunnelMessage::TcpConnect {
                    stream_id: 1,
                    remote_addr: "127.0.0.1".to_string(),
                    remote_port: 12345,
                };

                info!("Server: sending TcpConnect to client (stream 1)");
                control_stream.send_message(&tcp_connect_msg).await.unwrap();
                info!("Server: TcpConnect sent");

                // Wait for TcpData response from client
                info!("Server: waiting for TcpData response...");
                match control_stream.recv_message().await {
                    Ok(Some(TunnelMessage::TcpData { stream_id, data })) => {
                        info!(
                            "Server: ✅ received TcpData (stream {}): {} bytes",
                            stream_id,
                            data.len()
                        );
                        assert_eq!(stream_id, 1);
                        assert!(!data.is_empty());
                        Ok(data.len())
                    }
                    Ok(Some(other)) => {
                        panic!("Server: expected TcpData, got {:?}", other);
                    }
                    Ok(None) => {
                        panic!("Server: connection closed before receiving TcpData");
                    }
                    Err(e) => {
                        panic!("Server: error receiving message: {}", e);
                    }
                }
            }
            other => {
                panic!("Server: expected Connect, got {:?}", other);
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Step 3: Connect client via QUIC
    let client_config = Arc::new(QuicConfig::client_insecure());
    let connector = QuicConnector::new(client_config).unwrap();

    info!("Client: connecting to {}", server_addr);
    let connection = connector.connect(server_addr, "localhost").await.unwrap();
    info!("Client: QUIC connection established");

    // Open control stream
    let mut control_stream = connection.open_stream().await.unwrap();
    info!("Client: control stream opened");

    // Send Connect message
    let connect_msg = TunnelMessage::Connect {
        tunnel_id: "test-tunnel-123".to_string(),
        auth_token: "test-token".to_string(),
        protocols: vec![Protocol::Tcp { port: 8080 }],
        config: tunnel_proto::TunnelConfig::default(),
    };

    control_stream.send_message(&connect_msg).await.unwrap();
    info!("Client: sent Connect message");

    // Receive Connected response
    let response = control_stream.recv_message().await.unwrap().unwrap();
    info!("Client: received response: {:?}", response);

    match response {
        TunnelMessage::Connected {
            tunnel_id,
            endpoints,
        } => {
            info!("Client: tunnel registered as {}", tunnel_id);
            assert_eq!(endpoints.len(), 1);
        }
        other => {
            panic!("Client: expected Connected, got {:?}", other);
        }
    }

    // Receive TcpConnect from server
    info!("Client: waiting for TcpConnect...");
    let msg = control_stream.recv_message().await.unwrap().unwrap();
    info!("Client: received message: {:?}", msg);

    match msg {
        TunnelMessage::TcpConnect {
            stream_id,
            remote_addr,
            remote_port,
        } => {
            info!(
                "Client: received TcpConnect (stream {}) from {}:{}",
                stream_id, remote_addr, remote_port
            );

            // Send TcpData response
            let response_data = b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, World!";
            let tcp_data_msg = TunnelMessage::TcpData {
                stream_id,
                data: response_data.to_vec(),
            };

            info!(
                "Client: sending TcpData response ({} bytes)",
                response_data.len()
            );
            control_stream.send_message(&tcp_data_msg).await.unwrap();
            info!("Client: ✅ TcpData sent successfully");
        }
        other => {
            panic!("Client: expected TcpConnect, got {:?}", other);
        }
    }

    // Wait for server task to complete
    match tokio::time::timeout(Duration::from_secs(5), server_task).await {
        Ok(Ok(Ok(bytes_received))) => {
            info!(
                "=== Test PASSED: Server received {} bytes ===",
                bytes_received
            );
        }
        Ok(Ok(Err(e))) => {
            panic!("Server task failed: {:?}", e);
        }
        Ok(Err(e)) => {
            panic!("Server task panicked: {:?}", e);
        }
        Err(_) => {
            panic!("Test timeout - server never received TcpData response");
        }
    }
}
