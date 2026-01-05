//! IPC (Inter-Process Communication) module for daemon-CLI communication
//!
//! Uses Unix domain sockets on Unix platforms and named pipes on Windows
//! for local IPC. The daemon listens on a socket/pipe and the CLI connects
//! to query status or send commands.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// Platform-specific imports
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

#[cfg(windows)]
use tokio::net::{TcpListener, TcpStream};

/// IPC request from CLI to daemon
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcRequest {
    /// Get status of all tunnels
    GetStatus,

    /// Start a specific tunnel by name
    StartTunnel { name: String },

    /// Stop a specific tunnel by name
    StopTunnel { name: String },

    /// Reload a specific tunnel (stop + start with new config)
    ReloadTunnel { name: String },

    /// Ping to check if daemon is alive
    Ping,

    /// Trigger configuration reload (all tunnels)
    Reload,

    /// Shutdown the daemon (stops all tunnels and exits)
    Shutdown,
}

/// IPC response from daemon to CLI
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcResponse {
    /// Status response with all tunnel information
    Status {
        tunnels: HashMap<String, TunnelStatusInfo>,
    },

    /// Success acknowledgment
    Ok { message: Option<String> },

    /// Error response
    Error { message: String },

    /// Pong response to ping
    Pong,
}

/// Detailed tunnel status for display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TunnelStatusInfo {
    /// Tunnel name
    pub name: String,

    /// Protocol type (http, https, tcp, tls)
    pub protocol: String,

    /// Local port being forwarded
    pub local_port: u16,

    /// Public URL if connected
    pub public_url: Option<String>,

    /// Current status
    pub status: TunnelStatusDisplay,

    /// Uptime in seconds if connected
    pub uptime_seconds: Option<u64>,

    /// Last error message if failed
    pub last_error: Option<String>,
}

/// Display-friendly status enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TunnelStatusDisplay {
    /// Tunnel is starting up
    Starting,

    /// Tunnel is connected and operational
    Connected,

    /// Tunnel is attempting to reconnect
    Reconnecting { attempt: u32 },

    /// Tunnel has failed
    Failed,

    /// Tunnel is stopped
    Stopped,
}

impl std::fmt::Display for TunnelStatusDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TunnelStatusDisplay::Starting => write!(f, "◐ Starting"),
            TunnelStatusDisplay::Connected => write!(f, "● Connected"),
            TunnelStatusDisplay::Reconnecting { attempt } => {
                write!(f, "⟳ Reconnecting (attempt {})", attempt)
            }
            TunnelStatusDisplay::Failed => write!(f, "✗ Failed"),
            TunnelStatusDisplay::Stopped => write!(f, "○ Stopped"),
        }
    }
}

/// Get the path to the daemon socket file (Unix) or port file (Windows)
#[cfg(unix)]
pub fn socket_path() -> PathBuf {
    dirs::home_dir()
        .expect("Failed to get home directory")
        .join(".localup")
        .join("daemon.sock")
}

/// On Windows, we use a fixed localhost port stored in a file
#[cfg(windows)]
pub fn socket_path() -> PathBuf {
    dirs::home_dir()
        .expect("Failed to get home directory")
        .join(".localup")
        .join("daemon.port")
}

/// Default port for Windows IPC (used if port file doesn't exist)
#[cfg(windows)]
const DEFAULT_IPC_PORT: u16 = 17845;

/// Read the IPC port from the port file on Windows
#[cfg(windows)]
fn read_ipc_port() -> u16 {
    let port_path = socket_path();
    if port_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&port_path) {
            if let Ok(port) = content.trim().parse() {
                return port;
            }
        }
    }
    DEFAULT_IPC_PORT
}

/// Write the IPC port to the port file on Windows
#[cfg(windows)]
fn write_ipc_port(port: u16) -> std::io::Result<()> {
    let port_path = socket_path();
    if let Some(parent) = port_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&port_path, port.to_string())
}

// ============================================================================
// Unix Implementation
// ============================================================================

#[cfg(unix)]
/// IPC client for CLI to connect to daemon
pub struct IpcClient {
    stream: BufReader<UnixStream>,
}

#[cfg(unix)]
impl IpcClient {
    /// Connect to the daemon socket
    pub async fn connect() -> Result<Self> {
        let path = socket_path();
        let stream = UnixStream::connect(&path)
            .await
            .with_context(|| format!("Failed to connect to daemon socket at {:?}", path))?;

        Ok(Self {
            stream: BufReader::new(stream),
        })
    }

    /// Connect to a specific socket path (for testing)
    pub async fn connect_to(path: &std::path::Path) -> Result<Self> {
        let stream = UnixStream::connect(path)
            .await
            .with_context(|| format!("Failed to connect to socket at {:?}", path))?;

        Ok(Self {
            stream: BufReader::new(stream),
        })
    }

    /// Send a request and receive a response
    pub async fn request(&mut self, req: &IpcRequest) -> Result<IpcResponse> {
        // Serialize request to JSON and send with newline delimiter
        let mut json = serde_json::to_string(req)?;
        json.push('\n');

        self.stream
            .get_mut()
            .write_all(json.as_bytes())
            .await
            .context("Failed to send request")?;

        self.stream
            .get_mut()
            .flush()
            .await
            .context("Failed to flush request")?;

        // Read response line
        let mut response_line = String::new();
        self.stream
            .read_line(&mut response_line)
            .await
            .context("Failed to read response")?;

        // Parse response
        let response: IpcResponse =
            serde_json::from_str(&response_line).context("Failed to parse response")?;

        Ok(response)
    }
}

#[cfg(unix)]
/// IPC server for daemon to listen for CLI connections
pub struct IpcServer {
    listener: UnixListener,
    socket_path: PathBuf,
}

#[cfg(unix)]
impl IpcServer {
    /// Bind to the daemon socket
    pub async fn bind() -> Result<Self> {
        let path = socket_path();
        Self::bind_to(&path).await
    }

    /// Bind to a specific socket path (for testing)
    pub async fn bind_to(path: &std::path::Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove stale socket if it exists
        if path.exists() {
            // Try to connect to see if it's alive
            match UnixStream::connect(path).await {
                Ok(_) => {
                    anyhow::bail!(
                        "Another daemon is already running (socket at {:?} is active)",
                        path
                    );
                }
                Err(_) => {
                    // Socket is stale, remove it
                    std::fs::remove_file(path)?;
                }
            }
        }

        let listener = UnixListener::bind(path)
            .with_context(|| format!("Failed to bind to socket at {:?}", path))?;

        Ok(Self {
            listener,
            socket_path: path.to_path_buf(),
        })
    }

    /// Accept an incoming connection
    pub async fn accept(&self) -> Result<IpcConnection> {
        let (stream, _) = self.listener.accept().await?;
        Ok(IpcConnection {
            stream: BufReader::new(stream),
        })
    }

    /// Get the socket path
    pub fn path(&self) -> &std::path::Path {
        &self.socket_path
    }
}

#[cfg(unix)]
impl Drop for IpcServer {
    fn drop(&mut self) {
        // Clean up socket file on shutdown
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

#[cfg(unix)]
/// A single IPC connection from a client
pub struct IpcConnection {
    stream: BufReader<UnixStream>,
}

#[cfg(unix)]
impl IpcConnection {
    /// Receive a request from the client
    pub async fn recv(&mut self) -> Result<IpcRequest> {
        let mut line = String::new();
        let bytes_read = self
            .stream
            .read_line(&mut line)
            .await
            .context("Failed to read request")?;

        if bytes_read == 0 {
            anyhow::bail!("Connection closed");
        }

        let request: IpcRequest = serde_json::from_str(&line).context("Failed to parse request")?;

        Ok(request)
    }

    /// Send a response to the client
    pub async fn send(&mut self, response: &IpcResponse) -> Result<()> {
        let mut json = serde_json::to_string(response)?;
        json.push('\n');

        self.stream
            .get_mut()
            .write_all(json.as_bytes())
            .await
            .context("Failed to send response")?;

        self.stream
            .get_mut()
            .flush()
            .await
            .context("Failed to flush response")?;

        Ok(())
    }
}

// ============================================================================
// Windows Implementation (using TCP on localhost)
// ============================================================================

#[cfg(windows)]
/// IPC client for CLI to connect to daemon
pub struct IpcClient {
    stream: BufReader<TcpStream>,
}

#[cfg(windows)]
impl IpcClient {
    /// Connect to the daemon
    pub async fn connect() -> Result<Self> {
        let port = read_ipc_port();
        let addr = format!("127.0.0.1:{}", port);
        let stream = TcpStream::connect(&addr)
            .await
            .with_context(|| format!("Failed to connect to daemon at {}", addr))?;

        Ok(Self {
            stream: BufReader::new(stream),
        })
    }

    /// Connect to a specific port (for testing)
    pub async fn connect_to_port(port: u16) -> Result<Self> {
        let addr = format!("127.0.0.1:{}", port);
        let stream = TcpStream::connect(&addr)
            .await
            .with_context(|| format!("Failed to connect to {}", addr))?;

        Ok(Self {
            stream: BufReader::new(stream),
        })
    }

    /// Send a request and receive a response
    pub async fn request(&mut self, req: &IpcRequest) -> Result<IpcResponse> {
        // Serialize request to JSON and send with newline delimiter
        let mut json = serde_json::to_string(req)?;
        json.push('\n');

        self.stream
            .get_mut()
            .write_all(json.as_bytes())
            .await
            .context("Failed to send request")?;

        self.stream
            .get_mut()
            .flush()
            .await
            .context("Failed to flush request")?;

        // Read response line
        let mut response_line = String::new();
        self.stream
            .read_line(&mut response_line)
            .await
            .context("Failed to read response")?;

        // Parse response
        let response: IpcResponse =
            serde_json::from_str(&response_line).context("Failed to parse response")?;

        Ok(response)
    }
}

#[cfg(windows)]
/// IPC server for daemon to listen for CLI connections
pub struct IpcServer {
    listener: TcpListener,
    port: u16,
}

#[cfg(windows)]
impl IpcServer {
    /// Bind to the daemon port
    pub async fn bind() -> Result<Self> {
        Self::bind_to_port(DEFAULT_IPC_PORT).await
    }

    /// Bind to a specific port
    pub async fn bind_to_port(port: u16) -> Result<Self> {
        let addr = format!("127.0.0.1:{}", port);

        // Try to connect to see if another daemon is running
        if TcpStream::connect(&addr).await.is_ok() {
            anyhow::bail!(
                "Another daemon is already running (port {} is in use)",
                port
            );
        }

        let listener = TcpListener::bind(&addr)
            .await
            .with_context(|| format!("Failed to bind to {}", addr))?;

        // Write port to file so clients can find us
        write_ipc_port(port)?;

        Ok(Self { listener, port })
    }

    /// Accept an incoming connection
    pub async fn accept(&self) -> Result<IpcConnection> {
        let (stream, _) = self.listener.accept().await?;
        Ok(IpcConnection {
            stream: BufReader::new(stream),
        })
    }

    /// Get the port
    pub fn port(&self) -> u16 {
        self.port
    }
}

#[cfg(windows)]
impl Drop for IpcServer {
    fn drop(&mut self) {
        // Clean up port file on shutdown
        let port_path = socket_path();
        if port_path.exists() {
            let _ = std::fs::remove_file(&port_path);
        }
    }
}

#[cfg(windows)]
/// A single IPC connection from a client
pub struct IpcConnection {
    stream: BufReader<TcpStream>,
}

#[cfg(windows)]
impl IpcConnection {
    /// Receive a request from the client
    pub async fn recv(&mut self) -> Result<IpcRequest> {
        let mut line = String::new();
        let bytes_read = self
            .stream
            .read_line(&mut line)
            .await
            .context("Failed to read request")?;

        if bytes_read == 0 {
            anyhow::bail!("Connection closed");
        }

        let request: IpcRequest = serde_json::from_str(&line).context("Failed to parse request")?;

        Ok(request)
    }

    /// Send a response to the client
    pub async fn send(&mut self, response: &IpcResponse) -> Result<()> {
        let mut json = serde_json::to_string(response)?;
        json.push('\n');

        self.stream
            .get_mut()
            .write_all(json.as_bytes())
            .await
            .context("Failed to send response")?;

        self.stream
            .get_mut()
            .flush()
            .await
            .context("Failed to flush response")?;

        Ok(())
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Format duration in human-readable format
pub fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        format!("{}h {}m", hours, minutes)
    }
}

/// Print status table to stdout
pub fn print_status_table(tunnels: &HashMap<String, TunnelStatusInfo>) {
    if tunnels.is_empty() {
        println!("No tunnels configured.");
        return;
    }

    // Header
    println!(
        "{:<12} {:<10} {:<10} {:<40} STATUS",
        "TUNNEL", "PROTOCOL", "LOCAL", "PUBLIC URL"
    );

    // Sort tunnels by name for consistent output
    let mut sorted: Vec<_> = tunnels.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));

    for (_, info) in sorted {
        let status_str = match &info.status {
            TunnelStatusDisplay::Connected => {
                let uptime = info
                    .uptime_seconds
                    .map(|s| format!(" ({})", format_duration(s)))
                    .unwrap_or_default();
                format!("● Connected{}", uptime)
            }
            TunnelStatusDisplay::Starting => "◐ Starting".to_string(),
            TunnelStatusDisplay::Reconnecting { attempt } => {
                format!("⟳ Reconnecting (attempt {})", attempt)
            }
            TunnelStatusDisplay::Failed => {
                let error = info
                    .last_error
                    .as_ref()
                    .map(|e| format!(": {}", e))
                    .unwrap_or_default();
                format!("✗ Failed{}", error)
            }
            TunnelStatusDisplay::Stopped => "○ Stopped".to_string(),
        };

        let public_url = info.public_url.as_deref().unwrap_or("-");
        let local = format!(":{}", info.local_port);

        println!(
            "{:<12} {:<10} {:<10} {:<40} {}",
            info.name, info.protocol, local, public_url, status_str
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_request_serialization() {
        // GetStatus
        let req = IpcRequest::GetStatus;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"type":"get_status"}"#);

        // StartTunnel
        let req = IpcRequest::StartTunnel {
            name: "api".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"type":"start_tunnel","name":"api"}"#);

        // StopTunnel
        let req = IpcRequest::StopTunnel {
            name: "db".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"type":"stop_tunnel","name":"db"}"#);

        // Ping
        let req = IpcRequest::Ping;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"type":"ping"}"#);

        // Reload
        let req = IpcRequest::Reload;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"type":"reload"}"#);
    }

    #[test]
    fn test_ipc_request_deserialization() {
        let json = r#"{"type":"get_status"}"#;
        let req: IpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req, IpcRequest::GetStatus);

        let json = r#"{"type":"start_tunnel","name":"myapp"}"#;
        let req: IpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            req,
            IpcRequest::StartTunnel {
                name: "myapp".to_string()
            }
        );

        let json = r#"{"type":"ping"}"#;
        let req: IpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req, IpcRequest::Ping);
    }

    #[test]
    fn test_ipc_response_serialization() {
        // Status response
        let mut tunnels = HashMap::new();
        tunnels.insert(
            "api".to_string(),
            TunnelStatusInfo {
                name: "api".to_string(),
                protocol: "http".to_string(),
                local_port: 3000,
                public_url: Some("https://api.localup.dev".to_string()),
                status: TunnelStatusDisplay::Connected,
                uptime_seconds: Some(3600),
                last_error: None,
            },
        );

        let resp = IpcResponse::Status { tunnels };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""type":"status""#));
        assert!(json.contains(r#""name":"api""#));
        assert!(json.contains(r#""protocol":"http""#));
        assert!(json.contains(r#""local_port":3000"#));

        // Ok response
        let resp = IpcResponse::Ok {
            message: Some("Tunnel started".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"type":"ok","message":"Tunnel started"}"#);

        // Error response
        let resp = IpcResponse::Error {
            message: "Not found".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"type":"error","message":"Not found"}"#);

        // Pong response
        let resp = IpcResponse::Pong;
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"type":"pong"}"#);
    }

    #[test]
    fn test_ipc_response_deserialization() {
        let json = r#"{"type":"pong"}"#;
        let resp: IpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp, IpcResponse::Pong);

        let json = r#"{"type":"ok","message":null}"#;
        let resp: IpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp, IpcResponse::Ok { message: None });

        let json = r#"{"type":"error","message":"Something went wrong"}"#;
        let resp: IpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            resp,
            IpcResponse::Error {
                message: "Something went wrong".to_string()
            }
        );
    }

    #[test]
    fn test_tunnel_status_display() {
        assert_eq!(TunnelStatusDisplay::Starting.to_string(), "◐ Starting");
        assert_eq!(TunnelStatusDisplay::Connected.to_string(), "● Connected");
        assert_eq!(
            TunnelStatusDisplay::Reconnecting { attempt: 3 }.to_string(),
            "⟳ Reconnecting (attempt 3)"
        );
        assert_eq!(TunnelStatusDisplay::Failed.to_string(), "✗ Failed");
        assert_eq!(TunnelStatusDisplay::Stopped.to_string(), "○ Stopped");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(59), "59s");
        assert_eq!(format_duration(60), "1m 0s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3599), "59m 59s");
        assert_eq!(format_duration(3600), "1h 0m");
        assert_eq!(format_duration(3660), "1h 1m");
        assert_eq!(format_duration(7200), "2h 0m");
        assert_eq!(format_duration(7265), "2h 1m");
    }

    #[test]
    fn test_socket_path() {
        let path = socket_path();
        #[cfg(unix)]
        {
            assert!(path.ends_with("daemon.sock"));
            assert!(path.to_string_lossy().contains(".localup"));
        }
        #[cfg(windows)]
        {
            assert!(path.ends_with("daemon.port"));
            assert!(path.to_string_lossy().contains(".localup"));
        }
    }

    #[test]
    fn test_tunnel_status_info_serialization_roundtrip() {
        let info = TunnelStatusInfo {
            name: "test".to_string(),
            protocol: "https".to_string(),
            local_port: 8080,
            public_url: Some("https://test.example.com".to_string()),
            status: TunnelStatusDisplay::Reconnecting { attempt: 2 },
            uptime_seconds: None,
            last_error: Some("Connection timeout".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: TunnelStatusInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.protocol, "https");
        assert_eq!(parsed.local_port, 8080);
        assert_eq!(
            parsed.public_url,
            Some("https://test.example.com".to_string())
        );
        assert_eq!(
            parsed.status,
            TunnelStatusDisplay::Reconnecting { attempt: 2 }
        );
        assert_eq!(parsed.uptime_seconds, None);
        assert_eq!(parsed.last_error, Some("Connection timeout".to_string()));
    }

    // Unix-only integration tests
    #[cfg(unix)]
    mod unix_tests {
        use super::*;

        #[tokio::test]
        async fn test_ipc_client_server_roundtrip() {
            use tempfile::TempDir;

            let temp_dir = TempDir::new().unwrap();
            let socket_path = temp_dir.path().join("test.sock");

            // Start server
            let server = IpcServer::bind_to(&socket_path).await.unwrap();

            // Spawn server handler
            let server_handle = tokio::spawn(async move {
                let mut conn = server.accept().await.unwrap();
                let request = conn.recv().await.unwrap();

                let response = match request {
                    IpcRequest::Ping => IpcResponse::Pong,
                    IpcRequest::GetStatus => IpcResponse::Status {
                        tunnels: HashMap::new(),
                    },
                    _ => IpcResponse::Error {
                        message: "Unknown request".to_string(),
                    },
                };

                conn.send(&response).await.unwrap();
            });

            // Connect client and send request
            let mut client = IpcClient::connect_to(&socket_path).await.unwrap();
            let response = client.request(&IpcRequest::Ping).await.unwrap();

            assert_eq!(response, IpcResponse::Pong);

            server_handle.await.unwrap();
        }

        #[tokio::test]
        async fn test_ipc_get_status_roundtrip() {
            use tempfile::TempDir;

            let temp_dir = TempDir::new().unwrap();
            let socket_path = temp_dir.path().join("test.sock");

            // Start server
            let server = IpcServer::bind_to(&socket_path).await.unwrap();

            // Spawn server handler with status data
            let server_handle = tokio::spawn(async move {
                let mut conn = server.accept().await.unwrap();
                let request = conn.recv().await.unwrap();

                if let IpcRequest::GetStatus = request {
                    let mut tunnels = HashMap::new();
                    tunnels.insert(
                        "api".to_string(),
                        TunnelStatusInfo {
                            name: "api".to_string(),
                            protocol: "http".to_string(),
                            local_port: 3000,
                            public_url: Some("https://api.example.com".to_string()),
                            status: TunnelStatusDisplay::Connected,
                            uptime_seconds: Some(120),
                            last_error: None,
                        },
                    );

                    conn.send(&IpcResponse::Status { tunnels }).await.unwrap();
                }
            });

            // Connect client and get status
            let mut client = IpcClient::connect_to(&socket_path).await.unwrap();
            let response = client.request(&IpcRequest::GetStatus).await.unwrap();

            if let IpcResponse::Status { tunnels } = response {
                assert_eq!(tunnels.len(), 1);
                let api = tunnels.get("api").unwrap();
                assert_eq!(api.name, "api");
                assert_eq!(api.protocol, "http");
                assert_eq!(api.local_port, 3000);
                assert_eq!(api.status, TunnelStatusDisplay::Connected);
            } else {
                panic!("Expected Status response");
            }

            server_handle.await.unwrap();
        }

        #[tokio::test]
        async fn test_ipc_stale_socket_cleanup() {
            use tempfile::TempDir;

            let temp_dir = TempDir::new().unwrap();
            let socket_path = temp_dir.path().join("stale.sock");

            // Create a stale socket file (not a real socket)
            std::fs::write(&socket_path, "stale").unwrap();

            // Server should clean up stale socket and bind successfully
            let server = IpcServer::bind_to(&socket_path).await.unwrap();
            assert!(socket_path.exists());

            drop(server);

            // Socket should be cleaned up on drop
            assert!(!socket_path.exists());
        }

        #[tokio::test]
        async fn test_ipc_multiple_requests() {
            use tempfile::TempDir;

            let temp_dir = TempDir::new().unwrap();
            let socket_path = temp_dir.path().join("multi.sock");

            let server = IpcServer::bind_to(&socket_path).await.unwrap();

            // Server handles multiple requests on same connection
            let server_handle = tokio::spawn(async move {
                let mut conn = server.accept().await.unwrap();

                // Handle first request
                let req1 = conn.recv().await.unwrap();
                assert_eq!(req1, IpcRequest::Ping);
                conn.send(&IpcResponse::Pong).await.unwrap();

                // Handle second request
                let req2 = conn.recv().await.unwrap();
                assert_eq!(req2, IpcRequest::Reload);
                conn.send(&IpcResponse::Ok {
                    message: Some("Reloaded".to_string()),
                })
                .await
                .unwrap();
            });

            let mut client = IpcClient::connect_to(&socket_path).await.unwrap();

            // Send first request
            let resp1 = client.request(&IpcRequest::Ping).await.unwrap();
            assert_eq!(resp1, IpcResponse::Pong);

            // Send second request on same connection
            let resp2 = client.request(&IpcRequest::Reload).await.unwrap();
            assert_eq!(
                resp2,
                IpcResponse::Ok {
                    message: Some("Reloaded".to_string())
                }
            );

            server_handle.await.unwrap();
        }
    }

    // Windows integration tests
    #[cfg(windows)]
    mod windows_tests {
        use super::*;

        #[tokio::test]
        async fn test_ipc_client_server_roundtrip() {
            // Use a random high port to avoid conflicts
            let port = 19845;

            // Start server
            let server = IpcServer::bind_to_port(port).await.unwrap();

            // Spawn server handler
            let server_handle = tokio::spawn(async move {
                let mut conn = server.accept().await.unwrap();
                let request = conn.recv().await.unwrap();

                let response = match request {
                    IpcRequest::Ping => IpcResponse::Pong,
                    _ => IpcResponse::Error {
                        message: "Unknown request".to_string(),
                    },
                };

                conn.send(&response).await.unwrap();
            });

            // Connect client and send request
            let mut client = IpcClient::connect_to_port(port).await.unwrap();
            let response = client.request(&IpcRequest::Ping).await.unwrap();

            assert_eq!(response, IpcResponse::Pong);

            server_handle.await.unwrap();
        }
    }
}
