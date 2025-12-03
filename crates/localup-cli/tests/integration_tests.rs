//! Integration tests for tunnel CLI

use localup_cli::localup_store::{StoredTunnel, TunnelStore};
use localup_client::{ProtocolConfig, TunnelConfig};
use localup_proto::{ExitNodeConfig, HttpAuthConfig};
use std::time::Duration;
use tempfile::TempDir;

/// Create a test tunnel configuration
fn create_test_config(name: &str, port: u16) -> StoredTunnel {
    StoredTunnel {
        name: name.to_string(),
        enabled: true,
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
            preferred_transport: None,
            http_auth: HttpAuthConfig::None,
        },
    }
}

/// Create a test store with temporary directory
fn create_test_store() -> (TunnelStore, TempDir) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    let test_id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = TempDir::new().unwrap();

    // Create unique home directory for this test
    let home_dir = temp_dir.path().join(format!("home-{}", test_id));
    std::fs::create_dir_all(&home_dir).unwrap();

    // Save current HOME and set to test directory
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &home_dir);

    let store = TunnelStore::new().unwrap();

    // Restore old HOME if it existed
    if let Some(old) = old_home {
        std::env::set_var("HOME", old);
    }

    (store, temp_dir)
}

#[test]
fn test_localup_store_create() {
    let (store, _temp) = create_test_store();
    assert!(store.base_dir().exists());
}

#[test]
fn test_localup_store_save_and_load() {
    let (store, _temp) = create_test_store();
    let tunnel = create_test_config("myapp", 3000);

    // Save
    store.save(&tunnel).unwrap();

    // Load
    let loaded = store.load("myapp").unwrap();
    assert_eq!(loaded.name, "myapp");
    assert!(loaded.enabled);
    assert_eq!(loaded.config.protocols.len(), 1);
}

#[test]
fn test_localup_store_list_empty() {
    let (store, _temp) = create_test_store();
    let tunnels = store.list().unwrap();
    assert_eq!(tunnels.len(), 0);
}

#[test]
fn test_localup_store_list_multiple() {
    let (store, _temp) = create_test_store();

    // Add multiple tunnels
    store.save(&create_test_config("app1", 3000)).unwrap();
    store.save(&create_test_config("app2", 3001)).unwrap();
    store.save(&create_test_config("app3", 3002)).unwrap();

    // List all
    let tunnels = store.list().unwrap();
    assert_eq!(tunnels.len(), 3);

    // Verify sorted by name
    assert_eq!(tunnels[0].name, "app1");
    assert_eq!(tunnels[1].name, "app2");
    assert_eq!(tunnels[2].name, "app3");
}

#[test]
fn test_localup_store_list_enabled() {
    let (store, _temp) = create_test_store();

    // Add enabled tunnel
    let mut tunnel1 = create_test_config("enabled1", 3000);
    tunnel1.enabled = true;
    store.save(&tunnel1).unwrap();

    // Add disabled tunnel
    let mut tunnel2 = create_test_config("disabled1", 3001);
    tunnel2.enabled = false;
    store.save(&tunnel2).unwrap();

    // Add another enabled tunnel
    let mut tunnel3 = create_test_config("enabled2", 3002);
    tunnel3.enabled = true;
    store.save(&tunnel3).unwrap();

    // List enabled only
    let enabled = store.list_enabled().unwrap();
    assert_eq!(enabled.len(), 2);
    assert_eq!(enabled[0].name, "enabled1");
    assert_eq!(enabled[1].name, "enabled2");
}

#[test]
fn test_localup_store_enable_disable() {
    let (store, _temp) = create_test_store();

    // Create disabled tunnel
    let mut tunnel = create_test_config("myapp", 3000);
    tunnel.enabled = false;
    store.save(&tunnel).unwrap();

    // Verify disabled
    let loaded = store.load("myapp").unwrap();
    assert!(!loaded.enabled);

    // Enable
    store.enable("myapp").unwrap();
    let loaded = store.load("myapp").unwrap();
    assert!(loaded.enabled);

    // Disable
    store.disable("myapp").unwrap();
    let loaded = store.load("myapp").unwrap();
    assert!(!loaded.enabled);
}

#[test]
fn test_localup_store_remove() {
    let (store, _temp) = create_test_store();
    let tunnel = create_test_config("myapp", 3000);

    // Save
    store.save(&tunnel).unwrap();
    assert!(store.exists("myapp"));

    // Remove
    store.remove("myapp").unwrap();
    assert!(!store.exists("myapp"));

    // Verify load fails
    assert!(store.load("myapp").is_err());
}

#[test]
fn test_localup_store_remove_nonexistent() {
    let (store, _temp) = create_test_store();

    // Try to remove non-existent tunnel
    let result = store.remove("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_localup_store_update() {
    let (store, _temp) = create_test_store();

    // Save initial version
    let tunnel = create_test_config("myapp", 3000);
    store.save(&tunnel).unwrap();

    // Load and verify
    let loaded = store.load("myapp").unwrap();
    assert_eq!(loaded.config.protocols[0].local_port(), 3000);

    // Update with different port
    let updated = create_test_config("myapp", 8080);
    store.save(&updated).unwrap();

    // Verify updated
    let loaded = store.load("myapp").unwrap();
    assert_eq!(loaded.config.protocols[0].local_port(), 8080);
}

#[test]
fn test_localup_store_invalid_names() {
    let (store, _temp) = create_test_store();

    // Invalid name with slash
    let mut tunnel = create_test_config("app/bad", 3000);
    tunnel.name = "app/bad".to_string();
    assert!(store.save(&tunnel).is_err());

    // Invalid name with dots
    let mut tunnel = create_test_config("app..bad", 3000);
    tunnel.name = "app..bad".to_string();
    assert!(store.save(&tunnel).is_err());

    // Empty name
    let mut tunnel = create_test_config("", 3000);
    tunnel.name = "".to_string();
    assert!(store.save(&tunnel).is_err());
}

#[test]
fn test_localup_store_valid_names() {
    let (store, _temp) = create_test_store();

    // Alphanumeric
    let tunnel = create_test_config("myapp123", 3000);
    assert!(store.save(&tunnel).is_ok());

    // With hyphens
    let tunnel = create_test_config("my-app", 3001);
    assert!(store.save(&tunnel).is_ok());

    // With underscores
    let tunnel = create_test_config("my_app", 3002);
    assert!(store.save(&tunnel).is_ok());

    // Mixed
    let tunnel = create_test_config("my-app_123", 3003);
    assert!(store.save(&tunnel).is_ok());

    // Verify all saved
    let tunnels = store.list().unwrap();
    assert_eq!(tunnels.len(), 4);
}

#[test]
fn test_localup_store_protocol_types() {
    let (store, _temp) = create_test_store();

    // HTTP protocol
    let http_tunnel = StoredTunnel {
        name: "http-test".to_string(),
        enabled: true,
        config: TunnelConfig {
            local_host: "localhost".to_string(),
            protocols: vec![ProtocolConfig::Http {
                local_port: 3000,
                subdomain: Some("test".to_string()),
            }],
            auth_token: "test-token".to_string(),
            exit_node: ExitNodeConfig::Auto,
            failover: true,
            connection_timeout: Duration::from_secs(30),
            preferred_transport: None,
            http_auth: HttpAuthConfig::None,
        },
    };
    store.save(&http_tunnel).unwrap();

    // HTTPS protocol
    let https_tunnel = StoredTunnel {
        name: "https-test".to_string(),
        enabled: true,
        config: TunnelConfig {
            local_host: "localhost".to_string(),
            protocols: vec![ProtocolConfig::Https {
                local_port: 3000,
                subdomain: Some("test".to_string()),
            }],
            auth_token: "test-token".to_string(),
            exit_node: ExitNodeConfig::Auto,
            failover: true,
            connection_timeout: Duration::from_secs(30),
            preferred_transport: None,
            http_auth: HttpAuthConfig::None,
        },
    };
    store.save(&https_tunnel).unwrap();

    // TCP protocol
    let tcp_tunnel = StoredTunnel {
        name: "tcp-test".to_string(),
        enabled: true,
        config: TunnelConfig {
            local_host: "localhost".to_string(),
            protocols: vec![ProtocolConfig::Tcp {
                local_port: 5432,
                remote_port: Some(5432),
            }],
            auth_token: "test-token".to_string(),
            exit_node: ExitNodeConfig::Auto,
            failover: true,
            connection_timeout: Duration::from_secs(30),
            preferred_transport: None,
            http_auth: HttpAuthConfig::None,
        },
    };
    store.save(&tcp_tunnel).unwrap();

    // TLS protocol
    let tls_tunnel = StoredTunnel {
        name: "tls-test".to_string(),
        enabled: true,
        config: TunnelConfig {
            local_host: "localhost".to_string(),
            protocols: vec![ProtocolConfig::Tls {
                local_port: 9000,
                sni_hostname: Some("tls-test.example.com".to_string()),
            }],
            auth_token: "test-token".to_string(),
            exit_node: ExitNodeConfig::Auto,
            failover: true,
            connection_timeout: Duration::from_secs(30),
            preferred_transport: None,
            http_auth: HttpAuthConfig::None,
        },
    };
    store.save(&tls_tunnel).unwrap();

    // Verify all protocols saved correctly
    let tunnels = store.list().unwrap();
    assert_eq!(tunnels.len(), 4);

    // Verify each can be loaded
    let http = store.load("http-test").unwrap();
    assert!(matches!(
        http.config.protocols[0],
        ProtocolConfig::Http { .. }
    ));

    let https = store.load("https-test").unwrap();
    assert!(matches!(
        https.config.protocols[0],
        ProtocolConfig::Https { .. }
    ));

    let tcp = store.load("tcp-test").unwrap();
    assert!(matches!(
        tcp.config.protocols[0],
        ProtocolConfig::Tcp { .. }
    ));

    let tls = store.load("tls-test").unwrap();
    assert!(matches!(
        tls.config.protocols[0],
        ProtocolConfig::Tls { .. }
    ));
}

#[test]
fn test_localup_store_exit_node_configs() {
    let (store, _temp) = create_test_store();

    // Auto exit node
    let auto_tunnel = StoredTunnel {
        name: "auto".to_string(),
        enabled: true,
        config: TunnelConfig {
            local_host: "localhost".to_string(),
            protocols: vec![ProtocolConfig::Http {
                local_port: 3000,
                subdomain: None,
            }],
            auth_token: "test-token".to_string(),
            exit_node: ExitNodeConfig::Auto,
            failover: true,
            connection_timeout: Duration::from_secs(30),
            preferred_transport: None,
            http_auth: HttpAuthConfig::None,
        },
    };
    store.save(&auto_tunnel).unwrap();

    // Custom relay
    let custom_tunnel = StoredTunnel {
        name: "custom".to_string(),
        enabled: true,
        config: TunnelConfig {
            local_host: "localhost".to_string(),
            protocols: vec![ProtocolConfig::Http {
                local_port: 3000,
                subdomain: None,
            }],
            auth_token: "test-token".to_string(),
            exit_node: ExitNodeConfig::Custom("relay.example.com:8080".to_string()),
            failover: true,
            connection_timeout: Duration::from_secs(30),
            preferred_transport: None,
            http_auth: HttpAuthConfig::None,
        },
    };
    store.save(&custom_tunnel).unwrap();

    // Verify both saved correctly
    let auto = store.load("auto").unwrap();
    assert!(matches!(auto.config.exit_node, ExitNodeConfig::Auto));

    let custom = store.load("custom").unwrap();
    assert!(matches!(custom.config.exit_node, ExitNodeConfig::Custom(_)));
}

#[test]
fn test_localup_store_serialization_roundtrip() {
    let (store, _temp) = create_test_store();

    // Create a complex configuration
    let tunnel = StoredTunnel {
        name: "complex".to_string(),
        enabled: false,
        config: TunnelConfig {
            local_host: "127.0.0.1".to_string(),
            protocols: vec![ProtocolConfig::Https {
                local_port: 8443,
                subdomain: Some("complex-app".to_string()),
            }],
            auth_token: "very-secret-token-12345".to_string(),
            exit_node: ExitNodeConfig::Custom("custom-relay.example.com:9999".to_string()),
            failover: false,
            connection_timeout: Duration::from_secs(60),
            preferred_transport: None,
            http_auth: HttpAuthConfig::None,
        },
    };

    // Save
    store.save(&tunnel).unwrap();

    // Load
    let loaded = store.load("complex").unwrap();

    // Verify all fields
    assert_eq!(loaded.name, "complex");
    assert!(!loaded.enabled);
    assert_eq!(loaded.config.local_host, "127.0.0.1");
    assert_eq!(loaded.config.protocols.len(), 1);
    assert_eq!(loaded.config.auth_token, "very-secret-token-12345");
    assert!(!loaded.config.failover);
    assert_eq!(loaded.config.connection_timeout, Duration::from_secs(60));

    // Verify protocol details
    match &loaded.config.protocols[0] {
        ProtocolConfig::Https {
            local_port,
            subdomain,
        } => {
            assert_eq!(*local_port, 8443);
            assert_eq!(subdomain.as_deref(), Some("complex-app"));
        }
        _ => panic!("Expected HTTPS protocol"),
    }

    // Verify exit node
    match &loaded.config.exit_node {
        ExitNodeConfig::Custom(addr) => {
            assert_eq!(addr, "custom-relay.example.com:9999");
        }
        _ => panic!("Expected custom exit node"),
    }
}

// Helper trait to get local_port from ProtocolConfig
trait ProtocolConfigExt {
    fn local_port(&self) -> u16;
}

impl ProtocolConfigExt for ProtocolConfig {
    fn local_port(&self) -> u16 {
        match self {
            ProtocolConfig::Http { local_port, .. } => *local_port,
            ProtocolConfig::Https { local_port, .. } => *local_port,
            ProtocolConfig::Tcp { local_port, .. } => *local_port,
            ProtocolConfig::Tls { local_port, .. } => *local_port,
        }
    }
}
