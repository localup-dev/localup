//! Integration tests for QUIC transport implementation

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tunnel_proto::{Endpoint, Protocol, TunnelConfig, TunnelMessage};
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

/// Helper to create a test server with ephemeral self-signed certificates
async fn create_test_server() -> (QuicListener, SocketAddr) {
    init_crypto_provider();

    // Use QuicConfig::server_ephemeral() which generates unique certificates for each test
    // This avoids conflicts when tests run in parallel
    let config = Arc::new(
        QuicConfig::server_ephemeral()
            .expect("Failed to create server config with ephemeral cert")
            .with_insecure_skip_verify(), // For testing only
    );

    let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = QuicListener::new(bind_addr, config).expect("Failed to create listener");
    let local_addr = listener.local_addr().expect("Failed to get local addr");

    (listener, local_addr)
}

/// Helper to create a test client
fn create_test_client() -> QuicConnector {
    let config = Arc::new(
        QuicConfig::client_default()
            .with_insecure_skip_verify() // For testing only
            .with_idle_timeout(Duration::from_secs(10)),
    );

    QuicConnector::new(config).expect("Failed to create connector")
}

#[tokio::test]
async fn test_quic_connection_establishment() {
    let (listener, server_addr) = create_test_server().await;
    let connector = create_test_client();

    // Spawn server task
    let server_task = tokio::spawn(async move {
        let result = timeout(Duration::from_secs(5), listener.accept()).await;
        result.expect("Server timeout").expect("Accept failed")
    });

    // Connect from client
    let client_conn = timeout(
        Duration::from_secs(5),
        connector.connect(server_addr, "localhost"),
    )
    .await
    .expect("Client timeout")
    .expect("Connect failed");

    // Wait for server to accept
    let (server_conn, remote_addr) = server_task.await.expect("Server task failed");

    // Verify connections
    assert!(!client_conn.is_closed());
    assert!(!server_conn.is_closed());

    // Client's remote address should be the server address
    assert_eq!(client_conn.remote_address(), server_addr);

    // Server's remote address should match the remote_addr returned by accept
    assert_eq!(server_conn.remote_address(), remote_addr);
    assert!(remote_addr.port() > 0);
}

#[tokio::test]
async fn test_quic_stream_creation() {
    let (listener, server_addr) = create_test_server().await;
    let connector = create_test_client();

    // Spawn server task
    let server_task = tokio::spawn(async move {
        let (conn, _) = listener.accept().await.expect("Accept failed");
        conn
    });

    // Connect from client
    let client_conn = connector
        .connect(server_addr, "localhost")
        .await
        .expect("Connect failed");

    let server_conn = server_task.await.expect("Server task failed");

    // Client opens stream and sends a byte to make it visible to server
    let mut client_stream = client_conn
        .open_stream()
        .await
        .expect("Failed to open stream");
    client_stream
        .send_bytes(b"ping")
        .await
        .expect("Failed to send ping");

    // Server accepts stream
    let mut server_stream = timeout(Duration::from_secs(5), server_conn.accept_stream())
        .await
        .expect("Server timeout")
        .expect("Failed to accept stream")
        .expect("No stream available");

    // Server receives the ping
    let data = server_stream
        .recv_bytes(1024)
        .await
        .expect("Failed to receive");
    assert_eq!(&data[..], b"ping");

    // Verify stream IDs
    assert_eq!(client_stream.stream_id(), server_stream.stream_id());
}

#[tokio::test]
async fn test_quic_message_exchange() {
    let (listener, server_addr) = create_test_server().await;
    let connector = create_test_client();

    // Spawn server task
    let server_task = tokio::spawn(async move {
        let (conn, _) = listener.accept().await.expect("Accept failed");
        let mut stream = conn
            .accept_stream()
            .await
            .expect("Failed to accept stream")
            .expect("No stream available");

        // Receive message
        let msg = stream
            .recv_message()
            .await
            .expect("Failed to receive message")
            .expect("No message received");

        // Send response
        let response = TunnelMessage::Connected {
            tunnel_id: "test-tunnel".to_string(),
            endpoints: vec![Endpoint {
                protocol: Protocol::Http {
                    subdomain: Some("test".to_string()),
                },
                public_url: "https://test.tunnel.io".to_string(),
                port: Some(8080),
            }],
        };
        stream
            .send_message(&response)
            .await
            .expect("Failed to send response");

        // Keep the stream alive for a bit to allow client to receive
        tokio::time::sleep(Duration::from_millis(100)).await;

        msg
    });

    // Connect from client
    let client_conn = connector
        .connect(server_addr, "localhost")
        .await
        .expect("Connect failed");

    let mut client_stream = client_conn
        .open_stream()
        .await
        .expect("Failed to open stream");

    // Send Connect message
    let connect_msg = TunnelMessage::Connect {
        tunnel_id: "test-tunnel".to_string(),
        auth_token: "token123".to_string(),
        protocols: vec![Protocol::Http {
            subdomain: Some("test".to_string()),
        }],
        config: TunnelConfig::default(),
    };

    client_stream
        .send_message(&connect_msg)
        .await
        .expect("Failed to send message");

    // Receive response
    let response = timeout(Duration::from_secs(5), client_stream.recv_message())
        .await
        .expect("Client timeout")
        .expect("Failed to receive response")
        .expect("No response received");

    // Wait for server to receive message
    let received_msg = server_task.await.expect("Server task failed");

    // Verify messages
    if let TunnelMessage::Connect { tunnel_id, .. } = received_msg {
        assert_eq!(tunnel_id, "test-tunnel");
    } else {
        panic!("Expected Connect message");
    }

    if let TunnelMessage::Connected {
        tunnel_id,
        endpoints,
    } = response
    {
        assert_eq!(tunnel_id, "test-tunnel");
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].port, Some(8080));
        assert_eq!(endpoints[0].public_url, "https://test.tunnel.io");
    } else {
        panic!("Expected Connected message");
    }
}

#[tokio::test]
async fn test_quic_multiple_streams() {
    let (listener, server_addr) = create_test_server().await;
    let connector = create_test_client();

    // Spawn server task
    let server_task = tokio::spawn(async move {
        let (conn, _) = listener.accept().await.expect("Accept failed");

        // Accept multiple streams
        let mut streams = Vec::new();
        for _ in 0..3 {
            if let Some(stream) = conn.accept_stream().await.expect("Failed to accept stream") {
                streams.push(stream);
            }
        }

        streams.len()
    });

    // Connect from client
    let client_conn = connector
        .connect(server_addr, "localhost")
        .await
        .expect("Connect failed");

    // Open multiple streams and send data to make them visible to server
    let mut stream1 = client_conn
        .open_stream()
        .await
        .expect("Failed to open stream 1");
    let mut stream2 = client_conn
        .open_stream()
        .await
        .expect("Failed to open stream 2");
    let mut stream3 = client_conn
        .open_stream()
        .await
        .expect("Failed to open stream 3");

    // Send data on each stream
    stream1
        .send_bytes(b"1")
        .await
        .expect("Failed to send on stream 1");
    stream2
        .send_bytes(b"2")
        .await
        .expect("Failed to send on stream 2");
    stream3
        .send_bytes(b"3")
        .await
        .expect("Failed to send on stream 3");

    // Verify unique stream IDs
    assert_ne!(stream1.stream_id(), stream2.stream_id());
    assert_ne!(stream2.stream_id(), stream3.stream_id());
    assert_ne!(stream1.stream_id(), stream3.stream_id());

    // Wait for server to accept all streams
    let accepted_count = timeout(Duration::from_secs(5), server_task)
        .await
        .expect("Server timeout")
        .expect("Server task failed");

    assert_eq!(accepted_count, 3);
}

#[tokio::test]
async fn test_quic_connection_close() {
    let (listener, server_addr) = create_test_server().await;
    let connector = create_test_client();

    // Spawn server task
    let server_task = tokio::spawn(async move {
        let (conn, _) = listener.accept().await.expect("Accept failed");

        // Wait a bit for client to close
        tokio::time::sleep(Duration::from_millis(100)).await;

        conn.is_closed()
    });

    // Connect from client
    let client_conn = connector
        .connect(server_addr, "localhost")
        .await
        .expect("Connect failed");

    // Close connection
    client_conn.close(0, "Test close").await;

    // Wait a bit for close to propagate
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify client sees it as closed
    assert!(client_conn.is_closed());

    // Verify server sees it as closed
    let server_closed = server_task.await.expect("Server task failed");
    assert!(server_closed);
}

#[tokio::test]
async fn test_quic_connection_stats() {
    let (listener, server_addr) = create_test_server().await;
    let connector = create_test_client();

    // Spawn server task
    let server_task = tokio::spawn(async move {
        let (conn, _) = listener.accept().await.expect("Accept failed");
        tokio::time::sleep(Duration::from_millis(100)).await;
        conn.stats()
    });

    // Connect from client
    let client_conn = connector
        .connect(server_addr, "localhost")
        .await
        .expect("Connect failed");

    // Get stats
    let client_stats = client_conn.stats();
    let server_stats = server_task.await.expect("Server task failed");

    // Verify stats are populated
    assert!(client_stats.rtt_ms.is_some());
    assert!(server_stats.rtt_ms.is_some());
    assert!(client_stats.uptime_secs < 5); // Should be very recent
}

#[tokio::test]
async fn test_quic_stream_finish() {
    let (listener, server_addr) = create_test_server().await;
    let connector = create_test_client();

    // Spawn server task
    let server_task = tokio::spawn(async move {
        let (conn, _) = listener.accept().await.expect("Accept failed");
        let mut stream = conn
            .accept_stream()
            .await
            .expect("Failed to accept stream")
            .expect("No stream available");

        // Try to receive after client finishes
        let result = stream.recv_message().await;
        (stream.is_closed(), result)
    });

    // Connect from client
    let client_conn = connector
        .connect(server_addr, "localhost")
        .await
        .expect("Connect failed");

    let mut client_stream = client_conn
        .open_stream()
        .await
        .expect("Failed to open stream");

    // Finish stream
    client_stream
        .finish()
        .await
        .expect("Failed to finish stream");

    assert!(client_stream.is_closed());

    // Wait for server to detect close
    let (server_closed, recv_result) = timeout(Duration::from_secs(5), server_task)
        .await
        .expect("Server timeout")
        .expect("Server task failed");

    // Server should see the stream as closed and receive None
    assert!(server_closed || recv_result.unwrap().is_none());
}

#[tokio::test]
async fn test_quic_raw_bytes_exchange() {
    let (listener, server_addr) = create_test_server().await;
    let connector = create_test_client();

    let test_data = b"Hello, QUIC!";

    // Spawn server task
    let server_task = tokio::spawn(async move {
        let (conn, _) = listener.accept().await.expect("Accept failed");
        let mut stream = conn
            .accept_stream()
            .await
            .expect("Failed to accept stream")
            .expect("No stream available");

        // Receive raw bytes
        let data = stream
            .recv_bytes(1024)
            .await
            .expect("Failed to receive bytes");

        // Send response
        stream
            .send_bytes(b"ACK")
            .await
            .expect("Failed to send bytes");

        // Keep stream alive for client to receive
        tokio::time::sleep(Duration::from_millis(100)).await;

        data
    });

    // Connect from client
    let client_conn = connector
        .connect(server_addr, "localhost")
        .await
        .expect("Connect failed");

    let mut client_stream = client_conn
        .open_stream()
        .await
        .expect("Failed to open stream");

    // Send raw bytes
    client_stream
        .send_bytes(test_data)
        .await
        .expect("Failed to send bytes");

    // Receive response
    let response = timeout(Duration::from_secs(5), client_stream.recv_bytes(1024))
        .await
        .expect("Client timeout")
        .expect("Failed to receive bytes");

    // Wait for server to receive data
    let received_data = server_task.await.expect("Server task failed");

    // Verify data
    assert_eq!(&received_data[..], test_data);
    assert_eq!(&response[..], b"ACK");
}
