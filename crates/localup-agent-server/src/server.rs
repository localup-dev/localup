//! Agent-server implementation
//!
//! Combines relay and agent functionality in a single server.
//! Accepts client connections and forwards to internal targets with access control.

use crate::access_control::AccessControl;
use localup_agent::TcpForwarder;
use localup_proto::TunnelMessage;
use localup_transport::{TransportConnection, TransportListener, TransportStream};
use localup_transport_quic::{QuicConfig, QuicListener};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error, info, warn};

/// Relay connection configuration
/// When set, this agent server will connect to a relay server and register itself
/// Clients can then connect to the relay to reach this agent server
#[derive(Debug, Clone)]
pub struct RelayConfig {
    /// Relay server address (the relay to connect to)
    pub relay_addr: SocketAddr,
    /// Agent server ID on the relay (how clients will identify this server)
    pub server_id: String,
    /// Target address where this agent will forward relay traffic
    /// (e.g., "127.0.0.1:5432" for PostgreSQL, "192.168.1.100:8080" for web service)
    pub target_address: String,
    /// Authentication token for relay
    pub relay_token: Option<String>,
}

/// Agent-server configuration
#[derive(Debug, Clone)]
pub struct AgentServerConfig {
    /// Listen address for QUIC server
    pub listen_addr: SocketAddr,
    /// TLS certificate path (optional, auto-generated if None)
    pub cert_path: Option<String>,
    /// TLS key path (optional, auto-generated if None)
    pub key_path: Option<String>,
    /// Access control rules
    pub access_control: AccessControl,
    /// Optional JWT secret for authentication
    pub jwt_secret: Option<String>,
    /// Optional relay connection configuration
    /// If set, this server will register itself with the relay
    pub relay_config: Option<RelayConfig>,
}

/// Agent-server
pub struct AgentServer {
    config: AgentServerConfig,
    listener: QuicListener,
    forwarder: Arc<TcpForwarder>,
}

impl AgentServer {
    /// Create new agent-server
    pub fn new(config: AgentServerConfig) -> anyhow::Result<Self> {
        info!("Initializing agent-server on {}", config.listen_addr);

        // Create QUIC config with auto-generation if needed
        let quic_config =
            if let (Some(cert_path), Some(key_path)) = (&config.cert_path, &config.key_path) {
                info!("🔐 Using custom TLS certificates for QUIC");
                Arc::new(QuicConfig::server_default(cert_path, key_path)?)
            } else {
                info!("🔐 Auto-generating self-signed certificate for QUIC...");
                let config = Arc::new(QuicConfig::server_self_signed()?);
                info!("✅ Self-signed certificate generated (valid for 90 days)");
                config
            };

        // Create QUIC listener
        let listener = QuicListener::new(config.listen_addr, quic_config)?;

        // Create TCP forwarder
        let forwarder = Arc::new(TcpForwarder::new());

        Ok(Self {
            config,
            listener,
            forwarder,
        })
    }

    /// Start the server
    pub async fn run(self) -> anyhow::Result<()> {
        info!("🚀 Agent-server listening on {}", self.config.listen_addr);
        info!(
            "Access control: {} CIDR ranges, {} port ranges",
            if self.config.access_control.allowed_cidrs.is_empty() {
                "ALL".to_string()
            } else {
                self.config.access_control.allowed_cidrs.len().to_string()
            },
            if self.config.access_control.allowed_ports.is_empty() {
                "ALL".to_string()
            } else {
                self.config.access_control.allowed_ports.len().to_string()
            }
        );

        let config = self.config.clone();
        let forwarder = self.forwarder.clone();
        let jwt_secret = config.jwt_secret.clone();

        // If relay is configured, spawn relay connection task with exponential backoff
        if let Some(relay_config) = &config.relay_config {
            let relay_config = relay_config.clone();
            tokio::spawn(async move {
                // Exponential backoff parameters (matching tunnel-client behavior)
                let initial_backoff = Duration::from_secs(1);
                let max_backoff = Duration::from_secs(60);
                let backoff_multiplier = 2.0;

                let mut current_backoff = initial_backoff;
                let mut attempt = 0;

                loop {
                    attempt += 1;
                    info!(
                        "Connecting to relay at {} with agent ID: {} (attempt {})",
                        relay_config.relay_addr, relay_config.server_id, attempt
                    );

                    // Create agent configuration for relay connection
                    let agent_config = localup_agent::AgentConfig {
                        agent_id: relay_config.server_id.clone(),
                        relay_addr: format!("{}", relay_config.relay_addr),
                        auth_token: relay_config.relay_token.clone().unwrap_or_default(),
                        target_address: relay_config.target_address.clone(),
                        local_address: None, // We don't need local listener when acting as relay agent
                        insecure: true,      // Use insecure mode for self-signed relay certificates
                        jwt_secret: jwt_secret.clone(),
                    };

                    match localup_agent::Agent::new(agent_config) {
                        Ok(mut agent) => {
                            info!("Agent created successfully, starting relay connection");
                            // Reset backoff on successful connection
                            current_backoff = initial_backoff;
                            attempt = 0;

                            match agent.start().await {
                                Ok(_) => {
                                    info!("Agent stopped gracefully");
                                }
                                Err(e) => {
                                    error!("Agent error: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to create agent: {}", e);
                        }
                    }

                    // Exponential backoff before reconnecting
                    info!(
                        "Relay connection ended, reconnecting in {}s (attempt {})...",
                        current_backoff.as_secs(),
                        attempt
                    );
                    tokio::time::sleep(current_backoff).await;

                    // Increase backoff for next attempt
                    let next_backoff =
                        Duration::from_secs_f64(current_backoff.as_secs_f64() * backoff_multiplier);
                    current_backoff = next_backoff.min(max_backoff);
                }
            });
        }

        loop {
            match self.listener.accept().await {
                Ok((connection, peer_addr)) => {
                    info!("New connection from {}", peer_addr);

                    let config_clone = config.clone();
                    let forwarder_clone = forwarder.clone();

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(
                            Arc::new(connection),
                            peer_addr,
                            config_clone,
                            forwarder_clone,
                        )
                        .await
                        {
                            error!("Connection error from {}: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// Handle a single client connection
    async fn handle_connection(
        connection: Arc<localup_transport_quic::QuicConnection>,
        peer_addr: SocketAddr,
        config: AgentServerConfig,
        forwarder: Arc<TcpForwarder>,
    ) -> anyhow::Result<()> {
        // Accept control stream
        let mut control_stream = match connection.accept_stream().await? {
            Some(stream) => stream,
            None => {
                error!("Connection closed before control stream");
                return Ok(());
            }
        };

        // Read first message (should be AgentRegister)
        let first_message = match control_stream.recv_message().await? {
            Some(msg) => msg,
            None => {
                error!("Connection closed before first message");
                return Ok(());
            }
        };

        match first_message {
            TunnelMessage::AgentRegister {
                agent_id,
                auth_token: _,
                target_address,
                metadata,
            } => {
                info!(
                    "Agent registration from {}: agent_id={}, target={}, hostname={}",
                    peer_addr, agent_id, target_address, metadata.hostname
                );

                // Validate target address against access control
                match config.access_control.validate_target(&target_address) {
                    Ok(target_addr) => {
                        // Send acceptance
                        control_stream
                            .send_message(&TunnelMessage::AgentRegistered {
                                agent_id: agent_id.clone(),
                            })
                            .await?;

                        info!("✅ Agent registered: {} -> {}", agent_id, target_addr);

                        // Handle agent's forwarding requests
                        Self::handle_agent_forwarding(
                            control_stream,
                            connection,
                            agent_id,
                            target_addr,
                            forwarder,
                        )
                        .await;
                    }
                    Err(e) => {
                        warn!("Access denied for agent {}: {}", agent_id, e);
                        control_stream
                            .send_message(&TunnelMessage::AgentRejected {
                                reason: format!("Access denied: {}", e),
                            })
                            .await?;
                    }
                }
            }
            _ => {
                error!("Unexpected first message: {:?}", first_message);
                control_stream
                    .send_message(&TunnelMessage::AgentRejected {
                        reason: "Expected AgentRegister message as first message".to_string(),
                    })
                    .await?;
            }
        }

        Ok(())
    }

    /// Handle agent forwarding requests
    async fn handle_agent_forwarding(
        mut control_stream: localup_transport_quic::QuicStream,
        _connection: Arc<localup_transport_quic::QuicConnection>,
        agent_id: String,
        _target_addr: SocketAddr,
        _forwarder: Arc<TcpForwarder>,
    ) {
        info!("Agent {} connected and ready for forwarding", agent_id);

        // Keep the agent connected with heartbeat
        loop {
            match control_stream.recv_message().await {
                Ok(Some(TunnelMessage::Ping { timestamp })) => {
                    debug!("Received Ping from agent {} at {}", agent_id, timestamp);
                    if let Err(e) = control_stream
                        .send_message(&TunnelMessage::Pong { timestamp })
                        .await
                    {
                        error!("Failed to send Pong to agent {}: {}", agent_id, e);
                        break;
                    }
                }
                Ok(Some(TunnelMessage::Disconnect { reason })) => {
                    info!("Agent {} disconnected: {}", agent_id, reason);
                    break;
                }
                Ok(None) => {
                    info!("Agent {} control stream closed", agent_id);
                    break;
                }
                Err(e) => {
                    error!("Error reading from agent {}: {}", agent_id, e);
                    break;
                }
                Ok(Some(msg)) => {
                    warn!("Unexpected message from agent {}: {:?}", agent_id, msg);
                }
            }
        }

        info!("Agent {} session ended", agent_id);
    }

    /// Handle a single TCP connection to target (unused for now, reserved for future use)
    #[allow(dead_code)]
    async fn handle_tcp_connection(
        tcp_stream: TcpStream,
        mut rx: tokio::sync::mpsc::Receiver<TunnelMessage>,
        control_send: Arc<tokio::sync::Mutex<localup_transport_quic::QuicSendHalf>>,
        localup_id: String,
        stream_id: u32,
    ) -> anyhow::Result<()> {
        let (mut tcp_read, mut tcp_write) = tcp_stream.into_split();

        // Task: TCP → Client
        let localup_id_clone = localup_id.clone();
        let control_send_clone = control_send.clone();
        let tcp_to_client = tokio::spawn(async move {
            let mut buffer = vec![0u8; 8192];
            loop {
                match tcp_read.read(&mut buffer).await {
                    Ok(0) => {
                        debug!("TCP connection closed (stream {})", stream_id);
                        let mut send = control_send_clone.lock().await;
                        let _ = send
                            .send_message(&TunnelMessage::ReverseClose {
                                localup_id: localup_id_clone.clone(),
                                stream_id,
                                reason: None,
                            })
                            .await;
                        break;
                    }
                    Ok(n) => {
                        debug!("Read {} bytes from target (stream {})", n, stream_id);
                        let mut send = control_send_clone.lock().await;
                        if let Err(e) = send
                            .send_message(&TunnelMessage::ReverseData {
                                localup_id: localup_id_clone.clone(),
                                stream_id,
                                data: buffer[..n].to_vec(),
                            })
                            .await
                        {
                            error!("Failed to send to client: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error reading from target (stream {}): {}", stream_id, e);
                        break;
                    }
                }
            }
        });

        // Task: Client → TCP
        let client_to_tcp = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Some(TunnelMessage::ReverseData { data, .. }) => {
                        debug!(
                            "Received {} bytes from client (stream {})",
                            data.len(),
                            stream_id
                        );
                        if let Err(e) = tcp_write.write_all(&data).await {
                            error!("Failed to write to target (stream {}): {}", stream_id, e);
                            break;
                        }
                    }
                    Some(TunnelMessage::ReverseClose { .. }) => {
                        debug!("Client closed stream {}", stream_id);
                        break;
                    }
                    None => {
                        debug!("Message channel closed (stream {})", stream_id);
                        break;
                    }
                    Some(msg) => {
                        warn!("Unexpected message for stream {}: {:?}", stream_id, msg);
                    }
                }
            }
        });

        let _ = tokio::join!(tcp_to_client, client_to_tcp);
        debug!("TCP connection handler finished (stream {})", stream_id);

        Ok(())
    }
}
