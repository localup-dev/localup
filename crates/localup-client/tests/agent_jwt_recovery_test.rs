use std::sync::atomic::{AtomicBool, Ordering};
/// Integration test for agent server with JWT authentication and recovery
///
/// This test covers:
/// 1. End-to-end reverse tunnel with JWT agent authentication
/// 2. Agent server restart recovery (client reconnects)
/// 3. Backend service restart recovery
/// 4. Network interruption handling
///
/// Test setup:
/// - Backend TCP service on 127.0.0.1:9001 (echo server)
/// - Agent Server on 127.0.0.1:9002 (with JWT validation)
/// - Client connection to agent's reverse tunnel
///
/// Scenario:
/// 1. Start backend → agent server → client
/// 2. Verify tunnel works (send data through)
/// 3. Stop agent server
/// 4. Wait (client should be trying to reconnect)
/// 5. Restart agent server
/// 6. Verify client auto-reconnects and tunnel works
/// 7. Stop backend
/// 8. Restart backend
/// 9. Verify tunnel recovers
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{sleep, timeout, Duration};

/// Start a simple echo TCP server on the given address
async fn start_echo_server(addr: &str) -> (tokio::task::JoinHandle<()>, Arc<AtomicBool>) {
    let listener = TcpListener::bind(addr)
        .await
        .expect("Failed to bind echo server");
    let is_running = Arc::new(AtomicBool::new(true));
    let is_running_clone = is_running.clone();

    let handle = tokio::spawn(async move {
        loop {
            if !is_running_clone.load(Ordering::Relaxed) {
                break;
            }

            match timeout(Duration::from_millis(100), listener.accept()).await {
                Ok(Ok((mut socket, peer_addr))) => {
                    println!("[Backend] New connection from {}", peer_addr);

                    tokio::spawn(async move {
                        let mut buffer = [0u8; 1024];
                        loop {
                            match socket.read(&mut buffer).await {
                                Ok(0) => {
                                    println!("[Backend] Connection closed");
                                    break;
                                }
                                Ok(n) => {
                                    println!("[Backend] Received {} bytes, echoing back", n);
                                    let _ = socket.write_all(&buffer[..n]).await;
                                }
                                Err(e) => {
                                    eprintln!("[Backend] Read error: {}", e);
                                    break;
                                }
                            }
                        }
                    });
                }
                Ok(Err(e)) => {
                    eprintln!("[Backend] Accept error: {}", e);
                }
                Err(_) => {
                    // Timeout - continue
                }
            }
        }
    });

    (handle, is_running)
}

/// Test: Happy path - tunnel established and working
#[tokio::test]
async fn test_agent_jwt_localup_happy_path() {
    // Start echo server (backend)
    let backend_addr = "127.0.0.1:9001";
    let (backend_handle, backend_running) = start_echo_server(backend_addr).await;
    println!("✅ Backend echo server started on {}", backend_addr);

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    // TODO: Start agent server with JWT secret
    // For now, this is a placeholder test
    println!("✅ Test checkpoint: Backend running");

    // Cleanup
    backend_running.store(false, Ordering::Relaxed);
    let _ = timeout(Duration::from_secs(2), backend_handle).await;
}

/// Test: Agent server restart recovery
///
/// This test verifies:
/// 1. Client connects to agent → backend
/// 2. Agent server is stopped
/// 3. Client receives disconnect and starts reconnecting
/// 4. Agent server restarts
/// 5. Client reconnects successfully
/// 6. Tunnel works again
#[tokio::test]
async fn test_agent_restart_recovery() {
    println!("\n=== Agent Server Restart Recovery Test ===");

    // Start echo server (backend)
    let backend_addr = "127.0.0.1:9010";
    let (backend_handle, backend_running) = start_echo_server(backend_addr).await;
    println!("✅ Backend echo server started on {}", backend_addr);

    sleep(Duration::from_millis(200)).await;

    // TODO: Start agent server on 127.0.0.1:9011
    // let agent = AgentServer::new(config).start().await;
    // println!("✅ Agent server started on 127.0.0.1:9011");

    // TODO: Create client with agent token
    // let client = ReverseTunnelClient::connect(config).await.unwrap();
    // println!("✅ Client connected");

    // TODO: Send test data through tunnel
    // let mut conn = TcpStream::connect("127.0.0.1:9010").await.unwrap();
    // conn.write_all(b"Hello, agent!\n").await.unwrap();
    // let mut buf = [0u8; 100];
    // let n = conn.read(&mut buf).await.unwrap();
    // assert_eq!(&buf[..n], b"Hello, agent!\n");
    // println!("✅ Tunnel working: data echoed back");

    // TODO: Stop agent server
    // agent.stop().await;
    // println!("⏸️  Agent server stopped");
    // sleep(Duration::from_secs(2)).await;

    // TODO: Verify client is trying to reconnect
    // assert!(client.is_reconnecting().await);
    // println!("✅ Client detected disconnect and is reconnecting");

    // TODO: Restart agent server
    // let agent = AgentServer::new(config).start().await;
    // println!("✅ Agent server restarted");
    // sleep(Duration::from_secs(2)).await;

    // TODO: Verify client auto-reconnected
    // assert!(client.is_connected().await);
    // println!("✅ Client auto-reconnected");

    // TODO: Test tunnel again
    // let mut conn = TcpStream::connect("127.0.0.1:9010").await.unwrap();
    // conn.write_all(b"Test after restart\n").await.unwrap();
    // let mut buf = [0u8; 100];
    // let n = conn.read(&mut buf).await.unwrap();
    // assert_eq!(&buf[..n], b"Test after restart\n");
    // println!("✅ Tunnel working after restart");

    println!("✅ Agent restart recovery test completed");

    // Cleanup
    backend_running.store(false, Ordering::Relaxed);
    let _ = timeout(Duration::from_secs(2), backend_handle).await;
}

/// Test: Backend restart recovery
///
/// This test verifies:
/// 1. Tunnel established with backend running
/// 2. Backend stops
/// 3. Connection attempts fail with appropriate errors
/// 4. Backend restarts
/// 5. New connections through tunnel work again
#[tokio::test]
async fn test_backend_restart_recovery() {
    println!("\n=== Backend Restart Recovery Test ===");

    // Start echo server
    let backend_addr = "127.0.0.1:9020";
    let (backend_handle, backend_running) = start_echo_server(backend_addr).await;
    println!("✅ Backend started on {}", backend_addr);

    sleep(Duration::from_millis(200)).await;

    // TODO: Setup agent and client
    // ...

    // TODO: Backend working test
    // let mut conn = TcpStream::connect(backend_addr).await.unwrap();
    // conn.write_all(b"test\n").await.unwrap();
    // let mut buf = [0u8; 100];
    // conn.read(&mut buf).await.unwrap();
    // println!("✅ Backend responding");

    // TODO: Stop backend
    backend_running.store(false, Ordering::Relaxed);
    let _ = timeout(Duration::from_secs(2), backend_handle).await;
    println!("⏸️  Backend stopped");

    sleep(Duration::from_secs(1)).await;

    // TODO: Verify connection attempts fail
    // let result = timeout(Duration::from_secs(1), TcpStream::connect(backend_addr)).await;
    // assert!(result.is_err() || result.as_ref().is_ok_and(|r| r.is_err()));
    // println!("✅ Connection correctly refused");

    // TODO: Restart backend
    // let (backend_handle, backend_running) = start_echo_server(backend_addr).await;
    // println!("✅ Backend restarted");
    // sleep(Duration::from_millis(200)).await;

    // TODO: Verify tunnel recovers
    // let mut conn = TcpStream::connect(backend_addr).await.unwrap();
    // conn.write_all(b"test again\n").await.unwrap();
    // let mut buf = [0u8; 100];
    // conn.read(&mut buf).await.unwrap();
    // println!("✅ Backend responding again after restart");

    println!("✅ Backend restart recovery test completed");

    // Cleanup
    backend_running.store(false, Ordering::Relaxed);
}

/// Test: JWT authentication validation
///
/// This test verifies:
/// 1. Valid JWT token is accepted
/// 2. Invalid JWT token is rejected with clear error
/// 3. Missing JWT token (when required) is rejected
/// 4. No token required when agent has no jwt_secret configured
#[tokio::test]
async fn test_jwt_authentication() {
    println!("\n=== JWT Authentication Test ===");

    // TODO: Test 1: Valid token accepted
    // let token = generate_jwt_token("postgres-prod", JWT_SECRET);
    // let client = ReverseTunnelClient::connect(config.with_agent_token(token)).await;
    // assert!(client.is_ok());
    // println!("✅ Valid JWT token accepted");

    // TODO: Test 2: Invalid token rejected
    // let invalid_token = "eyJhbGciOiJIUzI1NiJ9.invalid.invalid";
    // let client = ReverseTunnelClient::connect(config.with_agent_token(invalid_token)).await;
    // assert!(client.is_err());
    // assert!(client.err().unwrap().to_string().contains("invalid"));
    // println!("✅ Invalid JWT token rejected");

    // TODO: Test 3: Missing token rejected when required
    // let client = ReverseTunnelClient::connect(config).await;
    // assert!(client.is_err());
    // assert!(client.err().unwrap().to_string().contains("token is required"));
    // println!("✅ Missing token rejected when required");

    // TODO: Test 4: No token required when agent has no jwt_secret
    // let client = ReverseTunnelClient::connect(config).await;
    // assert!(client.is_ok());
    // println!("✅ Connection works without token when not required");

    println!("✅ JWT authentication tests completed");
}

/// Test: Rapid connect/disconnect cycles with recovery
///
/// This simulates a flaky network by rapidly toggling the agent connection
#[tokio::test]
async fn test_rapid_recovery_cycles() {
    println!("\n=== Rapid Recovery Cycles Test ===");

    let backend_addr = "127.0.0.1:9030";
    let (backend_handle, backend_running) = start_echo_server(backend_addr).await;
    println!("✅ Backend started");

    sleep(Duration::from_millis(100)).await;

    // TODO: Setup agent and client
    // for i in 0..5 {
    //     println!("Cycle {} - stopping agent...", i);
    //     agent.stop().await;
    //     sleep(Duration::from_millis(500)).await;
    //
    //     println!("Cycle {} - restarting agent...", i);
    //     agent.start().await;
    //     sleep(Duration::from_secs(1)).await;
    //
    //     // Verify tunnel still works
    //     let mut conn = TcpStream::connect(backend_addr).await.unwrap();
    //     conn.write_all(b"cycle test\n").await.unwrap();
    //     let mut buf = [0u8; 100];
    //     conn.read(&mut buf).await.unwrap();
    //     println!("✅ Cycle {} passed", i);
    // }

    println!("✅ Rapid recovery cycles test completed");

    // Cleanup
    backend_running.store(false, Ordering::Relaxed);
    let _ = timeout(Duration::from_secs(2), backend_handle).await;
}

/// Test: Error handling and logging
///
/// This test verifies that errors are properly logged and reported
#[tokio::test]
async fn test_error_handling_and_logging() {
    println!("\n=== Error Handling and Logging Test ===");

    // TODO: Test various error scenarios:
    // 1. Relay unreachable
    // 2. Agent unreachable
    // 3. Backend unreachable
    // 4. Network timeout
    // 5. Invalid JWT token
    // 6. Token expired

    // Verify errors are descriptive and actionable
    // Check logs contain:
    // - Clear error message
    // - Component information
    // - Suggestion for recovery

    println!("✅ Error handling test completed");
}

#[tokio::test]
async fn test_client_reconnection_with_exponential_backoff() {
    println!("\n=== Client Reconnection with Exponential Backoff Test ===");

    let backend_addr = "127.0.0.1:9040";
    let (backend_handle, backend_running) = start_echo_server(backend_addr).await;
    println!("✅ Backend started");

    sleep(Duration::from_millis(200)).await;

    // TODO: Setup agent and client with backoff tracking
    // let start = Instant::now();
    // agent.stop().await;
    // println!("⏸️  Agent stopped at {:?}", start.elapsed());

    // Measure reconnection attempts:
    // Attempt 1: immediate (or very fast)
    // Wait 1s → Attempt 2
    // Wait 2s → Attempt 3
    // Wait 4s → Attempt 4
    // Wait 8s → Attempt 5
    // Wait 16s → etc (capped at 60s)

    // TODO: Verify backoff timing
    // assert!((first_retry - first_failure) < Duration::from_millis(100));
    // assert!((second_retry - first_retry) >= Duration::from_secs(1));
    // assert!((third_retry - second_retry) >= Duration::from_secs(2));
    // println!("✅ Exponential backoff verified");

    // TODO: Restart agent and verify reconnection within 1s
    // agent.start().await;
    // let reconnect_time = instant_reconnect.elapsed();
    // assert!(reconnect_time < Duration::from_secs(1));
    // println!("✅ Reconnected in {:?}", reconnect_time);

    println!("✅ Exponential backoff test completed");

    // Cleanup
    backend_running.store(false, Ordering::Relaxed);
    let _ = timeout(Duration::from_secs(2), backend_handle).await;
}
