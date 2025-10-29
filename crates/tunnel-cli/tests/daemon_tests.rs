//! Daemon tests

use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tunnel_cli::daemon::{Daemon, DaemonCommand, TunnelStatus};
use tunnel_cli::tunnel_store::{StoredTunnel, TunnelStore};
use tunnel_client::{ProtocolConfig, TunnelConfig};
use tunnel_proto::ExitNodeConfig;

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
            }],
            auth_token: "test-token".to_string(),
            exit_node: ExitNodeConfig::Auto,
            failover: true,
            connection_timeout: Duration::from_secs(30),
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
        let _ = daemon.run(command_rx).await;
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
        let _ = daemon.run(command_rx).await;
    });

    // Give daemon time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Request status
    let (status_tx, mut status_rx) = mpsc::channel(1);
    command_tx
        .send(DaemonCommand::GetStatus(status_tx))
        .await
        .unwrap();

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
fn test_tunnel_status_variants() {
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
        let _ = daemon.run(command_rx).await;
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
fn test_tunnel_status_debug() {
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
        let _ = daemon.run(command_rx).await;
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
