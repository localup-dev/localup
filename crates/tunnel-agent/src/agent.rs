use crate::connection::{ConnectionInfo, ConnectionManager};
use crate::forwarder::{ForwarderError, TcpForwarder};
use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tunnel_proto::{AgentMetadata, TunnelMessage};
use tunnel_transport::{TransportConnection, TransportConnector, TransportError, TransportStream};
use tunnel_transport_quic::{QuicConfig, QuicConnection, QuicConnector, QuicStream};

/// Errors that can occur in the agent
#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Invalid allowlist configuration: {0}")]
    InvalidAllowlist(String),

    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("Forwarding error: {0}")]
    Forwarder(#[from] ForwarderError),

    #[error("Registration failed: {0}")]
    RegistrationFailed(String),

    #[error("Message handling error: {0}")]
    MessageHandling(String),

    #[error("Agent already running")]
    AlreadyRunning,

    #[error("Connection not established")]
    ConnectionNotEstablished,

    #[error("Address resolution failed: {0}")]
    AddressResolution(String),
}

/// Configuration for the agent
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Unique identifier for this agent
    pub agent_id: String,

    /// Relay server address (host:port)
    pub relay_addr: String,

    /// Authentication token for the relay
    pub auth_token: String,

    /// Target address this agent will forward to (e.g., "192.168.1.100:8080")
    /// This agent will ONLY forward traffic to this specific address
    pub target_address: String,

    /// Local address to bind and listen (optional, e.g., "0.0.0.0:5433")
    /// If specified, incoming connections will be forwarded to target_address via the relay
    pub local_address: Option<String>,

    /// Whether to skip certificate verification (insecure, for development only)
    pub insecure: bool,

    /// JWT secret for validating agent tokens from clients (optional)
    /// If set, agent will validate tokens in ForwardRequest messages
    pub jwt_secret: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_id: uuid::Uuid::new_v4().to_string(),
            relay_addr: "localhost:4443".to_string(),
            auth_token: String::new(),
            target_address: "localhost:8080".to_string(),
            local_address: None,
            insecure: false,
            jwt_secret: None,
        }
    }
}

/// The tunnel agent - connects to relay and forwards traffic to a specific remote address
pub struct Agent {
    /// Unique identifier for this agent
    agent_id: String,

    /// Relay server address
    relay_addr: String,

    /// Authentication token
    auth_token: String,

    /// Target address this agent forwards to (e.g., "192.168.1.100:8080")
    target_address: String,

    /// TCP forwarder
    forwarder: Arc<TcpForwarder>,

    /// Connection manager
    connection_manager: ConnectionManager,

    /// The QUIC connection to the relay
    connection: Arc<Mutex<Option<Arc<QuicConnection>>>>,

    /// Flag indicating if the agent is running (public for listener to access)
    pub running: Arc<Mutex<bool>>,

    /// Flag indicating if the relay connection is active
    /// Used by local listener to check if relay is available
    connection_active: Arc<AtomicBool>,

    /// Configuration (for insecure flag and jwt_secret)
    config: AgentConfig,

    /// JWT secret for validating agent tokens (optional)
    jwt_secret: Option<String>,
}

impl Agent {
    /// Create a new agent with the given configuration
    ///
    /// # Arguments
    /// * `config` - Agent configuration
    ///
    /// # Returns
    /// Result with Agent or error if configuration is invalid
    pub fn new(config: AgentConfig) -> Result<Self, AgentError> {
        // Validate target address format
        if config.target_address.is_empty() {
            return Err(AgentError::InvalidAllowlist(
                "Target address cannot be empty".to_string(),
            ));
        }

        // Validate it's in "host:port" format
        if !config.target_address.contains(':') {
            return Err(AgentError::InvalidAllowlist(format!(
                "Invalid target address format '{}'. Expected 'host:port' (e.g., '192.168.1.100:8080')",
                config.target_address
            )));
        }

        let jwt_secret = config.jwt_secret.clone();
        let connection = Arc::new(Mutex::new(None));
        let forwarder = Arc::new(TcpForwarder::new());
        let connection_manager = ConnectionManager::new();

        Ok(Self {
            agent_id: config.agent_id.clone(),
            relay_addr: config.relay_addr.clone(),
            auth_token: config.auth_token.clone(),
            target_address: config.target_address.clone(),
            forwarder,
            connection_manager,
            connection,
            running: Arc::new(Mutex::new(false)),
            connection_active: Arc::new(AtomicBool::new(false)),
            config,
            jwt_secret,
        })
    }

    /// Start the agent - connects to relay and begins handling messages
    ///
    /// This method will block until the agent is stopped or an error occurs.
    ///
    /// # Returns
    /// Result indicating success or failure
    pub async fn start(&mut self) -> Result<(), AgentError> {
        // Check if already running
        let mut running = self.running.lock().await;
        if *running {
            return Err(AgentError::AlreadyRunning);
        }
        *running = true;
        drop(running);

        tracing::info!(
            agent_id = %self.agent_id,
            relay_addr = %self.relay_addr,
            "Starting agent"
        );

        // Connect to relay using QUIC
        let result = self.connect_to_relay().await;
        if let Err(e) = result {
            *self.running.lock().await = false;
            return Err(e);
        }

        // Register with the relay and get control stream
        let control_stream = match self.register().await {
            Ok(stream) => {
                // Mark connection as active after successful registration
                self.connection_active.store(true, Ordering::SeqCst);
                stream
            }
            Err(e) => {
                *self.running.lock().await = false;
                self.connection_active.store(false, Ordering::SeqCst);
                return Err(e);
            }
        };

        // Start message handling loop with control stream
        let result = self.handle_messages(control_stream).await;

        // Cleanup
        *self.running.lock().await = false;
        self.connection_active.store(false, Ordering::SeqCst);
        self.connection_manager.clear().await;

        tracing::info!(
            agent_id = %self.agent_id,
            "Agent stopped"
        );

        result
    }

    /// Start the local listener (if configured)
    /// This should be called once before entering the reconnection loop
    /// The listener persists across reconnects
    pub async fn start_local_listener(
        &self,
    ) -> Result<Option<tokio::task::JoinHandle<()>>, AgentError> {
        if let Some(local_addr_str) = &self.config.local_address {
            let local_addr = local_addr_str
                .parse::<std::net::SocketAddr>()
                .map_err(|e| {
                    AgentError::InvalidAllowlist(format!(
                        "Invalid local address '{}': {}",
                        local_addr_str, e
                    ))
                })?;

            let target_address = self.target_address.clone();
            let agent_id = self.agent_id.clone();
            let running_clone = self.running.clone();
            let connection_active_clone = self.connection_active.clone();

            let task = tokio::spawn(async move {
                Self::run_local_listener(
                    local_addr,
                    target_address,
                    agent_id,
                    running_clone,
                    connection_active_clone,
                )
                .await
            });

            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    /// Stop the agent gracefully
    pub async fn stop(&self) {
        tracing::info!(
            agent_id = %self.agent_id,
            "Stopping agent"
        );

        *self.running.lock().await = false;

        // Close the connection
        if let Some(conn) = self.connection.lock().await.as_ref() {
            conn.close(0, "Agent stopping").await;
        }
    }

    /// Check if the agent is currently running
    pub async fn is_running(&self) -> bool {
        *self.running.lock().await
    }

    /// Get the agent ID
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Get the number of active connections
    pub async fn active_connections(&self) -> usize {
        self.connection_manager.count().await
    }

    /// Run local TCP listener that accepts connections and proxies to target
    async fn run_local_listener(
        local_addr: std::net::SocketAddr,
        target_address: String,
        agent_id: String,
        running: Arc<Mutex<bool>>,
        connection_active: Arc<AtomicBool>,
    ) {
        let listener = match tokio::net::TcpListener::bind(local_addr).await {
            Ok(l) => {
                tracing::info!(
                    agent_id = %agent_id,
                    local_addr = %local_addr,
                    "Local TCP listener started"
                );
                l
            }
            Err(e) => {
                tracing::error!(
                    agent_id = %agent_id,
                    local_addr = %local_addr,
                    error = %e,
                    "Failed to bind local address"
                );
                return;
            }
        };

        let mut check_interval = tokio::time::interval(std::time::Duration::from_secs(1));

        loop {
            // Check if still running
            if !*running.lock().await {
                tracing::debug!(
                    agent_id = %agent_id,
                    "Local listener stopping"
                );
                break;
            }

            tokio::select! {
                // Accept new connections
                result = listener.accept() => {
                    match result {
                        Ok((socket, peer_addr)) => {
                            let target = target_address.clone();
                            let agent_id_clone = agent_id.clone();
                            let is_connected = connection_active.load(Ordering::SeqCst);

                            tracing::debug!(
                                agent_id = %agent_id_clone,
                                peer_addr = %peer_addr,
                                relay_active = is_connected,
                                "Accepted connection on local listener"
                            );

                            tokio::spawn(async move {
                                if let Err(e) = Self::proxy_connection(socket, target, agent_id_clone).await {
                                    tracing::error!(error = %e, "Local proxy connection failed");
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!(
                                agent_id = %agent_id,
                                error = %e,
                                "Local listener accept error"
                            );
                        }
                    }
                }

                // Check connection status every second
                _ = check_interval.tick() => {
                    let is_connected = connection_active.load(Ordering::SeqCst);
                    if !is_connected {
                        tracing::debug!(
                            agent_id = %agent_id,
                            "Relay connection inactive - waiting for reconnection"
                        );
                    }

                    if !*running.lock().await {
                        break;
                    }
                }
            }
        }

        tracing::info!(
            agent_id = %agent_id,
            "Local TCP listener stopped"
        );
    }

    /// Proxy a connection from local client to target address
    async fn proxy_connection(
        client: tokio::net::TcpStream,
        target: String,
        agent_id: String,
    ) -> Result<(), AgentError> {
        // Connect to target address
        let server = tokio::net::TcpStream::connect(&target).await.map_err(|e| {
            tracing::warn!(
                agent_id = %agent_id,
                target = %target,
                error = %e,
                "Failed to connect to target address"
            );
            AgentError::Forwarder(ForwarderError::ConnectionFailed {
                address: target.clone(),
                source: e,
            })
        })?;

        tracing::debug!(
            agent_id = %agent_id,
            target = %target,
            "Connected to target address, starting bidirectional proxy"
        );

        // Split both streams for bidirectional proxying
        let (mut client_read, mut client_write) = client.into_split();
        let (mut server_read, mut server_write) = server.into_split();

        // Proxy data in both directions concurrently
        tokio::select! {
            result = async {
                tokio::io::copy(&mut client_read, &mut server_write).await
            } => {
                if let Err(e) = result {
                    tracing::warn!(
                        agent_id = %agent_id,
                        target = %target,
                        error = %e,
                        "Error proxying client -> server"
                    );
                }
            }
            result = async {
                tokio::io::copy(&mut server_read, &mut client_write).await
            } => {
                if let Err(e) = result {
                    tracing::warn!(
                        agent_id = %agent_id,
                        target = %target,
                        error = %e,
                        "Error proxying server -> client"
                    );
                }
            }
        }

        tracing::debug!(
            agent_id = %agent_id,
            target = %target,
            "Proxy connection closed"
        );

        Ok(())
    }

    /// Connect to the relay server using QUIC
    async fn connect_to_relay(&self) -> Result<(), AgentError> {
        tracing::info!(
            agent_id = %self.agent_id,
            relay_addr = %self.relay_addr,
            "Connecting to relay"
        );

        // Resolve relay address
        let socket_addr = self
            .relay_addr
            .to_socket_addrs()
            .map_err(|e| {
                AgentError::AddressResolution(format!(
                    "Failed to resolve {}: {}",
                    self.relay_addr, e
                ))
            })?
            .next()
            .ok_or_else(|| {
                AgentError::AddressResolution(format!("No addresses found for {}", self.relay_addr))
            })?;

        // Extract server name for TLS (use hostname without port)
        let server_name = self.relay_addr.split(':').next().unwrap_or("localhost");

        // Create QUIC config
        let quic_config = if self.config.insecure {
            Arc::new(QuicConfig::client_insecure())
        } else {
            Arc::new(QuicConfig::client_default())
        };

        // Create connector
        let connector = QuicConnector::new(quic_config).map_err(AgentError::Transport)?;

        // Connect to relay
        let connection = connector.connect(socket_addr, server_name).await?;

        // Store connection
        *self.connection.lock().await = Some(Arc::new(connection));

        tracing::info!(
            agent_id = %self.agent_id,
            "Connected to relay successfully"
        );

        Ok(())
    }

    /// Register with the relay server and return the control stream
    async fn register(&self) -> Result<QuicStream, AgentError> {
        tracing::info!(
            agent_id = %self.agent_id,
            relay_addr = %self.relay_addr,
            "Registering with relay"
        );

        // Get connection
        let conn = self.connection.lock().await;
        let connection = conn.as_ref().ok_or(AgentError::ConnectionNotEstablished)?;

        // Open control stream (stream 0)
        let mut stream = connection.open_stream().await?;

        // Send AgentRegister message
        let register_msg = TunnelMessage::AgentRegister {
            agent_id: self.agent_id.clone(),
            auth_token: self.auth_token.clone(),
            target_address: self.target_address.clone(),
            metadata: AgentMetadata::default(),
        };

        stream.send_message(&register_msg).await?;

        // Wait for response
        let response = stream.recv_message().await?;

        match response {
            Some(TunnelMessage::AgentRegistered { agent_id }) => {
                tracing::info!(
                    agent_id = %agent_id,
                    "Registration successful"
                );
                Ok(stream) // Return the control stream to keep it alive
            }
            Some(TunnelMessage::AgentRejected { reason }) => {
                tracing::error!(agent_id = %self.agent_id, reason = %reason, "Registration rejected");
                Err(AgentError::RegistrationFailed(reason))
            }
            Some(TunnelMessage::Disconnect { reason }) => {
                tracing::error!(agent_id = %self.agent_id, reason = %reason, "Registration rejected (disconnected)");
                Err(AgentError::RegistrationFailed(reason))
            }
            Some(msg) => {
                let msg_type = format!("{:?}", msg);
                tracing::error!(agent_id = %self.agent_id, response = %msg_type, "Unexpected registration response");
                Err(AgentError::RegistrationFailed(format!(
                    "Unexpected response: {}",
                    msg_type
                )))
            }
            None => {
                tracing::error!(agent_id = %self.agent_id, "Registration stream closed");
                Err(AgentError::RegistrationFailed(
                    "Stream closed unexpectedly".to_string(),
                ))
            }
        }
    }

    /// Main message handling loop
    async fn handle_messages(&self, mut control_stream: QuicStream) -> Result<(), AgentError> {
        tracing::info!(
            agent_id = %self.agent_id,
            "Starting message handling loop"
        );

        // Spawn task to handle control stream (heartbeat Ping/Pong)
        let agent_id_heartbeat = self.agent_id.clone();
        let running_clone = self.running.clone();
        let jwt_secret_for_heartbeat = self.jwt_secret.clone();
        let heartbeat_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            interval.tick().await; // First tick completes immediately

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Check if still running
                        if !*running_clone.lock().await {
                            tracing::debug!("Heartbeat task stopping");
                            break;
                        }

                        // Send Ping message
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        tracing::debug!(
                            agent_id = %agent_id_heartbeat,
                            "Sending ping"
                        );

                        if let Err(e) = control_stream
                            .send_message(&TunnelMessage::Ping { timestamp })
                            .await
                        {
                            tracing::error!(
                                agent_id = %agent_id_heartbeat,
                                error = %e,
                                "Failed to send ping"
                            );
                            break;
                        }
                    }

                    // Receive messages from relay
                    msg_result = control_stream.recv_message() => {
                        match msg_result {
                            Ok(Some(TunnelMessage::Ping { timestamp })) => {
                                tracing::debug!(
                                    agent_id = %agent_id_heartbeat,
                                    timestamp = %timestamp,
                                    "Received ping from relay, responding with pong"
                                );
                                // Respond to relay's ping
                                if let Err(e) = control_stream
                                    .send_message(&TunnelMessage::Pong { timestamp })
                                    .await
                                {
                                    tracing::error!(
                                        agent_id = %agent_id_heartbeat,
                                        error = %e,
                                        "Failed to send pong response"
                                    );
                                    break;
                                }
                            }
                            Ok(Some(TunnelMessage::Pong { timestamp })) => {
                                tracing::debug!(
                                    agent_id = %agent_id_heartbeat,
                                    timestamp = %timestamp,
                                    "Received pong"
                                );
                            }
                            Ok(Some(TunnelMessage::ValidateAgentToken { agent_token })) => {
                                tracing::info!(
                                    agent_id = %agent_id_heartbeat,
                                    "Validating agent token"
                                );

                                // Validate token
                                let response = if let Some(ref secret) = jwt_secret_for_heartbeat {
                                    match &agent_token {
                                        Some(token) => {
                                            use tunnel_auth::JwtValidator;
                                            let validator = JwtValidator::new(secret.as_bytes());

                                            match validator.validate(token) {
                                                Ok(_claims) => {
                                                    tracing::info!(
                                                        agent_id = %agent_id_heartbeat,
                                                        "Agent token validated successfully"
                                                    );
                                                    TunnelMessage::ValidateAgentTokenOk
                                                }
                                                Err(e) => {
                                                    tracing::warn!(
                                                        agent_id = %agent_id_heartbeat,
                                                        error = %e,
                                                        "Agent token validation failed"
                                                    );
                                                    TunnelMessage::ValidateAgentTokenReject {
                                                        reason: format!(
                                                            "Authentication failed: invalid agent token: {}",
                                                            e
                                                        ),
                                                    }
                                                }
                                            }
                                        }
                                        None => {
                                            tracing::warn!(
                                                agent_id = %agent_id_heartbeat,
                                                "Agent token is missing but jwt_secret is configured"
                                            );
                                            TunnelMessage::ValidateAgentTokenReject {
                                                reason: "Authentication failed: agent token is required"
                                                    .to_string(),
                                            }
                                        }
                                    }
                                } else {
                                    // No jwt_secret configured, token validation skipped
                                    tracing::debug!(
                                        agent_id = %agent_id_heartbeat,
                                        "No JWT secret configured, skipping validation"
                                    );
                                    TunnelMessage::ValidateAgentTokenOk
                                };

                                // Send response
                                if let Err(e) = control_stream.send_message(&response).await {
                                    tracing::error!(
                                        agent_id = %agent_id_heartbeat,
                                        error = %e,
                                        "Failed to send token validation response"
                                    );
                                    break;
                                }
                            }
                            Ok(Some(TunnelMessage::Disconnect { reason })) => {
                                tracing::info!(
                                    agent_id = %agent_id_heartbeat,
                                    reason = %reason,
                                    "Relay requested disconnect"
                                );
                                break;
                            }
                            Ok(None) => {
                                tracing::info!(
                                    agent_id = %agent_id_heartbeat,
                                    "Control stream closed by relay"
                                );
                                break;
                            }
                            Err(e) => {
                                tracing::error!(
                                    agent_id = %agent_id_heartbeat,
                                    error = %e,
                                    "Error on control stream"
                                );
                                break;
                            }
                            Ok(Some(msg)) => {
                                tracing::warn!(
                                    agent_id = %agent_id_heartbeat,
                                    message = ?msg,
                                    "Unexpected message on control stream"
                                );
                            }
                        }
                    }
                }
            }

            tracing::debug!(
                agent_id = %agent_id_heartbeat,
                "Heartbeat task ended"
            );
        });

        // Main loop: Accept data streams for ForwardRequest messages
        loop {
            // Check if still running
            if !*self.running.lock().await {
                tracing::info!("Agent stopped, exiting message loop");
                break;
            }

            // Get connection
            let conn = self.connection.lock().await;
            let connection = match conn.as_ref() {
                Some(c) => c.clone(),
                None => {
                    tracing::error!("Connection lost");
                    return Err(AgentError::ConnectionNotEstablished);
                }
            };
            drop(conn);

            // Accept next data stream
            let stream = match connection.accept_stream().await? {
                Some(s) => s,
                None => {
                    tracing::info!("Connection closed by relay");
                    break;
                }
            };

            tracing::debug!(
                agent_id = %self.agent_id,
                stream_id = stream.stream_id(),
                "Accepted new data stream"
            );

            // Spawn task to handle this data stream
            let agent_id = self.agent_id.clone();
            let forwarder = self.forwarder.clone();
            let target_address = self.target_address.clone();
            let connection_manager = self.connection_manager.clone();
            let jwt_secret = self.jwt_secret.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_stream(
                    agent_id,
                    stream,
                    forwarder,
                    target_address,
                    connection_manager,
                    jwt_secret,
                )
                .await
                {
                    tracing::error!(error = %e, "Stream handling error");
                }
            });
        }

        // Wait for heartbeat task to complete
        let _ = heartbeat_task.await;

        Ok(())
    }

    /// Handle a single stream
    async fn handle_stream(
        agent_id: String,
        mut stream: QuicStream,
        forwarder: Arc<TcpForwarder>,
        target_address: String,
        connection_manager: ConnectionManager,
        jwt_secret: Option<String>,
    ) -> Result<(), AgentError> {
        let stream_id = stream.stream_id() as u32;

        tracing::debug!(
            agent_id = %agent_id,
            stream_id = stream_id,
            "Handling stream"
        );

        // Read ForwardRequest message
        let message = stream.recv_message().await?;

        match message {
            Some(TunnelMessage::ForwardRequest {
                tunnel_id,
                stream_id,
                remote_address,
                agent_token,
            }) => {
                tracing::info!(
                    agent_id = %agent_id,
                    tunnel_id = %tunnel_id,
                    stream_id = stream_id,
                    remote_address = %remote_address,
                    target_address = %target_address,
                    "Received forward request"
                );

                // Validate agent token if jwt_secret is configured
                if let Some(ref secret) = jwt_secret {
                    match &agent_token {
                        Some(token) => {
                            // Validate the JWT token
                            use tunnel_auth::JwtValidator;
                            let validator = JwtValidator::new(secret.as_bytes());

                            match validator.validate(token) {
                                Ok(claims) => {
                                    tracing::info!(
                                        agent_id = %agent_id,
                                        stream_id = stream_id,
                                        tunnel_id = %claims.sub,
                                        "Agent token validated successfully"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        agent_id = %agent_id,
                                        stream_id = stream_id,
                                        error = %e,
                                        "Agent token validation failed"
                                    );

                                    // Send reject message
                                    let reject_msg = TunnelMessage::ForwardReject {
                                        tunnel_id: tunnel_id.clone(),
                                        stream_id,
                                        reason: format!(
                                            "Authentication failed: invalid agent token: {}",
                                            e
                                        ),
                                    };
                                    let _ = stream.send_message(&reject_msg).await;

                                    return Err(AgentError::MessageHandling(format!(
                                        "Token validation failed: {}",
                                        e
                                    )));
                                }
                            }
                        }
                        None => {
                            tracing::warn!(
                                agent_id = %agent_id,
                                stream_id = stream_id,
                                "Agent token is missing but jwt_secret is configured"
                            );

                            // Send reject message
                            let reject_msg = TunnelMessage::ForwardReject {
                                tunnel_id: tunnel_id.clone(),
                                stream_id,
                                reason: "Authentication failed: agent token is required"
                                    .to_string(),
                            };
                            let _ = stream.send_message(&reject_msg).await;

                            return Err(AgentError::MessageHandling(
                                "Agent token is required but not provided".to_string(),
                            ));
                        }
                    }
                }

                // Check exact address match
                if remote_address != target_address {
                    tracing::warn!(
                        agent_id = %agent_id,
                        stream_id = stream_id,
                        remote_address = %remote_address,
                        target_address = %target_address,
                        "Address mismatch: requested address does not match agent's target address"
                    );

                    // Send reject message
                    let reject_msg = TunnelMessage::ForwardReject {
                        tunnel_id: tunnel_id.clone(),
                        stream_id,
                        reason: format!(
                            "Address mismatch: this agent only forwards to {}, but {} was requested",
                            target_address, remote_address
                        ),
                    };
                    let _ = stream.send_message(&reject_msg).await;

                    return Err(AgentError::Forwarder(ForwarderError::AddressNotAllowed(
                        remote_address,
                    )));
                }

                // Send accept message
                let accept_msg = TunnelMessage::ForwardAccept {
                    tunnel_id: tunnel_id.clone(),
                    stream_id,
                };
                stream.send_message(&accept_msg).await?;

                // Register connection
                let info = ConnectionInfo {
                    tunnel_id: tunnel_id.clone(),
                    stream_id,
                    remote_address: remote_address.clone(),
                    established_at: std::time::Instant::now(),
                };
                connection_manager.register(stream_id, info).await;

                // Forward traffic
                let result = forwarder
                    .forward(tunnel_id, stream_id, remote_address, stream)
                    .await;

                // Unregister connection
                connection_manager.unregister(stream_id).await;

                result.map_err(AgentError::Forwarder)
            }
            Some(msg) => {
                tracing::warn!(
                    agent_id = %agent_id,
                    stream_id = stream_id,
                    message = ?msg,
                    "Unexpected message on data stream (expected ForwardRequest)"
                );
                Ok(())
            }
            None => {
                tracing::debug!(
                    agent_id = %agent_id,
                    stream_id = stream_id,
                    "Data stream closed by remote before receiving ForwardRequest"
                );
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert!(!config.agent_id.is_empty());
        assert_eq!(config.relay_addr, "localhost:4443");
    }

    #[test]
    fn test_agent_new_valid_config() {
        let config = AgentConfig {
            agent_id: "test-agent".to_string(),
            relay_addr: "localhost:4443".to_string(),
            auth_token: "token".to_string(),
            target_address: "192.168.1.100:8080".to_string(),
            insecure: false,
            local_address: None,
            jwt_secret: None,
        };

        let agent = Agent::new(config);
        assert!(agent.is_ok());
    }

    #[test]
    fn test_agent_new_invalid_target_address() {
        let config = AgentConfig {
            agent_id: "test-agent".to_string(),
            relay_addr: "localhost:4443".to_string(),
            auth_token: "token".to_string(),
            target_address: "invalid-address-no-port".to_string(),
            insecure: false,
            local_address: None,
            jwt_secret: None,
        };

        let agent = Agent::new(config);
        assert!(agent.is_err());
    }

    #[tokio::test]
    async fn test_agent_is_running() {
        let config = AgentConfig::default();
        let agent = Agent::new(config).unwrap();

        assert!(!agent.is_running().await);
    }
}
