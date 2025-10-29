//! Service manager tests

use tunnel_cli::service::{ServiceManager, ServiceStatus};

#[test]
fn test_service_manager_creation() {
    let manager = ServiceManager::new();

    // Platform detection should work
    #[cfg(target_os = "macos")]
    assert!(manager.is_supported());

    #[cfg(target_os = "linux")]
    assert!(manager.is_supported());

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    assert!(!manager.is_supported());
}

#[test]
fn test_service_status_display() {
    assert_eq!(ServiceStatus::Running.to_string(), "Running ✅");
    assert_eq!(ServiceStatus::Stopped.to_string(), "Stopped");
    assert_eq!(ServiceStatus::NotInstalled.to_string(), "Not installed");
}

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn test_service_initial_status() {
    let manager = ServiceManager::new();

    // On a fresh system, service should not be installed
    // (This test assumes the service is not currently installed)
    let status = manager.status();

    // Should not panic
    assert!(status.is_ok());

    // Status should be either NotInstalled or Stopped
    let status = status.unwrap();
    assert!(
        matches!(status, ServiceStatus::NotInstalled | ServiceStatus::Stopped),
        "Expected NotInstalled or Stopped, got {:?}",
        status
    );
}

#[test]
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn test_service_unsupported_platform() {
    let manager = ServiceManager::new();

    assert!(!manager.is_supported());

    // All operations should fail on unsupported platforms
    assert!(manager.install().is_err());
    assert!(manager.uninstall().is_err());
    assert!(manager.start().is_err());
    assert!(manager.stop().is_err());
    assert!(manager.restart().is_err());
    assert!(manager.logs(10).is_err());
}

// Note: We don't test actual install/uninstall/start/stop operations here
// because they require root/sudo privileges and would affect the actual system.
// These should be tested manually or in a containerized environment.

#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn test_service_restart_when_not_installed() {
    let manager = ServiceManager::new();

    // Ensure service is not installed (don't check result, might already be uninstalled)
    let _ = manager.uninstall();

    // Restart should fail gracefully when not installed
    let result = manager.restart();

    // Should either error or succeed (stop might fail, start should fail)
    // We're just checking it doesn't panic
    let _ = result;
}

#[test]
fn test_service_manager_default() {
    let _manager = ServiceManager::default();
    // Should create without error (test passes if no panic)
}
