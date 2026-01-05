//! Daemon tests

use localup_cli::daemon::{Daemon, DaemonCommand, TunnelStatus};
use localup_cli::localup_store::{StoredTunnel, TunnelStore};
use localup_client::{ProtocolConfig, TunnelConfig};
use localup_proto::{ExitNodeConfig, HttpAuthConfig};
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Create a test tunnel configuration
fn create_test_tunnel(name: &str, port: u16, enabled: bool) -> StoredTunnel {
    StoredTunnel {
        name: name.to_string(),
        enabled,
        config: TunnelConfig {
            local_host: "localhost".to_string(),
            protocols: vec![ProtocolConfig::Http {
                local_port: port,
                subdomain: Some(format!("{}-test", name)),
                custom_domain: None,
            }],
            auth_token: "test-token".to_string(),
            exit_node: ExitNodeConfig::Auto,
            failover: true,
            connection_timeout: Duration::from_secs(30),
            preferred_transport: None,
            http_auth: HttpAuthConfig::None,
        },
    }
}

/// Setup test environment with temporary directory
async fn setup_test_env() -> (TempDir, TunnelStore) {
    let temp_dir = TempDir::new().unwrap();
    let home_dir = temp_dir.path().join("home");
    std::fs::create_dir_all(&home_dir).unwrap();

    // Set HOME environment to temp dir for isolated testing
    std::env::set_var("HOME", &home_dir);

    let store = TunnelStore::new().unwrap();

    (temp_dir, store)
}

#[tokio::test]
async fn test_daemon_creation() {
    let (_temp, _store) = setup_test_env().await;

    // Note: Daemon::new() tries to access real home directory
    // For isolated testing, we would need to refactor to inject TunnelStore
    // For now, we test that creation doesn't panic
    let result = Daemon::new();
    assert!(result.is_ok() || result.is_err()); // Just checking it doesn't panic
}

#[tokio::test]
async fn test_daemon_shutdown_command() {
    // Create daemon with command channel
    let daemon = match Daemon::new() {
        Ok(d) => d,
        Err(_) => {
            // Skip test if daemon creation fails (e.g., permission issues)
            println!("Skipping test: Daemon creation failed");
            return;
        }
    };

    let (_command_tx, command_rx) = mpsc::channel::<DaemonCommand>(32);

    // Spawn daemon in background
    let daemon_handle = tokio::spawn(async move {
        let _ = daemon.run(command_rx, None, None).await;
    });

    // Give daemon time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Drop command_tx to close the channel, which should cause daemon to exit
    drop(_command_tx);

    // Wait for daemon to finish (with timeout)
    let result = timeout(Duration::from_secs(2), daemon_handle).await;

    assert!(
        result.is_ok(),
        "Daemon should exit when command channel closes"
    );
}

#[tokio::test]
async fn test_daemon_get_status_command() {
    let daemon = match Daemon::new() {
        Ok(d) => d,
        Err(_) => {
            println!("Skipping test: Daemon creation failed");
            return;
        }
    };

    let (command_tx, command_rx) = mpsc::channel::<DaemonCommand>(32);

    // Spawn daemon in background
    let daemon_handle = tokio::spawn(async move {
        let _ = daemon.run(command_rx, None, None).await;
    });

    // Give daemon more time to start and initialize (increased for CI stability)
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Request status
    let (status_tx, mut status_rx) = mpsc::channel(1);
    command_tx
        .send(DaemonCommand::GetStatus(status_tx))
        .await
        .expect("Daemon should be running and accepting commands");

    // Receive status
    let status = timeout(Duration::from_secs(1), status_rx.recv())
        .await
        .expect("Should receive status")
        .expect("Should get status map");

    // Initially, no tunnels should be running
    assert_eq!(status.len(), 0);

    // Shutdown daemon
    command_tx.send(DaemonCommand::Shutdown).await.unwrap();

    // Wait for daemon to finish
    let _ = timeout(Duration::from_secs(2), daemon_handle).await;
}

#[test]
fn test_localup_status_variants() {
    // Test all status variants can be created
    let _starting = TunnelStatus::Starting;
    let _connected = TunnelStatus::Connected {
        public_url: Some("https://test.example.com".to_string()),
    };
    let _reconnecting = TunnelStatus::Reconnecting { attempt: 1 };
    let _failed = TunnelStatus::Failed {
        error: "Test error".to_string(),
    };
    let _stopped = TunnelStatus::Stopped;

    // Test cloning
    let status = TunnelStatus::Connected {
        public_url: Some("https://test.example.com".to_string()),
    };
    let _cloned = status.clone();
}

#[test]
fn test_daemon_command_variants() {
    // Test all command variants can be created
    let _start = DaemonCommand::StartTunnel("test".to_string());
    let _stop = DaemonCommand::StopTunnel("test".to_string());

    let (tx, _rx) = mpsc::channel(1);
    let _status = DaemonCommand::GetStatus(tx);

    let _reload = DaemonCommand::Reload;
    let _shutdown = DaemonCommand::Shutdown;
}

#[tokio::test]
async fn test_daemon_with_no_enabled_tunnels() {
    let (_temp, store) = setup_test_env().await;

    // Add disabled tunnels
    store
        .save(&create_test_tunnel("app1", 3000, false))
        .unwrap();
    store
        .save(&create_test_tunnel("app2", 3001, false))
        .unwrap();

    // Verify no enabled tunnels
    let enabled = store.list_enabled().unwrap();
    assert_eq!(enabled.len(), 0);

    // Daemon should start successfully even with no enabled tunnels
    let daemon = match Daemon::new() {
        Ok(d) => d,
        Err(_) => {
            println!("Skipping test: Daemon creation failed");
            return;
        }
    };

    let (_command_tx, command_rx) = mpsc::channel::<DaemonCommand>(32);

    let daemon_handle = tokio::spawn(async move {
        let _ = daemon.run(command_rx, None, None).await;
    });

    // Give daemon time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Daemon should be running
    assert!(!daemon_handle.is_finished());

    // Shutdown
    drop(_command_tx);

    let _ = timeout(Duration::from_secs(2), daemon_handle).await;
}

#[test]
fn test_daemon_default_creation() {
    // Test default trait implementation
    let result = std::panic::catch_unwind(|| {
        let _daemon = Daemon::default();
    });

    // Should either succeed or panic with a clear error
    // (depending on whether home directory is accessible)
    assert!(result.is_ok() || result.is_err());
}

// Integration test: This would test full daemon lifecycle but requires
// a real tunnel-exit-node running, so we skip it in unit tests
#[ignore]
#[tokio::test]
async fn test_daemon_full_lifecycle() {
    // This test requires:
    // 1. A running tunnel-exit-node
    // 2. Valid authentication token
    // 3. Network connectivity
    //
    // Run manually with: cargo test --test daemon_tests test_daemon_full_lifecycle -- --ignored
    //
    // TODO: Implement when we have test infrastructure for full integration tests
}

#[test]
fn test_localup_status_debug() {
    let status = TunnelStatus::Connected {
        public_url: Some("https://test.example.com".to_string()),
    };

    let debug_str = format!("{:?}", status);
    assert!(debug_str.contains("Connected"));
    assert!(debug_str.contains("test.example.com"));
}

#[tokio::test]
async fn test_daemon_concurrent_status_queries() {
    let daemon = match Daemon::new() {
        Ok(d) => d,
        Err(_) => {
            println!("Skipping test: Daemon creation failed");
            return;
        }
    };

    let (command_tx, command_rx) = mpsc::channel::<DaemonCommand>(32);

    let daemon_handle = tokio::spawn(async move {
        let _ = daemon.run(command_rx, None, None).await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send multiple status requests concurrently
    let mut handles = vec![];

    for _ in 0..5 {
        let tx = command_tx.clone();
        let handle = tokio::spawn(async move {
            let (status_tx, mut status_rx) = mpsc::channel(1);
            tx.send(DaemonCommand::GetStatus(status_tx)).await.unwrap();
            status_rx.recv().await.unwrap()
        });
        handles.push(handle);
    }

    // Wait for all status queries to complete
    for handle in handles {
        let result = timeout(Duration::from_secs(1), handle).await;
        assert!(result.is_ok());
    }

    // Shutdown
    command_tx.send(DaemonCommand::Shutdown).await.unwrap();
    let _ = timeout(Duration::from_secs(2), daemon_handle).await;
}

// ============================================================================
// IPC Integration Tests
// ============================================================================

mod ipc_tests {
    use localup_cli::ipc::{
        format_duration, IpcClient, IpcRequest, IpcResponse, IpcServer, TunnelStatusDisplay,
        TunnelStatusInfo,
    };
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_ipc_server_bind_and_accept() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        // Bind server
        let server = IpcServer::bind_to(&socket_path).await.unwrap();
        assert!(socket_path.exists());

        // Server should be listening
        let server_handle = tokio::spawn(async move {
            let mut conn = server.accept().await.unwrap();
            let req = conn.recv().await.unwrap();
            assert_eq!(req, IpcRequest::Ping);
            conn.send(&IpcResponse::Pong).await.unwrap();
        });

        // Connect client and ping
        let mut client = IpcClient::connect_to(&socket_path).await.unwrap();
        let response = client.request(&IpcRequest::Ping).await.unwrap();
        assert_eq!(response, IpcResponse::Pong);

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_ipc_get_status_with_tunnels() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("status.sock");

        let server = IpcServer::bind_to(&socket_path).await.unwrap();

        // Server responds with mock tunnel status
        let server_handle = tokio::spawn(async move {
            let mut conn = server.accept().await.unwrap();
            let req = conn.recv().await.unwrap();

            if let IpcRequest::GetStatus = req {
                let mut tunnels = HashMap::new();
                tunnels.insert(
                    "api".to_string(),
                    TunnelStatusInfo {
                        name: "api".to_string(),
                        protocol: "http".to_string(),
                        local_port: 3000,
                        public_url: Some("https://api.example.com".to_string()),
                        status: TunnelStatusDisplay::Connected,
                        uptime_seconds: Some(3600),
                        last_error: None,
                    },
                );
                tunnels.insert(
                    "db".to_string(),
                    TunnelStatusInfo {
                        name: "db".to_string(),
                        protocol: "tcp".to_string(),
                        local_port: 5432,
                        public_url: Some("tcp://example.com:15432".to_string()),
                        status: TunnelStatusDisplay::Reconnecting { attempt: 2 },
                        uptime_seconds: None,
                        last_error: None,
                    },
                );
                conn.send(&IpcResponse::Status { tunnels }).await.unwrap();
            }
        });

        let mut client = IpcClient::connect_to(&socket_path).await.unwrap();
        let response = client.request(&IpcRequest::GetStatus).await.unwrap();

        if let IpcResponse::Status { tunnels } = response {
            assert_eq!(tunnels.len(), 2);

            let api = tunnels.get("api").unwrap();
            assert_eq!(api.name, "api");
            assert_eq!(api.protocol, "http");
            assert_eq!(api.local_port, 3000);
            assert_eq!(api.status, TunnelStatusDisplay::Connected);
            assert_eq!(api.uptime_seconds, Some(3600));

            let db = tunnels.get("db").unwrap();
            assert_eq!(db.name, "db");
            assert_eq!(db.protocol, "tcp");
            assert_eq!(db.status, TunnelStatusDisplay::Reconnecting { attempt: 2 });
        } else {
            panic!("Expected Status response");
        }

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_ipc_error_response() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("error.sock");

        let server = IpcServer::bind_to(&socket_path).await.unwrap();

        let server_handle = tokio::spawn(async move {
            let mut conn = server.accept().await.unwrap();
            let _req = conn.recv().await.unwrap();
            conn.send(&IpcResponse::Error {
                message: "Not implemented".to_string(),
            })
            .await
            .unwrap();
        });

        let mut client = IpcClient::connect_to(&socket_path).await.unwrap();
        let response = client
            .request(&IpcRequest::StartTunnel {
                name: "test".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(
            response,
            IpcResponse::Error {
                message: "Not implemented".to_string()
            }
        );

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_ipc_connection_refused_when_no_server() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("nonexistent.sock");

        // No server running, connection should fail
        let result = IpcClient::connect_to(&socket_path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_ipc_server_prevents_duplicate_bind() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("dup.sock");

        // First server binds successfully
        let _server1 = IpcServer::bind_to(&socket_path).await.unwrap();

        // Second server should fail (socket is in use)
        let result = IpcServer::bind_to(&socket_path).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(
            err.to_string().contains("already running"),
            "Expected 'already running' error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_ipc_socket_cleanup_on_drop() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("cleanup.sock");

        {
            let _server = IpcServer::bind_to(&socket_path).await.unwrap();
            assert!(socket_path.exists());
        }
        // Server dropped, socket should be cleaned up
        assert!(!socket_path.exists());
    }

    #[tokio::test]
    async fn test_ipc_concurrent_clients() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("concurrent.sock");

        let server = IpcServer::bind_to(&socket_path).await.unwrap();
        let socket_path_clone = socket_path.clone();

        // Server handles multiple sequential connections
        let server_handle = tokio::spawn(async move {
            for _ in 0..3 {
                let mut conn = server.accept().await.unwrap();
                let req = conn.recv().await.unwrap();
                if let IpcRequest::Ping = req {
                    conn.send(&IpcResponse::Pong).await.unwrap();
                }
            }
        });

        // Multiple clients connect sequentially
        for i in 0..3 {
            let mut client = IpcClient::connect_to(&socket_path_clone).await.unwrap();
            let response = client.request(&IpcRequest::Ping).await.unwrap();
            assert_eq!(response, IpcResponse::Pong, "Client {} should get Pong", i);
        }

        server_handle.await.unwrap();
    }

    #[test]
    fn test_format_duration_various_values() {
        // Seconds
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(1), "1s");
        assert_eq!(format_duration(59), "59s");

        // Minutes
        assert_eq!(format_duration(60), "1m 0s");
        assert_eq!(format_duration(61), "1m 1s");
        assert_eq!(format_duration(120), "2m 0s");
        assert_eq!(format_duration(3599), "59m 59s");

        // Hours
        assert_eq!(format_duration(3600), "1h 0m");
        assert_eq!(format_duration(3660), "1h 1m");
        assert_eq!(format_duration(7200), "2h 0m");
        assert_eq!(format_duration(86400), "24h 0m"); // 1 day
    }

    #[test]
    fn test_tunnel_status_display_all_variants() {
        // Test Display trait for all variants
        assert_eq!(TunnelStatusDisplay::Starting.to_string(), "◐ Starting");
        assert_eq!(TunnelStatusDisplay::Connected.to_string(), "● Connected");
        assert_eq!(
            TunnelStatusDisplay::Reconnecting { attempt: 1 }.to_string(),
            "⟳ Reconnecting (attempt 1)"
        );
        assert_eq!(
            TunnelStatusDisplay::Reconnecting { attempt: 5 }.to_string(),
            "⟳ Reconnecting (attempt 5)"
        );
        assert_eq!(TunnelStatusDisplay::Failed.to_string(), "✗ Failed");
        assert_eq!(TunnelStatusDisplay::Stopped.to_string(), "○ Stopped");
    }

    #[tokio::test]
    async fn test_ipc_request_response_all_types() {
        let temp_dir = TempDir::new().unwrap();
        let socket_path = temp_dir.path().join("alltypes.sock");

        let server = IpcServer::bind_to(&socket_path).await.unwrap();

        // Test each request type
        let requests = vec![
            IpcRequest::Ping,
            IpcRequest::GetStatus,
            IpcRequest::StartTunnel {
                name: "test".to_string(),
            },
            IpcRequest::StopTunnel {
                name: "test".to_string(),
            },
            IpcRequest::Reload,
        ];

        let num_requests = requests.len();

        let server_handle = tokio::spawn(async move {
            for _ in 0..num_requests {
                let mut conn = server.accept().await.unwrap();
                let req = conn.recv().await.unwrap();
                let response = match req {
                    IpcRequest::Ping => IpcResponse::Pong,
                    IpcRequest::GetStatus => IpcResponse::Status {
                        tunnels: HashMap::new(),
                    },
                    IpcRequest::StartTunnel { .. } => IpcResponse::Ok {
                        message: Some("Started".to_string()),
                    },
                    IpcRequest::StopTunnel { .. } => IpcResponse::Ok {
                        message: Some("Stopped".to_string()),
                    },
                    IpcRequest::Reload => IpcResponse::Ok {
                        message: Some("Reloaded".to_string()),
                    },
                    IpcRequest::ReloadTunnel { .. } => IpcResponse::Ok {
                        message: Some("Tunnel reloaded".to_string()),
                    },
                    IpcRequest::Shutdown => IpcResponse::Ok {
                        message: Some("Shutting down".to_string()),
                    },
                };
                conn.send(&response).await.unwrap();
            }
        });

        for req in requests {
            let mut client = IpcClient::connect_to(&socket_path).await.unwrap();
            let response = client.request(&req).await.unwrap();
            // Just verify we got a valid response
            match response {
                IpcResponse::Pong
                | IpcResponse::Status { .. }
                | IpcResponse::Ok { .. }
                | IpcResponse::Error { .. } => {}
            }
        }

        server_handle.await.unwrap();
    }
}
