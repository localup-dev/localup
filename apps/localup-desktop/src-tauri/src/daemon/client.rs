//! Daemon client for communicating with the daemon service

use localup_lib::HttpMetric;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tracing::info;

use super::protocol::{DaemonRequest, DaemonResponse, TunnelInfo};
use super::{pid_path, socket_path};

/// Default timeout for daemon operations
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Longer timeout for operations that may take time (like starting tunnels)
const LONG_TIMEOUT: Duration = Duration::from_secs(10);

/// Client for communicating with the daemon
pub struct DaemonClient {
    stream: UnixStream,
}

impl DaemonClient {
    /// Connect to the daemon
    pub async fn connect() -> Result<Self, DaemonError> {
        let socket_path = socket_path();

        let stream = UnixStream::connect(&socket_path)
            .await
            .map_err(|e| DaemonError::ConnectionFailed(e.to_string()))?;

        Ok(Self { stream })
    }

    /// Connect to the daemon, starting it if not running
    pub async fn connect_or_start() -> Result<Self, DaemonError> {
        match Self::connect().await {
            Ok(client) => Ok(client),
            Err(_) => {
                // Try to start the daemon
                Self::start_daemon()?;

                // Wait for it to be ready
                for i in 0..50 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    if let Ok(client) = Self::connect().await {
                        info!("Connected to daemon after {} attempts", i + 1);
                        return Ok(client);
                    }
                }

                Err(DaemonError::StartupFailed(
                    "Daemon did not start in time".to_string(),
                ))
            }
        }
    }

    /// Start the daemon process
    pub fn start_daemon() -> Result<Child, DaemonError> {
        // Create the .localup directory if needed
        let socket_path = socket_path();
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DaemonError::StartupFailed(e.to_string()))?;
        }

        // Find the daemon binary
        let daemon_path = Self::find_daemon_binary()?;
        info!("Starting daemon from: {:?}", daemon_path);

        // Set up log file
        let log_dir = super::log_path().parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&log_dir).map_err(|e| DaemonError::StartupFailed(e.to_string()))?;

        let log_file = std::fs::File::create(super::log_path())
            .map_err(|e| DaemonError::StartupFailed(e.to_string()))?;

        let child = Command::new(&daemon_path)
            .stdin(Stdio::null())
            .stdout(log_file.try_clone().unwrap())
            .stderr(log_file)
            .spawn()
            .map_err(|e| DaemonError::StartupFailed(e.to_string()))?;

        Ok(child)
    }

    /// Find the daemon binary
    fn find_daemon_binary() -> Result<std::path::PathBuf, DaemonError> {
        let current_exe =
            std::env::current_exe().map_err(|e| DaemonError::StartupFailed(e.to_string()))?;

        // Get the platform-specific suffix for sidecar binaries
        let target_triple = Self::get_target_triple();

        // Check in same directory as current executable (for bundled app)
        if let Some(dir) = current_exe.parent() {
            // First check for sidecar with platform suffix (Tauri bundled format)
            let sidecar_path = dir.join(format!("localup-daemon-{}", target_triple));
            if sidecar_path.exists() {
                info!("Found sidecar daemon at: {:?}", sidecar_path);
                return Ok(sidecar_path);
            }

            // Then check for plain daemon binary (development mode)
            let daemon_path = dir.join("localup-daemon");
            if daemon_path.exists() {
                info!("Found daemon at: {:?}", daemon_path);
                return Ok(daemon_path);
            }

            // On macOS bundled app, also check Resources directory
            #[cfg(target_os = "macos")]
            {
                if let Some(contents) = dir.parent() {
                    let resources_sidecar = contents
                        .join("Resources")
                        .join(format!("localup-daemon-{}", target_triple));
                    if resources_sidecar.exists() {
                        info!("Found sidecar daemon in Resources: {:?}", resources_sidecar);
                        return Ok(resources_sidecar);
                    }
                }
            }
        }

        // Check in ~/.local/bin
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let local_bin = std::path::PathBuf::from(&home)
            .join(".local")
            .join("bin")
            .join("localup-daemon");
        if local_bin.exists() {
            info!("Found daemon in ~/.local/bin: {:?}", local_bin);
            return Ok(local_bin);
        }

        // Check in PATH
        if let Ok(output) = Command::new("which").arg("localup-daemon").output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    info!("Found daemon in PATH: {}", path);
                    return Ok(std::path::PathBuf::from(path));
                }
            }
        }

        Err(DaemonError::StartupFailed(
            "Could not find localup-daemon binary. Please ensure it's installed.".to_string(),
        ))
    }

    /// Get the target triple for the current platform
    fn get_target_triple() -> &'static str {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            "aarch64-apple-darwin"
        }
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        {
            "x86_64-apple-darwin"
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            "x86_64-unknown-linux-gnu"
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            "aarch64-unknown-linux-gnu"
        }
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            "x86_64-pc-windows-msvc"
        }
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "windows", target_arch = "x86_64"),
        )))]
        {
            "unknown"
        }
    }

    /// Check if the daemon is running
    pub fn is_daemon_running() -> bool {
        let pid_path = pid_path();
        if !pid_path.exists() {
            return false;
        }

        // Read PID and check if process exists
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                return process_exists(pid);
            }
        }

        false
    }

    /// Stop the daemon
    pub async fn stop_daemon() -> Result<(), DaemonError> {
        if let Ok(mut client) = Self::connect().await {
            client.send(DaemonRequest::Shutdown).await?;
        }

        // Remove PID file
        let _ = std::fs::remove_file(pid_path());
        let _ = std::fs::remove_file(socket_path());

        Ok(())
    }

    /// Send a request and get a response with default timeout
    pub async fn send(&mut self, request: DaemonRequest) -> Result<DaemonResponse, DaemonError> {
        self.send_with_timeout(request, DEFAULT_TIMEOUT).await
    }

    /// Send a request and get a response with custom timeout
    pub async fn send_with_timeout(
        &mut self,
        request: DaemonRequest,
        timeout: Duration,
    ) -> Result<DaemonResponse, DaemonError> {
        tokio::time::timeout(timeout, self.send_internal(request))
            .await
            .map_err(|_| DaemonError::Timeout)?
    }

    /// Internal send implementation without timeout
    async fn send_internal(
        &mut self,
        request: DaemonRequest,
    ) -> Result<DaemonResponse, DaemonError> {
        // Serialize request
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| DaemonError::SerializationFailed(e.to_string()))?;

        // Write length prefix
        let len = (request_bytes.len() as u32).to_be_bytes();
        self.stream
            .write_all(&len)
            .await
            .map_err(|e| DaemonError::SendFailed(e.to_string()))?;

        // Write request
        self.stream
            .write_all(&request_bytes)
            .await
            .map_err(|e| DaemonError::SendFailed(e.to_string()))?;

        // Read response length
        let mut len_buf = [0u8; 4];
        self.stream
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| DaemonError::ReceiveFailed(e.to_string()))?;
        let len = u32::from_be_bytes(len_buf) as usize;

        // Read response
        let mut response_buf = vec![0u8; len];
        self.stream
            .read_exact(&mut response_buf)
            .await
            .map_err(|e| DaemonError::ReceiveFailed(e.to_string()))?;

        // Deserialize response
        let response: DaemonResponse = serde_json::from_slice(&response_buf)
            .map_err(|e| DaemonError::DeserializationFailed(e.to_string()))?;

        Ok(response)
    }

    /// Ping the daemon
    pub async fn ping(&mut self) -> Result<(String, u64, usize), DaemonError> {
        match self.send(DaemonRequest::Ping).await? {
            DaemonResponse::Pong {
                version,
                uptime_seconds,
                tunnel_count,
            } => Ok((version, uptime_seconds, tunnel_count)),
            DaemonResponse::Error { message } => Err(DaemonError::ServerError(message)),
            _ => Err(DaemonError::UnexpectedResponse),
        }
    }

    /// List all tunnels
    pub async fn list_tunnels(&mut self) -> Result<Vec<TunnelInfo>, DaemonError> {
        match self.send(DaemonRequest::ListTunnels).await? {
            DaemonResponse::Tunnels(tunnels) => Ok(tunnels),
            DaemonResponse::Error { message } => Err(DaemonError::ServerError(message)),
            _ => Err(DaemonError::UnexpectedResponse),
        }
    }

    /// Get a tunnel by ID
    pub async fn get_tunnel(&mut self, id: &str) -> Result<TunnelInfo, DaemonError> {
        match self
            .send(DaemonRequest::GetTunnel { id: id.to_string() })
            .await?
        {
            DaemonResponse::Tunnel(tunnel) => Ok(tunnel),
            DaemonResponse::Error { message } => Err(DaemonError::ServerError(message)),
            _ => Err(DaemonError::UnexpectedResponse),
        }
    }

    /// Start a tunnel (uses longer timeout as this may take time)
    pub async fn start_tunnel(
        &mut self,
        id: &str,
        name: &str,
        relay_address: &str,
        auth_token: &str,
        local_host: &str,
        local_port: u16,
        protocol: &str,
        subdomain: Option<&str>,
        custom_domain: Option<&str>,
    ) -> Result<TunnelInfo, DaemonError> {
        match self
            .send_with_timeout(
                DaemonRequest::StartTunnel {
                    id: id.to_string(),
                    name: name.to_string(),
                    relay_address: relay_address.to_string(),
                    auth_token: auth_token.to_string(),
                    local_host: local_host.to_string(),
                    local_port,
                    protocol: protocol.to_string(),
                    subdomain: subdomain.map(|s| s.to_string()),
                    custom_domain: custom_domain.map(|s| s.to_string()),
                },
                LONG_TIMEOUT,
            )
            .await?
        {
            DaemonResponse::Tunnel(tunnel) => Ok(tunnel),
            DaemonResponse::Error { message } => Err(DaemonError::ServerError(message)),
            _ => Err(DaemonError::UnexpectedResponse),
        }
    }

    /// Stop a tunnel
    pub async fn stop_tunnel(&mut self, id: &str) -> Result<(), DaemonError> {
        match self
            .send(DaemonRequest::StopTunnel { id: id.to_string() })
            .await?
        {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => Err(DaemonError::ServerError(message)),
            _ => Err(DaemonError::UnexpectedResponse),
        }
    }

    /// Delete a tunnel
    pub async fn delete_tunnel(&mut self, id: &str) -> Result<(), DaemonError> {
        match self
            .send(DaemonRequest::DeleteTunnel { id: id.to_string() })
            .await?
        {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => Err(DaemonError::ServerError(message)),
            _ => Err(DaemonError::UnexpectedResponse),
        }
    }

    /// Get metrics for a tunnel with pagination
    pub async fn get_tunnel_metrics(
        &mut self,
        id: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<(Vec<HttpMetric>, usize), DaemonError> {
        match self
            .send(DaemonRequest::GetTunnelMetrics {
                id: id.to_string(),
                offset,
                limit,
            })
            .await?
        {
            DaemonResponse::Metrics { items, total, .. } => Ok((items, total)),
            DaemonResponse::Error { message } => Err(DaemonError::ServerError(message)),
            _ => Err(DaemonError::UnexpectedResponse),
        }
    }

    /// Clear metrics for a tunnel
    pub async fn clear_tunnel_metrics(&mut self, id: &str) -> Result<(), DaemonError> {
        match self
            .send(DaemonRequest::ClearTunnelMetrics { id: id.to_string() })
            .await?
        {
            DaemonResponse::Ok => Ok(()),
            DaemonResponse::Error { message } => Err(DaemonError::ServerError(message)),
            _ => Err(DaemonError::UnexpectedResponse),
        }
    }

    /// Subscribe to metrics for a tunnel
    /// Returns a subscription that can be used to receive streaming events
    pub async fn subscribe_metrics(self, id: &str) -> Result<MetricsSubscription, DaemonError> {
        MetricsSubscription::new(self.stream, id.to_string()).await
    }
}

/// Subscription to tunnel metrics events
pub struct MetricsSubscription {
    stream: UnixStream,
    tunnel_id: String,
}

impl MetricsSubscription {
    /// Create a new metrics subscription
    async fn new(mut stream: UnixStream, tunnel_id: String) -> Result<Self, DaemonError> {
        // Send subscribe request
        let request = DaemonRequest::SubscribeMetrics {
            id: tunnel_id.clone(),
        };
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| DaemonError::SerializationFailed(e.to_string()))?;

        // Write length prefix
        let len = (request_bytes.len() as u32).to_be_bytes();
        stream
            .write_all(&len)
            .await
            .map_err(|e| DaemonError::SendFailed(e.to_string()))?;

        // Write request
        stream
            .write_all(&request_bytes)
            .await
            .map_err(|e| DaemonError::SendFailed(e.to_string()))?;

        // Read response
        let mut len_buf = [0u8; 4];
        stream
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| DaemonError::ReceiveFailed(e.to_string()))?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut response_buf = vec![0u8; len];
        stream
            .read_exact(&mut response_buf)
            .await
            .map_err(|e| DaemonError::ReceiveFailed(e.to_string()))?;

        let response: DaemonResponse = serde_json::from_slice(&response_buf)
            .map_err(|e| DaemonError::DeserializationFailed(e.to_string()))?;

        match response {
            DaemonResponse::Subscribed { id } => {
                info!("Subscribed to metrics for tunnel: {}", id);
                Ok(Self { stream, tunnel_id })
            }
            DaemonResponse::Error { message } => Err(DaemonError::ServerError(message)),
            _ => Err(DaemonError::UnexpectedResponse),
        }
    }

    /// Get the tunnel ID this subscription is for
    pub fn tunnel_id(&self) -> &str {
        &self.tunnel_id
    }

    /// Receive the next metrics event
    /// Returns None if the subscription ended
    pub async fn recv(&mut self) -> Option<localup_lib::MetricsEvent> {
        // Read response length
        let mut len_buf = [0u8; 4];
        if self.stream.read_exact(&mut len_buf).await.is_err() {
            return None;
        }
        let len = u32::from_be_bytes(len_buf) as usize;

        // Read response
        let mut response_buf = vec![0u8; len];
        if self.stream.read_exact(&mut response_buf).await.is_err() {
            return None;
        }

        // Deserialize response
        let response: DaemonResponse = match serde_json::from_slice(&response_buf) {
            Ok(r) => r,
            Err(_) => return None,
        };

        match response {
            DaemonResponse::MetricsEvent { event, .. } => Some(event),
            DaemonResponse::Ok => {
                // Unsubscribe confirmed
                None
            }
            _ => None,
        }
    }

    /// Unsubscribe from metrics
    pub async fn unsubscribe(mut self) -> Result<(), DaemonError> {
        let request = DaemonRequest::UnsubscribeMetrics {
            id: self.tunnel_id.clone(),
        };
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| DaemonError::SerializationFailed(e.to_string()))?;

        // Write length prefix
        let len = (request_bytes.len() as u32).to_be_bytes();
        self.stream
            .write_all(&len)
            .await
            .map_err(|e| DaemonError::SendFailed(e.to_string()))?;

        // Write request
        self.stream
            .write_all(&request_bytes)
            .await
            .map_err(|e| DaemonError::SendFailed(e.to_string()))?;

        // Read response (but don't wait forever)
        let result = tokio::time::timeout(Duration::from_secs(2), async {
            let mut len_buf = [0u8; 4];
            self.stream.read_exact(&mut len_buf).await?;
            let len = u32::from_be_bytes(len_buf) as usize;

            let mut response_buf = vec![0u8; len];
            self.stream.read_exact(&mut response_buf).await?;
            Ok::<_, std::io::Error>(response_buf)
        })
        .await;

        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(DaemonError::ReceiveFailed(e.to_string())),
            Err(_) => Ok(()), // Timeout is fine, we're closing anyway
        }
    }
}

/// Check if a process exists
fn process_exists(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        // On Windows, use OpenProcess to check
        use std::ptr::null_mut;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle != null_mut() {
                CloseHandle(handle);
                return true;
            }
        }
        false
    }

    #[cfg(not(target_os = "windows"))]
    {
        // On Unix, use kill with signal 0 to check
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
}

/// Daemon client errors
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Daemon startup failed: {0}")]
    StartupFailed(String),

    #[error("Serialization failed: {0}")]
    SerializationFailed(String),

    #[error("Deserialization failed: {0}")]
    DeserializationFailed(String),

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Receive failed: {0}")]
    ReceiveFailed(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Unexpected response")]
    UnexpectedResponse,

    #[error("Request timed out")]
    Timeout,
}
