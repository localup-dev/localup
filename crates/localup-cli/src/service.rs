//! Service installation for macOS (launchd) and Linux (systemd)

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Service manager for platform-specific service installation
pub struct ServiceManager {
    platform: Platform,
}

/// Platform type
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)] // Platform variants are conditionally used based on target OS
enum Platform {
    MacOS,
    Linux,
    Unsupported,
}

impl ServiceManager {
    /// Create a new service manager
    pub fn new() -> Self {
        let platform = Self::detect_platform();
        Self { platform }
    }

    /// Detect the current platform
    fn detect_platform() -> Platform {
        #[cfg(target_os = "macos")]
        {
            Platform::MacOS
        }
        #[cfg(target_os = "linux")]
        {
            Platform::Linux
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Platform::Unsupported
        }
    }

    /// Check if the current platform is supported
    pub fn is_supported(&self) -> bool {
        self.platform != Platform::Unsupported
    }

    /// Get the service name
    fn service_name(&self) -> &str {
        match self.platform {
            Platform::MacOS => "com.localup.daemon",
            Platform::Linux => "localup",
            Platform::Unsupported => "localup",
        }
    }

    /// Get the binary path (current executable)
    fn get_binary_path() -> Result<PathBuf> {
        std::env::current_exe().context("Failed to get current executable path")
    }

    /// Install the service
    pub fn install(&self) -> Result<()> {
        if !self.is_supported() {
            anyhow::bail!("Service installation is not supported on this platform");
        }

        let binary_path = Self::get_binary_path()?;

        match self.platform {
            Platform::MacOS => self.install_macos(&binary_path),
            Platform::Linux => self.install_linux(&binary_path),
            Platform::Unsupported => unreachable!(),
        }
    }

    /// Uninstall the service
    pub fn uninstall(&self) -> Result<()> {
        if !self.is_supported() {
            anyhow::bail!("Service uninstall is not supported on this platform");
        }

        match self.platform {
            Platform::MacOS => self.uninstall_macos(),
            Platform::Linux => self.uninstall_linux(),
            Platform::Unsupported => unreachable!(),
        }
    }

    /// Start the service
    pub fn start(&self) -> Result<()> {
        if !self.is_supported() {
            anyhow::bail!("Service start is not supported on this platform");
        }

        match self.platform {
            Platform::MacOS => self.start_macos(),
            Platform::Linux => self.start_linux(),
            Platform::Unsupported => unreachable!(),
        }
    }

    /// Stop the service
    pub fn stop(&self) -> Result<()> {
        if !self.is_supported() {
            anyhow::bail!("Service stop is not supported on this platform");
        }

        match self.platform {
            Platform::MacOS => self.stop_macos(),
            Platform::Linux => self.stop_linux(),
            Platform::Unsupported => unreachable!(),
        }
    }

    /// Restart the service
    pub fn restart(&self) -> Result<()> {
        self.stop().ok(); // Ignore error if not running
        self.start()
    }

    /// Get service status
    pub fn status(&self) -> Result<ServiceStatus> {
        if !self.is_supported() {
            return Ok(ServiceStatus::NotInstalled);
        }

        match self.platform {
            Platform::MacOS => self.status_macos(),
            Platform::Linux => self.status_linux(),
            Platform::Unsupported => Ok(ServiceStatus::NotInstalled),
        }
    }

    /// Get service logs
    pub fn logs(&self, lines: usize) -> Result<String> {
        if !self.is_supported() {
            anyhow::bail!("Service logs are not supported on this platform");
        }

        match self.platform {
            Platform::MacOS => self.logs_macos(lines),
            Platform::Linux => self.logs_linux(lines),
            Platform::Unsupported => unreachable!(),
        }
    }

    // ============ macOS (launchd) implementation ============

    fn get_launchd_plist_path(&self) -> Result<PathBuf> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        Ok(home
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{}.plist", self.service_name())))
    }

    fn install_macos(&self, binary_path: &Path) -> Result<()> {
        let plist_path = self.get_launchd_plist_path()?;
        let plist_dir = plist_path
            .parent()
            .context("Failed to get parent directory")?;

        // Create LaunchAgents directory if it doesn't exist
        fs::create_dir_all(plist_dir).context("Failed to create LaunchAgents directory")?;

        // Generate plist content
        let plist_content = self.generate_launchd_plist(binary_path)?;

        // Write plist file
        fs::write(&plist_path, plist_content)
            .context(format!("Failed to write plist file: {:?}", plist_path))?;

        println!("✅ Service installed: {}", plist_path.display());
        println!("   Start with: localup service start");

        Ok(())
    }

    fn generate_launchd_plist(&self, binary_path: &Path) -> Result<String> {
        let log_dir = dirs::home_dir()
            .context("Failed to get home directory")?
            .join(".localup")
            .join("logs");

        fs::create_dir_all(&log_dir).context("Failed to create log directory")?;

        let stdout_log = log_dir.join("daemon.log");
        let stderr_log = log_dir.join("daemon.error.log");

        Ok(format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{service_name}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary_path}</string>
        <string>daemon</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{stdout_log}</string>
    <key>StandardErrorPath</key>
    <string>{stderr_log}</string>
    <key>WorkingDirectory</key>
    <string>{home}</string>
</dict>
</plist>
"#,
            service_name = self.service_name(),
            binary_path = binary_path.display(),
            stdout_log = stdout_log.display(),
            stderr_log = stderr_log.display(),
            home = dirs::home_dir()
                .context("Failed to get home directory")?
                .display(),
        ))
    }

    fn uninstall_macos(&self) -> Result<()> {
        // Stop the service first (ignore errors)
        self.stop_macos().ok();

        let plist_path = self.get_launchd_plist_path()?;

        if plist_path.exists() {
            fs::remove_file(&plist_path)
                .context(format!("Failed to remove plist file: {:?}", plist_path))?;
            println!("✅ Service uninstalled");
        } else {
            println!("Service is not installed");
        }

        Ok(())
    }

    fn start_macos(&self) -> Result<()> {
        let plist_path = self.get_launchd_plist_path()?;

        if !plist_path.exists() {
            anyhow::bail!("Service is not installed. Run 'localup service install' first.");
        }

        let output = Command::new("launchctl")
            .arg("load")
            .arg(&plist_path)
            .output()
            .context("Failed to execute launchctl")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "service already loaded" errors
            if !stderr.contains("Already loaded") {
                anyhow::bail!("Failed to start service: {}", stderr);
            }
        }

        println!("✅ Service started");
        Ok(())
    }

    fn stop_macos(&self) -> Result<()> {
        let plist_path = self.get_launchd_plist_path()?;

        if !plist_path.exists() {
            anyhow::bail!("Service is not installed");
        }

        let output = Command::new("launchctl")
            .arg("unload")
            .arg(&plist_path)
            .output()
            .context("Failed to execute launchctl")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to stop service: {}", stderr);
        }

        println!("✅ Service stopped");
        Ok(())
    }

    fn status_macos(&self) -> Result<ServiceStatus> {
        let plist_path = self.get_launchd_plist_path()?;

        if !plist_path.exists() {
            return Ok(ServiceStatus::NotInstalled);
        }

        let output = Command::new("launchctl")
            .arg("list")
            .arg(self.service_name())
            .output()
            .context("Failed to execute launchctl")?;

        if output.status.success() {
            Ok(ServiceStatus::Running)
        } else {
            Ok(ServiceStatus::Stopped)
        }
    }

    fn logs_macos(&self, lines: usize) -> Result<String> {
        let log_file = dirs::home_dir()
            .context("Failed to get home directory")?
            .join(".localup")
            .join("logs")
            .join("daemon.log");

        if !log_file.exists() {
            return Ok("No logs available".to_string());
        }

        let output = Command::new("tail")
            .arg("-n")
            .arg(lines.to_string())
            .arg(&log_file)
            .output()
            .context("Failed to read logs")?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    // ============ Linux (systemd) implementation ============

    fn get_systemd_unit_path(&self) -> Result<PathBuf> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        Ok(home
            .join(".config")
            .join("systemd")
            .join("user")
            .join(format!("{}.service", self.service_name())))
    }

    fn install_linux(&self, binary_path: &Path) -> Result<()> {
        let unit_path = self.get_systemd_unit_path()?;
        let unit_dir = unit_path
            .parent()
            .context("Failed to get parent directory")?;

        // Create systemd user directory if it doesn't exist
        fs::create_dir_all(unit_dir).context("Failed to create systemd user directory")?;

        // Generate unit file content
        let unit_content = self.generate_systemd_unit(binary_path)?;

        // Write unit file
        fs::write(&unit_path, unit_content)
            .context(format!("Failed to write unit file: {:?}", unit_path))?;

        // Reload systemd daemon
        Command::new("systemctl")
            .arg("--user")
            .arg("daemon-reload")
            .output()
            .context("Failed to reload systemd daemon")?;

        println!("✅ Service installed: {}", unit_path.display());
        println!("   Start with: localup service start");

        Ok(())
    }

    fn generate_systemd_unit(&self, binary_path: &Path) -> Result<String> {
        Ok(format!(
            r#"[Unit]
Description=Localup Tunnel Daemon
After=network.target

[Service]
Type=simple
ExecStart={binary_path} daemon start
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=default.target
"#,
            binary_path = binary_path.display(),
        ))
    }

    fn uninstall_linux(&self) -> Result<()> {
        // Stop and disable the service first (ignore errors)
        self.stop_linux().ok();

        let unit_path = self.get_systemd_unit_path()?;

        if unit_path.exists() {
            fs::remove_file(&unit_path)
                .context(format!("Failed to remove unit file: {:?}", unit_path))?;

            // Reload systemd daemon
            Command::new("systemctl")
                .arg("--user")
                .arg("daemon-reload")
                .output()
                .context("Failed to reload systemd daemon")?;

            println!("✅ Service uninstalled");
        } else {
            println!("Service is not installed");
        }

        Ok(())
    }

    fn start_linux(&self) -> Result<()> {
        let unit_path = self.get_systemd_unit_path()?;

        if !unit_path.exists() {
            anyhow::bail!("Service is not installed. Run 'localup service install' first.");
        }

        let output = Command::new("systemctl")
            .arg("--user")
            .arg("start")
            .arg(self.service_name())
            .output()
            .context("Failed to execute systemctl")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to start service: {}", stderr);
        }

        // Enable for auto-start
        Command::new("systemctl")
            .arg("--user")
            .arg("enable")
            .arg(self.service_name())
            .output()
            .ok();

        println!("✅ Service started");
        Ok(())
    }

    fn stop_linux(&self) -> Result<()> {
        let output = Command::new("systemctl")
            .arg("--user")
            .arg("stop")
            .arg(self.service_name())
            .output()
            .context("Failed to execute systemctl")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to stop service: {}", stderr);
        }

        println!("✅ Service stopped");
        Ok(())
    }

    fn status_linux(&self) -> Result<ServiceStatus> {
        let unit_path = self.get_systemd_unit_path()?;

        if !unit_path.exists() {
            return Ok(ServiceStatus::NotInstalled);
        }

        let output = Command::new("systemctl")
            .arg("--user")
            .arg("is-active")
            .arg(self.service_name())
            .output()
            .context("Failed to execute systemctl")?;

        let status_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

        match status_str.as_str() {
            "active" => Ok(ServiceStatus::Running),
            _ => Ok(ServiceStatus::Stopped),
        }
    }

    fn logs_linux(&self, lines: usize) -> Result<String> {
        let output = Command::new("journalctl")
            .arg("--user")
            .arg("-u")
            .arg(self.service_name())
            .arg("-n")
            .arg(lines.to_string())
            .arg("--no-pager")
            .output()
            .context("Failed to read logs")?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// Service status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ServiceStatus {
    Running,
    Stopped,
    NotInstalled,
}

impl std::fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceStatus::Running => write!(f, "Running ✅"),
            ServiceStatus::Stopped => write!(f, "Stopped"),
            ServiceStatus::NotInstalled => write!(f, "Not installed"),
        }
    }
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}
