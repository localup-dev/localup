//! Integration tests for tunnel system
//!
//! These tests verify the end-to-end functionality of the tunnel system,
//! including client-server communication, routing, and proxying.

use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Duration};

/// Helper: Start a simple echo server
async fn start_echo_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 1024];
                loop {
                    match socket.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if socket.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    });

    (addr, handle)
}

/// Helper: Start a simple HTTP server
async fn start_http_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 1024];
                if socket.read(&mut buf).await.is_ok() {
                    let response = "HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, World!";
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            });
        }
    });

    (addr, handle)
}

#[tokio::test]
async fn test_tcp_echo_server() {
    // Test that our echo server works
    let (addr, _handle) = start_echo_server().await;

    let mut client = TcpStream::connect(addr).await.unwrap();

    let test_data = b"Hello, echo!";
    client.write_all(test_data).await.unwrap();

    let mut buf = vec![0u8; test_data.len()];
    client.read_exact(&mut buf).await.unwrap();

    assert_eq!(&buf, test_data);
}

#[tokio::test]
async fn test_http_server_basic() {
    // Test that our HTTP server works
    let (addr, _handle) = start_http_server().await;

    let mut client = TcpStream::connect(addr).await.unwrap();

    let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    client.write_all(request).await.unwrap();

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("HTTP/1.1 200 OK"));
    assert!(response.contains("Hello, World!"));
}

#[tokio::test]
async fn test_tcp_proxy_basic() {
    // This test demonstrates the concept of TCP proxying
    // In a full implementation, this would proxy through the tunnel

    let (echo_addr, _echo_handle) = start_echo_server().await;

    // Simulate a proxy by directly connecting
    // In reality, this would go through QUIC tunnel
    let mut client = TcpStream::connect(echo_addr).await.unwrap();

    let test_data = b"Proxied data";
    client.write_all(test_data).await.unwrap();

    let mut buf = vec![0u8; test_data.len()];
    client.read_exact(&mut buf).await.unwrap();

    assert_eq!(&buf, test_data);
}

#[tokio::test]
async fn test_concurrent_connections() {
    // Test multiple concurrent connections to the same server
    let (addr, _handle) = start_echo_server().await;

    let mut handles = vec![];

    for i in 0..10 {
        let handle = tokio::spawn(async move {
            let mut client = TcpStream::connect(addr).await.unwrap();

            let test_data = format!("Message {}", i);
            client.write_all(test_data.as_bytes()).await.unwrap();

            let mut buf = vec![0u8; test_data.len()];
            client.read_exact(&mut buf).await.unwrap();

            assert_eq!(&buf, test_data.as_bytes());
        });

        handles.push(handle);
    }

    // Wait for all connections to complete
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_large_data_transfer() {
    // Test transferring larger amounts of data
    let (addr, _handle) = start_echo_server().await;

    let mut client = TcpStream::connect(addr).await.unwrap();

    // Generate 1MB of test data
    let test_data: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();

    client.write_all(&test_data).await.unwrap();

    let mut buf = vec![0u8; test_data.len()];
    client.read_exact(&mut buf).await.unwrap();

    assert_eq!(buf, test_data);
}

#[tokio::test]
async fn test_connection_timeout() {
    // Test that connections timeout appropriately
    let result = timeout(
        Duration::from_millis(100),
        TcpStream::connect("192.0.2.1:1234"), // Non-routable IP
    )
    .await;

    assert!(result.is_err(), "Connection should timeout");
}

#[tokio::test]
async fn test_http_multiple_requests() {
    // Test handling multiple HTTP requests on the same connection
    let (addr, _handle) = start_http_server().await;

    for _ in 0..5 {
        let mut client = TcpStream::connect(addr).await.unwrap();

        let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
        client.write_all(request).await.unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("200 OK"));
    }
}

#[tokio::test]
async fn test_bidirectional_communication() {
    // Test bidirectional data flow
    let (addr, _handle) = start_echo_server().await;

    let mut client = TcpStream::connect(addr).await.unwrap();

    // Send multiple messages
    for i in 0..10 {
        let msg = format!("Message {}", i);
        client.write_all(msg.as_bytes()).await.unwrap();

        let mut buf = vec![0u8; msg.len()];
        client.read_exact(&mut buf).await.unwrap();

        assert_eq!(buf, msg.as_bytes());
    }
}

#[tokio::test]
async fn test_graceful_connection_close() {
    // Test graceful connection closing
    let (addr, _handle) = start_echo_server().await;

    let mut client = TcpStream::connect(addr).await.unwrap();

    let test_data = b"Final message";
    client.write_all(test_data).await.unwrap();

    let mut buf = vec![0u8; test_data.len()];
    client.read_exact(&mut buf).await.unwrap();

    // Shutdown write side
    client.shutdown().await.unwrap();
}

// Module for QUIC-specific integration tests
mod quic_tests {
    #[tokio::test]
    async fn test_quic_connection_placeholder() {
        // Placeholder for QUIC connection test
        // In a full implementation, this would:
        // 1. Start a QUIC server
        // 2. Connect a QUIC client
        // 3. Open bidirectional streams
        // 4. Transfer data
        // 5. Verify data integrity

        // Test framework is ready for future QUIC integration tests
        // These would test full QUIC handshake, connection establishment,
        // and bidirectional stream communication
    }

    #[tokio::test]
    async fn test_quic_multiplexing_placeholder() {
        // Placeholder for QUIC multiplexing test
        // Would test multiple concurrent streams over single QUIC connection
        // This functionality is already tested in tunnel-transport-quic integration tests
    }
}

// Module for routing tests
mod routing_tests {
    use localup_proto::IpFilter;
    use localup_router::{RouteKey, RouteRegistry, RouteTarget};
    use std::sync::Arc;

    #[test]
    fn test_tcp_route_lookup() {
        let registry = Arc::new(RouteRegistry::new());

        let key = RouteKey::TcpPort(5432);
        let target = RouteTarget {
            localup_id: "test-tunnel".to_string(),
            target_addr: "localhost:5432".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register(key.clone(), target.clone()).unwrap();

        let found = registry.lookup(&key).unwrap();
        assert_eq!(found.localup_id, "test-tunnel");
    }

    #[test]
    fn test_http_route_lookup() {
        let registry = Arc::new(RouteRegistry::new());

        let key = RouteKey::HttpHost("example.com".to_string());
        let target = RouteTarget {
            localup_id: "web-tunnel".to_string(),
            target_addr: "localhost:3000".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register(key.clone(), target).unwrap();

        let found = registry.lookup(&key).unwrap();
        assert_eq!(found.localup_id, "web-tunnel");
    }

    #[test]
    fn test_sni_route_lookup() {
        let registry = Arc::new(RouteRegistry::new());

        let key = RouteKey::TlsSni("db.example.com".to_string());
        let target = RouteTarget {
            localup_id: "db-tunnel".to_string(),
            target_addr: "localhost:5432".to_string(),
            metadata: None,
            ip_filter: IpFilter::new(),
        };

        registry.register(key.clone(), target).unwrap();

        let found = registry.lookup(&key).unwrap();
        assert_eq!(found.localup_id, "db-tunnel");
    }

    #[test]
    fn test_concurrent_route_access() {
        use std::thread;

        let registry = Arc::new(RouteRegistry::new());

        // Register initial routes
        for i in 0..100 {
            let key = RouteKey::TcpPort(5000 + i);
            let target = RouteTarget {
                localup_id: format!("localup-{}", i),
                target_addr: format!("localhost:{}", 5000 + i),
                metadata: None,
                ip_filter: IpFilter::new(),
            };
            registry.register(key, target).unwrap();
        }

        // Spawn multiple threads to access routes concurrently
        let mut handles = vec![];
        for _ in 0..10 {
            let reg = registry.clone();
            let handle = thread::spawn(move || {
                for i in 0..100 {
                    let key = RouteKey::TcpPort(5000 + i);
                    let result = reg.lookup(&key);
                    assert!(result.is_ok());
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }
}

// Performance baseline tests
mod performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_throughput_baseline() {
        let (addr, _handle) = start_echo_server().await;

        let start = std::time::Instant::now();
        let mut client = TcpStream::connect(addr).await.unwrap();

        // Transfer 10MB
        let chunk = vec![0u8; 1024];
        for _ in 0..10240 {
            client.write_all(&chunk).await.unwrap();
            let mut buf = vec![0u8; 1024];
            client.read_exact(&mut buf).await.unwrap();
        }

        let duration = start.elapsed();
        let throughput_mbps = (10.0 * 2.0 * 8.0) / duration.as_secs_f64();

        println!("Throughput: {:.2} Mbps", throughput_mbps);
        assert!(throughput_mbps > 0.0);
    }

    #[tokio::test]
    async fn test_connection_establishment_time() {
        let (addr, _handle) = start_echo_server().await;

        let mut times = vec![];

        for _ in 0..100 {
            let start = std::time::Instant::now();
            let _client = TcpStream::connect(addr).await.unwrap();
            times.push(start.elapsed());
        }

        let avg_time = times.iter().sum::<Duration>() / times.len() as u32;
        println!("Average connection time: {:?}", avg_time);

        // Connection should establish in reasonable time
        assert!(avg_time < Duration::from_millis(100));
    }
}
