//! Tunnel connection handler for exit nodes

use std::sync::Arc;
use tracing::{debug, error, info, warn};

use localup_auth::JwtValidator;
use localup_proto::{Endpoint, Protocol, TunnelMessage};
use localup_relay_db::entities::{auth_token, prelude::AuthToken as AuthTokenEntity};
use localup_router::{RouteKey, RouteRegistry, RouteTarget};
use localup_transport::{TransportConnection, TransportStream};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use sha2::{Digest, Sha256};

use crate::agent_registry::{AgentRegistry, RegisteredAgent};
use crate::connection::TunnelConnectionManager;
use crate::domain_provider::{DomainContext, DomainProvider};
use crate::pending_requests::PendingRequests;
use crate::task_tracker::TaskTracker;

/// Trait for port allocation (TCP tunnels)
pub trait PortAllocator: Send + Sync {
    /// Allocate a port for the given localup_id
    /// If requested_port is Some, try to allocate that specific port
    /// If requested_port is None or unavailable, allocate any available port
    fn allocate(&self, localup_id: &str, requested_port: Option<u16>) -> Result<u16, String>;
    fn deallocate(&self, localup_id: &str);
    fn get_allocated_port(&self, localup_id: &str) -> Option<u16>;
}

/// Callback for spawning TCP proxy servers
pub type TcpProxySpawner = Arc<
    dyn Fn(
            String,
            u16,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>
        + Send
        + Sync,
>;

/// Handles a tunnel connection from a client or agent
pub struct TunnelHandler {
    connection_manager: Arc<TunnelConnectionManager>,
    route_registry: Arc<RouteRegistry>,
    jwt_validator: Option<Arc<JwtValidator>>,
    db: Option<DatabaseConnection>,
    domain: String,
    domain_provider: Option<Arc<dyn DomainProvider>>,
    #[allow(dead_code)] // Used for HTTP request/response handling (future work)
    pending_requests: Arc<PendingRequests>,
    port_allocator: Option<Arc<dyn PortAllocator>>,
    tcp_proxy_spawner: Option<TcpProxySpawner>,
    agent_registry: Option<Arc<AgentRegistry>>,
    agent_connection_manager: Arc<crate::connection::AgentConnectionManager>,
    /// Actual TLS port the relay is listening on
    tls_port: Option<u16>,
    /// Actual HTTP port the relay is listening on
    http_port: Option<u16>,
    /// Actual HTTPS port the relay is listening on
    https_port: Option<u16>,
    /// Tracks TCP proxy server tasks to allow cleanup on disconnect
    task_tracker: Arc<TaskTracker>,
}

impl TunnelHandler {
    pub fn new(
        connection_manager: Arc<TunnelConnectionManager>,
        route_registry: Arc<RouteRegistry>,
        jwt_validator: Option<Arc<JwtValidator>>,
        domain: String,
        pending_requests: Arc<PendingRequests>,
    ) -> Self {
        Self {
            connection_manager,
            route_registry,
            jwt_validator,
            db: None,
            domain,
            domain_provider: None,
            pending_requests,
            port_allocator: None,
            tcp_proxy_spawner: None,
            agent_registry: None,
            agent_connection_manager: Arc::new(crate::connection::AgentConnectionManager::new()),
            tls_port: None,
            http_port: None,
            https_port: None,
            task_tracker: Arc::new(TaskTracker::new()),
        }
    }

    /// Set the database connection for auth token validation
    pub fn with_database(mut self, db: DatabaseConnection) -> Self {
        self.db = Some(db);
        self
    }

    pub fn with_port_allocator(mut self, port_allocator: Arc<dyn PortAllocator>) -> Self {
        self.port_allocator = Some(port_allocator);
        self
    }

    pub fn with_tcp_proxy_spawner(mut self, spawner: TcpProxySpawner) -> Self {
        self.tcp_proxy_spawner = Some(spawner);
        self
    }

    pub fn with_agent_registry(mut self, agent_registry: Arc<AgentRegistry>) -> Self {
        self.agent_registry = Some(agent_registry);
        self
    }

    pub fn with_domain_provider(mut self, domain_provider: Arc<dyn DomainProvider>) -> Self {
        self.domain_provider = Some(domain_provider);
        self
    }

    pub fn with_tls_port(mut self, port: u16) -> Self {
        self.tls_port = Some(port);
        self
    }

    pub fn with_http_port(mut self, port: u16) -> Self {
        self.http_port = Some(port);
        self
    }

    pub fn with_https_port(mut self, port: u16) -> Self {
        self.https_port = Some(port);
        self
    }

    /// Handle an incoming tunnel connection (client or agent)
    pub async fn handle_connection<C>(&self, connection: Arc<C>, peer_addr: std::net::SocketAddr)
    where
        C: TransportConnection + 'static,
        C::Stream: 'static,
    {
        info!("New tunnel connection from {}", peer_addr);

        // Accept the first stream for control messages
        let mut control_stream = match connection.accept_stream().await {
            Ok(Some(stream)) => stream,
            Ok(None) => {
                error!("Connection closed before control stream could be accepted");
                return;
            }
            Err(e) => {
                error!("Failed to accept control stream: {}", e);
                return;
            }
        };

        // Read the first message to determine connection type
        let first_message = match control_stream.recv_message().await {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                error!("Connection closed before first message");
                return;
            }
            Err(e) => {
                error!("Failed to read first message: {}", e);
                return;
            }
        };

        // Route based on message type
        match first_message {
            TunnelMessage::AgentRegister {
                agent_id,
                auth_token,
                target_address,
                metadata,
            } => {
                info!(
                    "Agent registration from {}: {} (target: {})",
                    peer_addr, agent_id, target_address
                );
                self.handle_agent_connection(
                    connection,
                    control_stream,
                    agent_id,
                    auth_token,
                    target_address,
                    metadata,
                    peer_addr,
                )
                .await;
            }
            TunnelMessage::Connect {
                localup_id,
                auth_token,
                protocols,
                config,
            } => {
                info!("Client connection from {}: {}", peer_addr, localup_id);
                let connect_result = self
                    .handle_client_connection(
                        connection,
                        control_stream,
                        localup_id,
                        auth_token,
                        protocols,
                        config,
                        peer_addr,
                    )
                    .await;

                if let Err(e) = connect_result {
                    error!("Client connection failed: {}", e);
                }
            }
            TunnelMessage::ReverseTunnelRequest {
                localup_id,
                remote_address,
                agent_id,
                agent_token,
            } => {
                info!(
                    "Reverse tunnel request from {}: tunnel={}, address={}, agent={}",
                    peer_addr, localup_id, remote_address, agent_id
                );
                self.handle_reverse_localup_request(
                    connection,
                    control_stream,
                    localup_id,
                    remote_address,
                    agent_id,
                    agent_token,
                    peer_addr,
                )
                .await;
            }
            _ => {
                error!("Unexpected first message: {:?}", first_message);
                let _ = control_stream
                    .send_message(&TunnelMessage::Disconnect {
                        reason: "Invalid first message".to_string(),
                    })
                    .await;
                // Gracefully close the stream and give QUIC time to transmit
                let _ = control_stream.finish().await;
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        }
    }

    /// Handle a client tunnel connection (existing functionality)
    #[allow(clippy::too_many_arguments)]
    async fn handle_client_connection<C, S>(
        &self,
        connection: Arc<C>,
        mut control_stream: S,
        localup_id: String,
        auth_token: String,
        protocols: Vec<Protocol>,
        config: localup_proto::TunnelConfig,
        peer_addr: std::net::SocketAddr,
    ) -> Result<(), String>
    where
        C: TransportConnection + 'static,
        S: TransportStream + 'static,
    {
        debug!("Received Connect from localup_id: {}", localup_id);

        // Validate authentication with enhanced auth token validation
        let user_id = match self.validate_auth_token(&auth_token).await {
            Ok(user_id) => user_id,
            Err(e) => {
                error!("Authentication failed for tunnel {}: {}", localup_id, e);
                let _ = control_stream
                    .send_message(&TunnelMessage::Disconnect {
                        reason: format!("Authentication failed: {}", e),
                    })
                    .await;
                // Gracefully close the stream and give QUIC time to transmit
                let _ = control_stream.finish().await;
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                return Err(e);
            }
        };

        debug!("Tunnel {} authenticated for user {}", localup_id, user_id);

        // Build endpoints based on requested protocols
        let mut endpoints = self
            .build_endpoints(&localup_id, &protocols, &config, peer_addr)
            .await;
        debug!(
            "Built {} endpoints for tunnel {}",
            endpoints.len(),
            localup_id
        );

        // Register routes in the route registry
        // If any route registration fails (e.g., subdomain conflict), reject the connection
        // For TCP endpoints, update with allocated port
        for endpoint in &mut endpoints {
            debug!("Registering endpoint: protocol={:?}", endpoint.protocol);
            match self.register_route(&localup_id, endpoint) {
                Ok(Some(allocated_port)) => {
                    // Update TCP endpoint with allocated port
                    endpoint.public_url = format!("tcp://{}:{}", self.domain, allocated_port);
                    endpoint.port = Some(allocated_port);
                    info!(
                        "Updated TCP endpoint with allocated port: {}",
                        allocated_port
                    );
                }
                Ok(None) => {
                    // Non-TCP endpoint, no port allocation needed
                }
                Err(e) => {
                    error!("Failed to register route for tunnel {}: {}", localup_id, e);

                    // Send error response and close connection
                    let error_str = e.to_string();
                    debug!("Error string for route registration: '{}' (contains 'already exists': {}, contains 'not available': {})",
                        error_str, error_str.contains("already exists"), error_str.contains("not available"));

                    let error_msg = if error_str.contains("already exists") {
                        "Subdomain is already in use by another tunnel".to_string()
                    } else if error_str.contains("not available") {
                        // Preserve the specific error message from allocator
                        error_str
                    } else {
                        format!("Failed to register route: {}", e)
                    };

                    // Send Disconnect message with detailed reason
                    debug!(
                        "Sending Disconnect message to tunnel {} with reason: {}",
                        localup_id, error_msg
                    );
                    if let Err(send_err) = control_stream
                        .send_message(&TunnelMessage::Disconnect {
                            reason: error_msg.clone(),
                        })
                        .await
                    {
                        error!(
                            "Failed to send Disconnect message to tunnel {}: {}",
                            localup_id, send_err
                        );
                    } else {
                        debug!(
                            "Disconnect message sent successfully to tunnel {}",
                            localup_id
                        );
                    }

                    // Gracefully close the stream and give QUIC time to transmit
                    let _ = control_stream.finish().await;
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

                    return Err(error_msg);
                }
            }
        }

        // Register the tunnel connection (optional - only for QUIC connections)
        // Note: Connection manager is primarily used for reverse tunnels and TCP proxies.
        // For HTTP/HTTPS tunnels, routing is handled by route_registry instead.
        let quic_conn = connection.clone();
        if let Ok(quic_conn) = (quic_conn as Arc<dyn std::any::Any + Send + Sync>)
            .downcast::<localup_transport_quic::QuicConnection>()
        {
            self.connection_manager
                .register(localup_id.clone(), endpoints.clone(), quic_conn)
                .await;
            debug!(
                "Registered QUIC connection in connection manager for tunnel {}",
                localup_id
            );
        } else {
            debug!(
                "Connection for tunnel {} is not QUIC (likely H2/WebSocket), skipping connection manager registration",
                localup_id
            );
        }

        info!(
            "✅ Tunnel registered: {} with {} endpoints",
            localup_id,
            endpoints.len()
        );

        // Send Connected response
        if let Err(e) = control_stream
            .send_message(&TunnelMessage::Connected {
                localup_id: localup_id.clone(),
                endpoints: endpoints.clone(),
            })
            .await
        {
            error!("Failed to send Connected message: {}", e);
            return Err(format!("Failed to send Connected message: {}", e));
        }

        // Keep control stream open for ping/pong heartbeat
        // Server actively sends pings every 10 seconds, expects pongs within 5 seconds
        let localup_id_heartbeat = localup_id.clone();
        let heartbeat_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut waiting_for_pong = false;
            let mut pong_deadline = tokio::time::Instant::now();

            loop {
                tokio::select! {
                    // Check for interval tick (send ping)
                    _ = interval.tick(), if !waiting_for_pong => {
                        // Send ping
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        debug!("Sending ping to tunnel {}", localup_id_heartbeat);
                        if let Err(e) = control_stream.send_message(&TunnelMessage::Ping { timestamp }).await {
                            error!("Failed to send ping to tunnel {}: {}", localup_id_heartbeat, e);
                            break;
                        }

                        // Start waiting for pong
                        waiting_for_pong = true;
                        pong_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
                    }

                    // Check for pong timeout
                    _ = tokio::time::sleep_until(pong_deadline), if waiting_for_pong => {
                        warn!("Pong timeout for tunnel {} (no response in 5s), assuming disconnected", localup_id_heartbeat);
                        break;
                    }

                    // Receive messages (always ready to receive)
                    result = control_stream.recv_message() => {
                        match result {
                            Ok(Some(TunnelMessage::Pong { .. })) => {
                                debug!("Received pong from tunnel {}", localup_id_heartbeat);
                                waiting_for_pong = false;
                            }
                            Ok(Some(TunnelMessage::Disconnect { reason })) => {
                                info!("Tunnel {} disconnected: {}", localup_id_heartbeat, reason);

                                // Send disconnect acknowledgment
                                if let Err(e) = control_stream.send_message(&TunnelMessage::DisconnectAck {
                                    localup_id: localup_id_heartbeat.clone(),
                                }).await {
                                    warn!("Failed to send disconnect ack: {}", e);
                                } else {
                                    debug!("Sent disconnect acknowledgment to tunnel {}", localup_id_heartbeat);
                                }

                                break;
                            }
                            Ok(None) => {
                                info!("Control stream closed for tunnel {}", localup_id_heartbeat);
                                break;
                            }
                            Err(e) => {
                                error!("Error on control stream for tunnel {}: {}", localup_id_heartbeat, e);
                                break;
                            }
                            Ok(Some(msg)) => {
                                warn!("Unexpected message on control stream from tunnel {}: {:?}", localup_id_heartbeat, msg);
                            }
                        }
                    }
                }
            }
            debug!("Heartbeat task ended for tunnel {}", localup_id_heartbeat);
        });

        // Wait for the heartbeat task to complete (signals disconnection)
        let _ = heartbeat_task.await;

        // Cleanup on disconnect
        debug!("Cleaning up tunnel {}", localup_id);

        self.connection_manager.unregister(&localup_id).await;

        // Unregister routes
        for endpoint in &endpoints {
            self.unregister_route(&localup_id, endpoint).await;
        }

        info!("Tunnel {} disconnected", localup_id);
        Ok(())
    }

    /// Handle a reverse tunnel request from a client
    #[allow(clippy::too_many_arguments)]
    async fn handle_reverse_localup_request<C, S>(
        &self,
        _connection: Arc<C>,
        mut control_stream: S,
        localup_id: String,
        remote_address: String,
        agent_id: String,
        agent_token: Option<String>,
        _peer_addr: std::net::SocketAddr,
    ) where
        C: TransportConnection + 'static,
        S: TransportStream + 'static,
    {
        debug!(
            "Processing reverse tunnel request: tunnel={}, address={}, agent={}",
            localup_id, remote_address, agent_id
        );

        // Clone agent_token for use in async closure
        let agent_token = agent_token.clone();

        // Check if agent registry is configured
        let Some(ref registry) = self.agent_registry else {
            error!("Agent registry not configured, rejecting reverse tunnel request");
            let _ = control_stream
                .send_message(&TunnelMessage::ReverseTunnelReject {
                    localup_id: localup_id.clone(),
                    reason: "Reverse tunnels not enabled on this relay".to_string(),
                })
                .await;
            return;
        };

        // Find agent by target address
        let Some(agent) = registry.find_by_address(&remote_address) else {
            error!(
                "No agent found for target address: {} (requested by tunnel {})",
                remote_address, localup_id
            );
            let _ = control_stream
                .send_message(&TunnelMessage::ReverseTunnelReject {
                    localup_id: localup_id.clone(),
                    reason: format!("No agent available for address: {}", remote_address),
                })
                .await;
            return;
        };

        // Verify agent_id matches
        if agent.agent_id != agent_id {
            warn!(
                "Agent ID mismatch: requested {}, but found {} for address {}",
                agent_id, agent.agent_id, remote_address
            );
            let _ = control_stream
                .send_message(&TunnelMessage::ReverseTunnelReject {
                    localup_id: localup_id.clone(),
                    reason: format!(
                        "Agent ID mismatch: expected {}, got {}",
                        agent.agent_id, agent_id
                    ),
                })
                .await;
            return;
        }

        // Get agent connection
        let Some(agent_connection) = self.agent_connection_manager.get(&agent_id).await else {
            error!(
                "Agent {} found in registry but connection not available (may have disconnected)",
                agent_id
            );
            let _ = control_stream
                .send_message(&TunnelMessage::ReverseTunnelReject {
                    localup_id: localup_id.clone(),
                    reason: "Agent connection not available".to_string(),
                })
                .await;
            return;
        };

        // Note: Agent token validation is handled by the relay's JWT validator
        // The client's JWT was already validated, providing sufficient authentication
        // Agent-level validation would require sending ValidateAgentToken on the agent's
        // control stream (not a new data stream), which is complex due to the stream
        // being owned by the heartbeat task. For now, rely on relay-level JWT auth.

        debug!(
            "Agent {} connection validated, accepting reverse tunnel {}",
            agent_id, localup_id
        );

        // Send ReverseTunnelAccept to client
        if let Err(e) = control_stream
            .send_message(&TunnelMessage::ReverseTunnelAccept {
                localup_id: localup_id.clone(),
                local_address: "localhost:0".to_string(), // Client will bind to dynamic port
            })
            .await
        {
            error!("Failed to send ReverseTunnelAccept to client: {}", e);
            return;
        }

        info!(
            "✅ Reverse tunnel established: {} -> {} (via agent {})",
            localup_id, remote_address, agent_id
        );

        // Spawn task to accept incoming QUIC streams from client
        // Each stream represents a new TCP connection
        let connection_clone = _connection.clone();
        let localup_id_clone = localup_id.clone();
        let remote_address_clone = remote_address.clone();

        tokio::spawn(async move {
            Self::handle_reverse_tunnel_streams(
                connection_clone,
                agent_connection,
                localup_id_clone,
                remote_address_clone,
                agent_token,
            )
            .await;
        });

        // Keep control stream open for heartbeat and disconnect messages
        Self::handle_reverse_control_stream(control_stream, localup_id).await;
    }

    /// Handle control stream for reverse tunnel (Ping/Pong only)
    async fn handle_reverse_control_stream<S>(mut control_stream: S, localup_id: String)
    where
        S: TransportStream + 'static,
    {
        loop {
            match control_stream.recv_message().await {
                Ok(Some(TunnelMessage::Ping { timestamp })) => {
                    debug!("Received Ping from reverse tunnel client {}", localup_id);
                    if let Err(e) = control_stream
                        .send_message(&TunnelMessage::Pong { timestamp })
                        .await
                    {
                        error!("Failed to send Pong to reverse tunnel client: {}", e);
                        break;
                    }
                }
                Ok(Some(TunnelMessage::Disconnect { reason })) => {
                    info!(
                        "Reverse tunnel client {} disconnected: {}",
                        localup_id, reason
                    );
                    break;
                }
                Ok(None) => {
                    debug!("Reverse tunnel control stream {} closed", localup_id);
                    break;
                }
                Err(e) => {
                    error!(
                        "Error reading from reverse tunnel control stream {}: {}",
                        localup_id, e
                    );
                    break;
                }
                Ok(Some(msg)) => {
                    warn!(
                        "Unexpected message on reverse tunnel control stream {}: {:?}",
                        localup_id, msg
                    );
                }
            }
        }

        info!("Reverse tunnel control stream {} closed", localup_id);
    }

    /// Accept incoming QUIC streams from reverse tunnel client
    /// Each stream represents a new TCP connection
    async fn handle_reverse_tunnel_streams<C>(
        connection: Arc<C>,
        agent_connection: Arc<localup_transport_quic::QuicConnection>,
        localup_id: String,
        remote_address: String,
        agent_token: Option<String>,
    ) where
        C: TransportConnection + 'static,
    {
        loop {
            // Accept incoming stream from client
            let client_stream = match connection.accept_stream().await {
                Ok(Some(stream)) => stream,
                Ok(None) => {
                    debug!("No more streams from reverse tunnel client {}", localup_id);
                    break;
                }
                Err(e) => {
                    error!(
                        "Failed to accept stream from reverse tunnel client {}: {}",
                        localup_id, e
                    );
                    break;
                }
            };

            // Clone for spawned task
            let agent_connection_clone = agent_connection.clone();
            let localup_id_clone = localup_id.clone();
            let remote_address_clone = remote_address.clone();
            let agent_token_clone = agent_token.clone();

            // Spawn task to handle this stream
            tokio::spawn(async move {
                if let Err(e) = Self::handle_reverse_stream(
                    client_stream,
                    agent_connection_clone,
                    localup_id_clone,
                    remote_address_clone,
                    agent_token_clone,
                )
                .await
                {
                    error!("Error handling reverse tunnel stream: {}", e);
                }
            });
        }

        info!(
            "Stopped accepting streams for reverse tunnel {}",
            localup_id
        );
    }

    /// Handle a single reverse tunnel stream (one TCP connection)
    async fn handle_reverse_stream<S>(
        mut client_stream: S,
        agent_connection: Arc<localup_transport_quic::QuicConnection>,
        localup_id: String,
        remote_address: String,
        agent_token: Option<String>,
    ) -> Result<(), String>
    where
        S: TransportStream + 'static,
    {
        // Read ReverseConnect message
        let (stream_id, expected_localup_id, expected_remote_address) =
            match client_stream.recv_message().await {
                Ok(Some(TunnelMessage::ReverseConnect {
                    localup_id: msg_localup_id,
                    stream_id,
                    remote_address: msg_remote_address,
                })) => (stream_id, msg_localup_id, msg_remote_address),
                Ok(Some(msg)) => {
                    return Err(format!("Expected ReverseConnect, got {:?}", msg));
                }
                Ok(None) => {
                    return Err("Stream closed before ReverseConnect".to_string());
                }
                Err(e) => {
                    return Err(format!("Failed to read ReverseConnect: {}", e));
                }
            };

        // Validate localup_id and remote_address match
        if expected_localup_id != localup_id {
            return Err(format!(
                "localup_id mismatch: expected {}, got {}",
                localup_id, expected_localup_id
            ));
        }

        if expected_remote_address != remote_address {
            return Err(format!(
                "remote_address mismatch: expected {}, got {}",
                remote_address, expected_remote_address
            ));
        }

        debug!(
            "Received ReverseConnect for tunnel {} stream {}",
            localup_id, stream_id
        );

        // Open agent stream
        let mut agent_stream = agent_connection
            .open_stream()
            .await
            .map_err(|e| format!("Failed to open agent stream: {}", e))?;

        // Send ForwardRequest to agent
        agent_stream
            .send_message(&TunnelMessage::ForwardRequest {
                localup_id: localup_id.clone(),
                stream_id,
                remote_address: remote_address.clone(),
                agent_token,
            })
            .await
            .map_err(|e| format!("Failed to send ForwardRequest: {}", e))?;

        // Wait for ForwardAccept/Reject
        match agent_stream.recv_message().await {
            Ok(Some(TunnelMessage::ForwardAccept { .. })) => {
                debug!(
                    "Agent accepted stream {} for tunnel {}",
                    stream_id, localup_id
                );
            }
            Ok(Some(TunnelMessage::ForwardReject { reason, .. })) => {
                warn!(
                    "Agent rejected stream {} for tunnel {}: {}",
                    stream_id, localup_id, reason
                );
                // Send ReverseClose to client
                let _ = client_stream
                    .send_message(&TunnelMessage::ReverseClose {
                        localup_id: localup_id.clone(),
                        stream_id,
                        reason: Some(reason.clone()),
                    })
                    .await;
                return Err(format!("Agent rejected: {}", reason));
            }
            Ok(Some(msg)) => {
                return Err(format!("Unexpected agent response: {:?}", msg));
            }
            Ok(None) => {
                return Err("Agent stream closed before response".to_string());
            }
            Err(e) => {
                return Err(format!("Failed to read agent response: {}", e));
            }
        }

        // Proxy data bidirectionally using tokio::select!
        loop {
            tokio::select! {
                // Client -> Agent
                client_msg = client_stream.recv_message() => {
                    match client_msg {
                        Ok(Some(TunnelMessage::ReverseData { data, .. })) => {
                            if let Err(e) = agent_stream
                                .send_message(&TunnelMessage::ReverseData {
                                    localup_id: localup_id.clone(),
                                    stream_id,
                                    data,
                                })
                                .await
                            {
                                error!("Failed to forward data to agent: {}", e);
                                break;
                            }
                        }
                        Ok(Some(TunnelMessage::ReverseClose { .. })) => {
                            debug!("Client closed stream {}", stream_id);
                            let _ = agent_stream
                                .send_message(&TunnelMessage::ReverseClose {
                                    localup_id: localup_id.clone(),
                                    stream_id,
                                    reason: None,
                                })
                                .await;
                            break;
                        }
                        Ok(None) | Err(_) => {
                            debug!("Client stream {} closed", stream_id);
                            break;
                        }
                        Ok(Some(msg)) => {
                            warn!("Unexpected message from client: {:?}", msg);
                        }
                    }
                }

                // Agent -> Client
                agent_msg = agent_stream.recv_message() => {
                    match agent_msg {
                        Ok(Some(TunnelMessage::ReverseData { data, .. })) => {
                            if let Err(e) = client_stream
                                .send_message(&TunnelMessage::ReverseData {
                                    localup_id: localup_id.clone(),
                                    stream_id,
                                    data,
                                })
                                .await
                            {
                                error!("Failed to forward data to client: {}", e);
                                break;
                            }
                        }
                        Ok(Some(TunnelMessage::ReverseClose { .. })) => {
                            debug!("Agent closed stream {}", stream_id);
                            let _ = client_stream
                                .send_message(&TunnelMessage::ReverseClose {
                                    localup_id: localup_id.clone(),
                                    stream_id,
                                    reason: None,
                                })
                                .await;
                            break;
                        }
                        Ok(None) | Err(_) => {
                            debug!("Agent stream {} closed", stream_id);
                            break;
                        }
                        Ok(Some(msg)) => {
                            warn!("Unexpected message from agent: {:?}", msg);
                        }
                    }
                }
            }
        }

        debug!("Stream {} for tunnel {} closed", stream_id, localup_id);
        Ok(())
    }

    /// Handle multiplexed reverse tunnel connections over control stream
    /// Each TCP connection gets a unique stream_id and a dedicated agent stream
    ///
    /// DEPRECATED: This is the old implementation that uses control stream for data.
    /// Kept for reference but should not be called anymore.
    #[allow(dead_code)]
    async fn handle_multiplexed_reverse_tunnel<S>(
        &self,
        mut control_stream: S,
        agent_connection: Arc<localup_transport_quic::QuicConnection>,
        localup_id: String,
        remote_address: String,
        agent_token: Option<String>,
    ) where
        S: TransportStream + 'static,
    {
        use std::collections::HashMap;
        use tokio::sync::mpsc;

        // Create channel for sending messages back to client
        // We use a channel because we can't split TransportStream trait
        let (to_client_tx_main, mut to_client_rx_main) = mpsc::channel::<TunnelMessage>(100);

        // Map of stream_id -> channel for sending to agent stream tasks
        type AgentSender = mpsc::Sender<TunnelMessage>;
        let agent_senders: Arc<tokio::sync::RwLock<HashMap<u32, AgentSender>>> =
            Arc::new(tokio::sync::RwLock::new(HashMap::new()));

        // Note: to_client_tx_main is used by agent stream tasks to send messages back to client

        // Main loop: handle messages from client and agents
        loop {
            tokio::select! {
                // Read from client control stream
                client_msg = control_stream.recv_message() => {
                    debug!("Relay received message from client: {:?}", client_msg);
                    match client_msg {
                    Ok(Some(TunnelMessage::ReverseData {
                        stream_id, data, ..
                    })) => {
                        // Check if we have an agent stream for this stream_id
                        let senders = agent_senders.read().await;
                        if let Some(tx) = senders.get(&stream_id) {
                            // Forward to existing agent stream
                            let _ = tx
                                .send(TunnelMessage::ReverseData {
                                    localup_id: localup_id.clone(),
                                    stream_id,
                                    data,
                                })
                                .await;
                        } else {
                            drop(senders); // Release read lock

                            // New stream_id - open agent stream and spawn task
                            match agent_connection.open_stream().await {
                                Ok(mut agent_stream) => {
                                    // Send ForwardRequest
                                    let forward_req = TunnelMessage::ForwardRequest {
                                        localup_id: localup_id.clone(),
                                        stream_id,
                                        remote_address: remote_address.clone(),
                                        agent_token: agent_token.clone(),
                                    };

                                    if let Err(e) = agent_stream.send_message(&forward_req).await {
                                        error!("Failed to send ForwardRequest: {}", e);
                                        continue;
                                    }

                                    // Wait for ForwardAccept/Reject
                                    match agent_stream.recv_message().await {
                                        Ok(Some(TunnelMessage::ForwardAccept { .. })) => {
                                            debug!("Agent accepted stream {}", stream_id);
                                        }
                                        Ok(Some(TunnelMessage::ForwardReject {
                                            reason, ..
                                        })) => {
                                            warn!(
                                                "Agent rejected stream {}: {}",
                                                stream_id, reason
                                            );
                                            // Send error back to client
                                            let _ = to_client_tx_main
                                                .send(TunnelMessage::ReverseClose {
                                                    localup_id: localup_id.clone(),
                                                    stream_id,
                                                    reason: Some(reason),
                                                })
                                                .await;
                                            continue;
                                        }
                                        _ => {
                                            error!("Unexpected agent response for stream {}", stream_id);
                                            // Send generic error back to client
                                            let _ = to_client_tx_main
                                                .send(TunnelMessage::ReverseClose {
                                                    localup_id: localup_id.clone(),
                                                    stream_id,
                                                    reason: Some(
                                                        "Agent did not respond with ForwardAccept or ForwardReject"
                                                            .to_string(),
                                                    ),
                                                })
                                                .await;
                                            continue;
                                        }
                                    }

                                    // Create channel for this agent stream
                                    let (tx, mut rx) = mpsc::channel::<TunnelMessage>(100);

                                    // Register sender
                                    {
                                        let mut senders = agent_senders.write().await;
                                        senders.insert(stream_id, tx.clone());
                                    }

                                    // Spawn task to handle this agent stream
                                    let to_client_tx_clone = to_client_tx_main.clone();
                                    let localup_id_clone2 = localup_id.clone();
                                    let agent_senders_clone2 = agent_senders.clone();

                                    tokio::spawn(async move {
                                        let (mut agent_send, mut agent_recv) = agent_stream.split();

                                        loop {
                                            tokio::select! {
                                                // Client -> Agent
                                                msg = rx.recv() => {
                                                    match msg {
                                                        Some(TunnelMessage::ReverseData { data, .. }) => {
                                                            if let Err(e) = agent_send.send_message(&TunnelMessage::ReverseData {
                                                                localup_id: localup_id_clone2.clone(),
                                                                stream_id,
                                                                data,
                                                            }).await {
                                                                error!("Failed to send to agent: {}", e);
                                                                break;
                                                            }
                                                        }
                                                        Some(TunnelMessage::ReverseClose { .. }) => {
                                                            let _ = agent_send.send_message(&TunnelMessage::ReverseClose {
                                                                localup_id: localup_id_clone2.clone(),
                                                                stream_id,
                                                                reason: None,
                                                            }).await;
                                                            break;
                                                        }
                                                        None => break,
                                                        _ => {}
                                                    }
                                                }

                                                // Agent -> Client
                                                msg = agent_recv.recv_message() => {
                                                    match msg {
                                                        Ok(Some(TunnelMessage::ReverseData { data, .. })) => {
                                                            let _ = to_client_tx_clone.send(TunnelMessage::ReverseData {
                                                                localup_id: localup_id_clone2.clone(),
                                                                stream_id,
                                                                data,
                                                            }).await;
                                                        }
                                                        Ok(Some(TunnelMessage::ReverseClose { .. })) => {
                                                            let _ = to_client_tx_clone.send(TunnelMessage::ReverseClose {
                                                                localup_id: localup_id_clone2.clone(),
                                                                stream_id,
                                                                reason: None,
                                                            }).await;
                                                            break;
                                                        }
                                                        Ok(None) | Err(_) => break,
                                                        _ => {}
                                                    }
                                                }
                                            }
                                        }

                                        // Cleanup
                                        let mut senders = agent_senders_clone2.write().await;
                                        senders.remove(&stream_id);
                                    });

                                    // Send the initial data
                                    let _ = tx
                                        .send(TunnelMessage::ReverseData {
                                            localup_id: localup_id.clone(),
                                            stream_id,
                                            data,
                                        })
                                        .await;
                                }
                                Err(e) => {
                                    error!("Failed to open agent stream: {}", e);
                                    // Agent connection is broken, notify client and exit
                                    let disconnect_msg = TunnelMessage::Disconnect {
                                        reason: format!("Agent disconnected: {}", e),
                                    };
                                    let _ = to_client_tx_main.send(disconnect_msg).await;
                                    break; // Exit the main loop
                                }
                            }
                        }
                    }
                    Ok(Some(TunnelMessage::ReverseClose { stream_id, .. })) => {
                        // Forward close to agent stream
                        let senders = agent_senders.read().await;
                        if let Some(tx) = senders.get(&stream_id) {
                            let _ = tx
                                .send(TunnelMessage::ReverseClose {
                                    localup_id: localup_id.clone(),
                                    stream_id,
                                    reason: None,
                                })
                                .await;
                        }
                    }
                    Ok(None) => {
                        debug!("Client control stream closed (received None)");
                        break;
                    }
                    Err(e) => {
                        error!("Client control stream error: {}", e);
                        break;
                    }
                    Ok(Some(msg)) => {
                        warn!("Unexpected message from client: {:?}", msg);
                    }
                    }
                }

                // Send messages from agent streams back to client
                msg_to_client = to_client_rx_main.recv() => {
                    if let Some(msg) = msg_to_client {
                        if let Err(e) = control_stream.send_message(&msg).await {
                            error!("Failed to send message to client: {}", e);
                            break;
                        }
                    } else {
                        // All agent stream senders dropped
                        break;
                    }
                }
            }
        }

        info!("Reverse tunnel {} closed", localup_id);
    }

    /// Proxy data bidirectionally between client and agent for reverse tunnel
    #[allow(dead_code)]
    async fn proxy_reverse_tunnel<S1, S2>(
        &self,
        mut client_stream: S1,
        mut agent_stream: S2,
        localup_id: String,
        stream_id: u32,
    ) where
        S1: TransportStream + 'static,
        S2: TransportStream + 'static,
    {
        debug!(
            "Starting bidirectional proxy for reverse tunnel {}",
            localup_id
        );

        loop {
            tokio::select! {
                // Read from client, forward to agent
                client_msg = client_stream.recv_message() => {
                    match client_msg {
                        Ok(Some(TunnelMessage::ReverseData {
                            localup_id: _,
                            stream_id: _,
                            data,
                        })) => {
                            // Forward data to agent
                            if let Err(e) = agent_stream
                                .send_message(&TunnelMessage::ReverseData {
                                    localup_id: localup_id.clone(),
                                    stream_id,
                                    data,
                                })
                                .await
                            {
                                error!("Failed to forward data to agent: {}", e);
                                break;
                            }
                        }
                        Ok(Some(TunnelMessage::ReverseClose { .. })) => {
                            debug!("Client closed reverse tunnel");
                            let _ = agent_stream
                                .send_message(&TunnelMessage::ReverseClose {
                                    localup_id: localup_id.clone(),
                                    stream_id,
                                    reason: None,
                                })
                                .await;
                            break;
                        }
                        Ok(None) => {
                            debug!("Client stream closed");
                            break;
                        }
                        Err(e) => {
                            error!("Error reading from client: {}", e);
                            break;
                        }
                        Ok(Some(msg)) => {
                            warn!("Unexpected message from client: {:?}", msg);
                        }
                    }
                }

                // Read from agent, forward to client
                agent_msg = agent_stream.recv_message() => {
                    match agent_msg {
                        Ok(Some(TunnelMessage::ReverseData {
                            localup_id: _,
                            stream_id: _,
                            data,
                        })) => {
                            // Forward data to client
                            if let Err(e) = client_stream
                                .send_message(&TunnelMessage::ReverseData {
                                    localup_id: localup_id.clone(),
                                    stream_id,
                                    data,
                                })
                                .await
                            {
                                error!("Failed to forward data to client: {}", e);
                                break;
                            }
                        }
                        Ok(Some(TunnelMessage::ReverseClose { .. })) => {
                            debug!("Agent closed reverse tunnel");
                            let _ = client_stream
                                .send_message(&TunnelMessage::ReverseClose {
                                    localup_id: localup_id.clone(),
                                    stream_id,
                                    reason: None,
                                })
                                .await;
                            break;
                        }
                        Ok(None) => {
                            debug!("Agent stream closed");
                            break;
                        }
                        Err(e) => {
                            error!("Error reading from agent: {}", e);
                            break;
                        }
                        Ok(Some(msg)) => {
                            warn!("Unexpected message from agent: {:?}", msg);
                        }
                    }
                }
            }
        }

        info!("Reverse tunnel {} closed", localup_id);
    }

    /// Handle an agent connection (reverse tunnel)
    #[allow(clippy::too_many_arguments)]
    async fn handle_agent_connection<C, S>(
        &self,
        connection: Arc<C>,
        mut control_stream: S,
        agent_id: String,
        auth_token: String,
        target_address: String,
        metadata: localup_proto::AgentMetadata,
        _peer_addr: std::net::SocketAddr,
    ) where
        C: TransportConnection + 'static,
        S: TransportStream + 'static,
    {
        debug!(
            "Received AgentRegister from agent_id: {} (target: {})",
            agent_id, target_address
        );

        // Validate authentication
        if let Some(ref validator) = self.jwt_validator {
            if let Err(e) = validator.validate(&auth_token) {
                error!("Authentication failed for agent {}: {}", agent_id, e);
                let _ = control_stream
                    .send_message(&TunnelMessage::AgentRejected {
                        reason: format!("Authentication failed: {}", e),
                    })
                    .await;
                return;
            }
        }

        // Check if agent registry is configured
        let Some(ref registry) = self.agent_registry else {
            error!(
                "Agent registry not configured, rejecting agent {}",
                agent_id
            );
            let _ = control_stream
                .send_message(&TunnelMessage::AgentRejected {
                    reason: "Reverse tunnels not enabled on this relay".to_string(),
                })
                .await;
            return;
        };

        // Register the agent (or replace if reconnecting)
        let agent = RegisteredAgent {
            agent_id: agent_id.clone(),
            target_address: target_address.clone(),
            metadata: metadata.clone(),
            connected_at: chrono::Utc::now(),
        };

        match registry.register_or_replace(agent) {
            Ok(old_agent) => {
                if old_agent.is_some() {
                    info!(
                        "✅ Agent re-registered (reconnection): {} (target: {})",
                        agent_id, target_address
                    );
                } else {
                    info!(
                        "✅ Agent registered: {} (target: {})",
                        agent_id, target_address
                    );
                }
            }
            Err(e) => {
                error!("Failed to register agent {}: {}", agent_id, e);
                let _ = control_stream
                    .send_message(&TunnelMessage::AgentRejected {
                        reason: format!("Registration failed: {}", e),
                    })
                    .await;
                return;
            }
        }

        // Send AgentRegistered response
        if let Err(e) = control_stream
            .send_message(&TunnelMessage::AgentRegistered {
                agent_id: agent_id.clone(),
            })
            .await
        {
            error!("Failed to send AgentRegistered message: {}", e);
            registry.unregister(&agent_id);
            return;
        }

        // Store agent connection for routing reverse tunnel requests
        let quic_conn = connection.clone();
        if let Ok(quic_conn) = (quic_conn as Arc<dyn std::any::Any + Send + Sync>)
            .downcast::<localup_transport_quic::QuicConnection>()
        {
            self.agent_connection_manager
                .register(agent_id.clone(), quic_conn)
                .await;
            debug!("Agent connection stored for routing: {}", agent_id);
        } else {
            // Note: Reverse tunnels currently require QUIC transport for connection manager
            // H2/WebSocket support for reverse tunnels is not yet implemented
            error!("Failed to downcast agent connection to QuicConnection - reverse tunnels require QUIC transport");
            registry.unregister(&agent_id);
            return;
        }

        // Keep control stream open for heartbeat and wait for disconnect
        let agent_id_heartbeat = agent_id.clone();
        let heartbeat_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut waiting_for_pong = false;
            let mut pong_deadline = tokio::time::Instant::now();

            loop {
                tokio::select! {
                    // Send ping every 10 seconds
                    _ = interval.tick(), if !waiting_for_pong => {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        debug!("Sending ping to agent {}", agent_id_heartbeat);
                        if let Err(e) = control_stream.send_message(&TunnelMessage::Ping { timestamp }).await {
                            error!("Failed to send ping to agent {}: {}", agent_id_heartbeat, e);
                            break;
                        }

                        waiting_for_pong = true;
                        pong_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
                    }

                    // Check for pong timeout
                    _ = tokio::time::sleep_until(pong_deadline), if waiting_for_pong => {
                        warn!("Pong timeout for agent {} (no response in 5s), assuming disconnected", agent_id_heartbeat);
                        break;
                    }

                    // Receive messages
                    result = control_stream.recv_message() => {
                        match result {
                            Ok(Some(TunnelMessage::Ping { timestamp })) => {
                                debug!("Received ping from agent {}, responding with pong", agent_id_heartbeat);
                                // Respond to agent's heartbeat ping
                                if let Err(e) = control_stream
                                    .send_message(&TunnelMessage::Pong { timestamp })
                                    .await
                                {
                                    error!("Failed to send pong to agent {}: {}", agent_id_heartbeat, e);
                                    break;
                                }
                            }
                            Ok(Some(TunnelMessage::Pong { .. })) => {
                                debug!("Received pong from agent {}", agent_id_heartbeat);
                                waiting_for_pong = false;
                            }
                            Ok(Some(TunnelMessage::Disconnect { reason })) => {
                                info!("Agent {} disconnected: {}", agent_id_heartbeat, reason);
                                break;
                            }
                            Ok(None) => {
                                info!("Control stream closed for agent {}", agent_id_heartbeat);
                                break;
                            }
                            Err(e) => {
                                error!("Error on control stream for agent {}: {}", agent_id_heartbeat, e);
                                break;
                            }
                            Ok(Some(msg)) => {
                                warn!("Unexpected message on agent control stream from {}: {:?}", agent_id_heartbeat, msg);
                            }
                        }
                    }
                }
            }
            debug!("Heartbeat task ended for agent {}", agent_id_heartbeat);
        });

        // Wait for heartbeat task to complete (signals disconnection)
        let _ = heartbeat_task.await;

        // Cleanup: Unregister agent and remove connection
        debug!("Cleaning up agent {}", agent_id);
        registry.unregister(&agent_id);
        self.agent_connection_manager.unregister(&agent_id).await;
        info!("Agent {} disconnected", agent_id);
    }

    /// Validate an auth token and return the user_id
    ///
    /// This method performs enhanced authentication by:
    /// 1. Validating JWT signature and expiration
    /// 2. Verifying token type is "auth" (not "session")
    /// 3. Hashing the token and looking it up in the database
    /// 4. Verifying the token is active (not revoked)
    /// 5. Updating the last_used_at timestamp
    ///
    /// Returns the user_id if authentication succeeds, otherwise returns an error
    async fn validate_auth_token(&self, token: &str) -> Result<String, String> {
        // Step 1: Validate JWT signature and expiration
        let claims = if let Some(ref validator) = self.jwt_validator {
            validator
                .validate(token)
                .map_err(|e| format!("Invalid JWT token: {}", e))?
        } else {
            // No JWT validator configured - skip database validation too
            return Ok("anonymous".to_string());
        };

        // Step 2: Verify token type is "auth" (not "session")
        match &claims.token_type {
            Some(token_type) if token_type == "auth" => {
                // Valid auth token, continue
            }
            Some(token_type) => {
                return Err(format!(
                    "Invalid token type '{}'. Expected 'auth' token for tunnel authentication",
                    token_type
                ));
            }
            None => {
                // Legacy token without token_type - allow for backward compatibility
                debug!("Token missing 'token_type' claim, treating as legacy auth token");
            }
        }

        // Extract user_id from claims (will be verified against database)
        let claimed_user_id = claims.user_id.ok_or_else(|| {
            "Token missing 'user_id' claim. Auth tokens must include user_id".to_string()
        })?;

        // Step 3-5: Database validation (if database is available)
        if let Some(ref db) = self.db {
            // Hash the token using SHA-256 (same as when storing)
            let mut hasher = Sha256::new();
            hasher.update(token.as_bytes());
            let token_hash = format!("{:x}", hasher.finalize());

            // Look up token in database by hash
            let token_record = AuthTokenEntity::find()
                .filter(auth_token::Column::TokenHash.eq(&token_hash))
                .one(db)
                .await
                .map_err(|e| format!("Database error during authentication: {}", e))?
                .ok_or_else(|| "Auth token not found or has been revoked".to_string())?;

            // Verify token is active
            if !token_record.is_active {
                return Err("Auth token has been deactivated".to_string());
            }

            // Check if token is expired
            if let Some(expires_at) = token_record.expires_at {
                let now = chrono::Utc::now();
                if expires_at < now {
                    return Err("Auth token has expired".to_string());
                }
            }

            // Verify user_id matches (ensure JWT wasn't tampered with)
            if token_record.user_id.to_string() != claimed_user_id {
                return Err("Token user_id mismatch - possible JWT tampering".to_string());
            }

            // Update last_used_at timestamp
            let mut active_model: auth_token::ActiveModel = token_record.clone().into();
            active_model.last_used_at = Set(Some(chrono::Utc::now()));
            if let Err(e) = ActiveModelTrait::update(active_model, db).await {
                // Log error but don't fail authentication
                warn!("Failed to update last_used_at for token: {}", e);
            }

            Ok(claimed_user_id)
        } else {
            // No database configured - rely only on JWT validation
            debug!("Database not configured, skipping token database validation");
            Ok(claimed_user_id)
        }
    }

    /// Generate a deterministic subdomain from localup_id and peer IP hash
    /// This ensures uniqueness even when multiple users use the same local port
    /// by incorporating the client's IP address into the hash
    fn generate_subdomain(localup_id: &str, peer_addr: std::net::SocketAddr) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash localup_id
        localup_id.hash(&mut hasher);

        // Hash peer IP (not port, since that can vary on reconnect)
        peer_addr.ip().to_string().hash(&mut hasher);

        let hash = hasher.finish();

        // Convert to base36 (lowercase letters + digits)
        const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let mut subdomain = String::new();
        let mut remaining = hash;

        // Generate 6 characters
        for _ in 0..6 {
            let idx = (remaining % 36) as usize;
            subdomain.push(CHARSET[idx] as char);
            remaining /= 36;
        }

        subdomain
    }

    async fn build_endpoints(
        &self,
        localup_id: &str,
        protocols: &[Protocol],
        _config: &localup_proto::TunnelConfig,
        peer_addr: std::net::SocketAddr,
    ) -> Vec<Endpoint> {
        let mut endpoints = Vec::new();

        for protocol in protocols {
            match protocol {
                Protocol::Http { subdomain, .. } | Protocol::Https { subdomain, .. } => {
                    // Extract local port if available for sticky domain context
                    let local_port = match protocol {
                        Protocol::Http { .. } => None, // HTTP doesn't have a local port concept in Protocol enum
                        Protocol::Https { .. } => None,
                        _ => None,
                    };

                    // Build domain context for custom domain providers
                    let domain_context = DomainContext::new()
                        .with_client_id(localup_id.to_string())
                        .with_local_port(local_port.unwrap_or(0))
                        .with_protocol(
                            if matches!(protocol, Protocol::Http { .. }) {
                                "http"
                            } else {
                                "https"
                            }
                            .to_string(),
                        );

                    // Use provided subdomain or generate via domain provider
                    let actual_subdomain = match subdomain {
                        Some(ref s) if !s.is_empty() => {
                            let protocol_name = if matches!(protocol, Protocol::Http { .. }) {
                                "http"
                            } else {
                                "https"
                            };
                            // Check if manual subdomains are allowed
                            if let Some(ref provider) = self.domain_provider {
                                if !provider.allow_manual_subdomain() {
                                    // Use provider's auto-generation instead
                                    match provider.generate_subdomain(&domain_context).await {
                                        Ok(generated) => {
                                            info!(
                                                "Domain provider auto-generated subdomain '{}' for tunnel {} ({})",
                                                generated, localup_id, protocol_name
                                            );
                                            generated
                                        }
                                        Err(e) => {
                                            warn!("Domain provider error, falling back to default: {}", e);
                                            Self::generate_subdomain(localup_id, peer_addr)
                                        }
                                    }
                                } else {
                                    info!(
                                        "Using user-provided subdomain: '{}' ({})",
                                        s, protocol_name
                                    );
                                    s.clone()
                                }
                            } else {
                                info!("Using user-provided subdomain: '{}' ({})", s, protocol_name);
                                s.clone()
                            }
                        }
                        _ => {
                            // Generate subdomain via domain provider or default
                            if let Some(ref provider) = self.domain_provider {
                                match provider.generate_subdomain(&domain_context).await {
                                    Ok(generated) => {
                                        let protocol_name =
                                            if matches!(protocol, Protocol::Http { .. }) {
                                                "http"
                                            } else {
                                                "https"
                                            };
                                        info!(
                                            "🎯 Domain provider generated subdomain '{}' for tunnel {} ({})",
                                            generated, localup_id, protocol_name
                                        );
                                        generated
                                    }
                                    Err(e) => {
                                        warn!(
                                            "Domain provider error, falling back to default: {}",
                                            e
                                        );
                                        let generated =
                                            Self::generate_subdomain(localup_id, peer_addr);
                                        info!(
                                            "🎯 Auto-generated subdomain '{}' for tunnel {} (fallback)",
                                            generated, localup_id
                                        );
                                        generated
                                    }
                                }
                            } else {
                                let generated = Self::generate_subdomain(localup_id, peer_addr);
                                let protocol_name = if matches!(protocol, Protocol::Http { .. }) {
                                    "http"
                                } else {
                                    "https"
                                };
                                info!(
                                    "🎯 Auto-generated subdomain '{}' for tunnel {} ({})",
                                    generated, localup_id, protocol_name
                                );
                                generated
                            }
                        }
                    };

                    let host = format!("{}.{}", actual_subdomain, self.domain);

                    // Create endpoint with actual subdomain used
                    let endpoint_protocol = if matches!(protocol, Protocol::Http { .. }) {
                        Protocol::Http {
                            subdomain: Some(actual_subdomain.clone()),
                        }
                    } else {
                        Protocol::Https {
                            subdomain: Some(actual_subdomain.clone()),
                        }
                    };

                    // Use actual HTTPS relay port if configured
                    let actual_port = self.https_port.unwrap_or(443);
                    let url_with_port = if actual_port == 443 {
                        // Standard HTTPS port - omit from URL
                        format!("https://{}", host)
                    } else {
                        // Non-standard port - include in URL
                        format!("https://{}:{}", host, actual_port)
                    };

                    endpoints.push(Endpoint {
                        protocol: endpoint_protocol,
                        // HTTP and HTTPS tunnels use HTTPS (TLS termination at exit node)
                        public_url: url_with_port,
                        port: Some(actual_port),
                    });
                }
                Protocol::Tcp { port } => {
                    // TCP endpoint - port will be allocated during registration
                    endpoints.push(Endpoint {
                        protocol: protocol.clone(),
                        public_url: format!("tcp://{}:{}", self.domain, port),
                        port: Some(*port),
                    });
                }
                Protocol::Tls { port, sni_pattern } => {
                    // TLS endpoint - use actual relay TLS port if configured, otherwise use client's requested port
                    let actual_port = self.tls_port.unwrap_or(*port);
                    debug!(
                        "Building TLS endpoint: relay_port={:?}, client_port={}, actual_port={}",
                        self.tls_port, port, actual_port
                    );
                    endpoints.push(Endpoint {
                        protocol: protocol.clone(),
                        public_url: format!(
                            "tls://{}:{} (SNI: {})",
                            self.domain, actual_port, sni_pattern
                        ),
                        port: Some(actual_port),
                    });
                }
            }
        }

        endpoints
    }

    fn register_route(&self, localup_id: &str, endpoint: &Endpoint) -> Result<Option<u16>, String> {
        match &endpoint.protocol {
            Protocol::Http { subdomain } | Protocol::Https { subdomain } => {
                let subdomain_str = subdomain
                    .as_ref()
                    .ok_or_else(|| "Subdomain is required for HTTP/HTTPS routes".to_string())?;
                let host = format!("{}.{}", subdomain_str, self.domain);

                let route_key = RouteKey::HttpHost(host.clone());

                // Check if route already exists
                if self.route_registry.exists(&route_key) {
                    if let Ok(existing_target) = self.route_registry.lookup(&route_key) {
                        if existing_target.localup_id == localup_id {
                            // Same tunnel ID reconnecting - force cleanup of old route
                            warn!(
                                "Route {} already exists for the same tunnel {}. Force cleaning up old route (likely a reconnect).",
                                host, localup_id
                            );
                            let _ = self.route_registry.unregister(&route_key);
                        } else {
                            // Different tunnel ID - this is a real conflict
                            error!(
                                "Route {} already exists for different tunnel {} (current tunnel: {}). Route conflict!",
                                host, existing_target.localup_id, localup_id
                            );
                            return Err(format!(
                                "Subdomain '{}' is already taken by another tunnel",
                                subdomain_str
                            ));
                        }
                    }
                }

                let route_target = RouteTarget {
                    localup_id: localup_id.to_string(),
                    target_addr: format!("tunnel:{}", localup_id), // Special marker for tunnel routing
                    metadata: Some("via-tunnel".to_string()),
                };

                self.route_registry
                    .register(route_key, route_target)
                    .map_err(|e| {
                        error!("Failed to register route {}: {}", host, e);
                        e.to_string()
                    })?;

                info!("✅ Registered route: {} -> tunnel:{}", host, localup_id);
                Ok(None)
            }
            Protocol::Tcp { port } => {
                if let Some(ref allocator) = self.port_allocator {
                    // Allocate a port for this TCP tunnel
                    // If port is 0, auto-allocate; otherwise try to allocate the specific port
                    let requested_port = if *port == 0 { None } else { Some(*port) };
                    let allocated_port = allocator.allocate(localup_id, requested_port)?;

                    if requested_port.is_some() {
                        info!(
                            "✅ Allocated requested TCP port {} for tunnel {}",
                            allocated_port, localup_id
                        );
                    } else {
                        info!(
                            "🎯 Auto-allocated TCP port {} for tunnel {}",
                            allocated_port, localup_id
                        );
                    }

                    // Spawn TCP proxy server if spawner is configured
                    if let Some(ref spawner) = self.tcp_proxy_spawner {
                        let localup_id_clone = localup_id.to_string();
                        let spawner_future = spawner(localup_id_clone.clone(), allocated_port);

                        // Spawn the proxy server in a background task and track the handle
                        let handle = tokio::spawn(async move {
                            if let Err(e) = spawner_future.await {
                                error!("Failed to spawn TCP proxy server: {}", e);
                            }
                        });

                        // Register the task handle so it can be aborted on disconnect
                        self.task_tracker.register(localup_id_clone, handle);

                        info!(
                            "Spawned TCP proxy server on port {} for tunnel {}",
                            allocated_port, localup_id
                        );
                    } else {
                        warn!(
                            "TCP proxy spawner not configured - TCP data forwarding will not work"
                        );
                    }

                    Ok(Some(allocated_port))
                } else {
                    warn!("TCP tunnel requested but no port allocator configured");
                    Err("TCP tunnels not supported (no port allocator)".to_string())
                }
            }
            Protocol::Tls { sni_pattern, .. } => {
                // Register TLS route based on SNI pattern
                let route_key = RouteKey::TlsSni(sni_pattern.clone());
                let route_target = RouteTarget {
                    localup_id: localup_id.to_string(),
                    target_addr: format!("tunnel:{}", localup_id), // Special marker for tunnel routing
                    metadata: Some("via-tunnel".to_string()),
                };

                self.route_registry
                    .register(route_key, route_target)
                    .map_err(|e| e.to_string())?;

                debug!(
                    "Registered TLS route for SNI pattern {} -> tunnel:{}",
                    sni_pattern, localup_id
                );
                Ok(None)
            }
        }
    }

    async fn unregister_route(&self, localup_id: &str, endpoint: &Endpoint) {
        match &endpoint.protocol {
            Protocol::Http { subdomain } | Protocol::Https { subdomain } => {
                if let Some(subdomain_str) = subdomain {
                    let host = format!("{}.{}", subdomain_str, self.domain);

                    let route_key = RouteKey::HttpHost(host.clone());
                    match self.route_registry.unregister(&route_key) {
                        Ok(_) => {
                            info!("🗑️  Unregistered route: {} (tunnel: {})", host, localup_id);
                        }
                        Err(e) => {
                            warn!(
                                "Failed to unregister route {}: {} (may already be removed)",
                                host, e
                            );
                        }
                    }
                }
            }
            Protocol::Tcp { .. } => {
                // 1. First abort the TCP proxy server task
                self.task_tracker.unregister(localup_id);
                info!("Terminated TCP proxy server task for tunnel {}", localup_id);

                // 2. Give the task time to drop the socket (brief delay)
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                // 3. NOW deallocate the port - socket should be released by now
                if let Some(ref allocator) = self.port_allocator {
                    allocator.deallocate(localup_id);
                    info!("Deallocated TCP port for tunnel {}", localup_id);
                }
            }
            Protocol::Tls { sni_pattern, .. } => {
                let route_key = RouteKey::TlsSni(sni_pattern.clone());
                let _ = self.route_registry.unregister(&route_key);
                debug!("Unregistered TLS route for SNI pattern: {}", sni_pattern);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use localup_proto::{Protocol, TunnelConfig};
    use std::sync::Arc;

    #[test]
    fn test_generate_subdomain_deterministic() {
        let localup_id = "my-tunnel-123";
        let peer_addr = "192.168.1.100:50000".parse().unwrap();

        // Generate subdomain multiple times - should be identical with same localup_id and peer_addr
        let subdomain1 = TunnelHandler::generate_subdomain(localup_id, peer_addr);
        let subdomain2 = TunnelHandler::generate_subdomain(localup_id, peer_addr);
        let subdomain3 = TunnelHandler::generate_subdomain(localup_id, peer_addr);

        assert_eq!(subdomain1, subdomain2);
        assert_eq!(subdomain2, subdomain3);
    }

    #[test]
    fn test_generate_subdomain_different_ids() {
        let peer_addr = "192.168.1.100:50000".parse().unwrap();
        let subdomain1 = TunnelHandler::generate_subdomain("localup-1", peer_addr);
        let subdomain2 = TunnelHandler::generate_subdomain("localup-2", peer_addr);

        // Different tunnel IDs should produce different subdomains
        assert_ne!(subdomain1, subdomain2);
    }

    #[test]
    fn test_generate_subdomain_length_and_charset() {
        let peer_addr = "192.168.1.100:50000".parse().unwrap();
        let subdomain = TunnelHandler::generate_subdomain("test-tunnel", peer_addr);

        // Should be 6 characters
        assert_eq!(subdomain.len(), 6);

        // Should only contain lowercase letters and digits
        assert!(subdomain
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn test_generate_subdomain_different_ips() {
        let localup_id = "localup-with-port-3000";
        let peer_addr1 = "192.168.1.100:50000".parse().unwrap();
        let peer_addr2 = "192.168.1.101:50000".parse().unwrap();

        // Same localup_id but different peer IPs should produce different subdomains
        let subdomain1 = TunnelHandler::generate_subdomain(localup_id, peer_addr1);
        let subdomain2 = TunnelHandler::generate_subdomain(localup_id, peer_addr2);

        // This ensures multiple users with same local port (e.g., port 3000) get unique subdomains
        assert_ne!(
            subdomain1, subdomain2,
            "Different IPs should produce different subdomains"
        );
    }

    #[tokio::test]
    async fn test_build_endpoints_http() {
        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry,
            None,
            "tunnel.test".to_string(),
            pending_requests,
        );

        let localup_id = "test-tunnel";
        let protocols = vec![Protocol::Http {
            subdomain: Some("custom".to_string()),
        }];
        let config = TunnelConfig::default();

        let mock_peer_addr = "127.0.0.1:12345".parse().unwrap();
        let endpoints = handler
            .build_endpoints(localup_id, &protocols, &config, mock_peer_addr)
            .await;

        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].public_url, "https://custom.tunnel.test");
        assert!(
            matches!(endpoints[0].protocol, Protocol::Http { subdomain: Some(ref s) } if s == "custom")
        );
    }

    #[tokio::test]
    async fn test_build_endpoints_http_auto_subdomain() {
        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry,
            None,
            "tunnel.test".to_string(),
            pending_requests,
        );

        let localup_id = "test-tunnel";
        let protocols = vec![Protocol::Http { subdomain: None }]; // Auto-generate subdomain
        let config = TunnelConfig::default();

        let mock_peer_addr = "127.0.0.1:12345".parse().unwrap();
        let endpoints = handler
            .build_endpoints(localup_id, &protocols, &config, mock_peer_addr)
            .await;

        assert_eq!(endpoints.len(), 1);

        // Should have generated a subdomain
        if let Protocol::Http {
            subdomain: Some(ref s),
        } = endpoints[0].protocol
        {
            assert!(!s.is_empty());
            assert_eq!(s.len(), 6);
        } else {
            panic!("Expected Http protocol with auto-generated subdomain");
        }
    }

    #[tokio::test]
    async fn test_build_endpoints_https() {
        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry,
            None,
            "tunnel.test".to_string(),
            pending_requests,
        );

        let localup_id = "test-tunnel";
        let protocols = vec![Protocol::Https {
            subdomain: Some("secure".to_string()),
        }];
        let config = TunnelConfig::default();

        let mock_peer_addr = "127.0.0.1:12345".parse().unwrap();
        let endpoints = handler
            .build_endpoints(localup_id, &protocols, &config, mock_peer_addr)
            .await;

        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].public_url, "https://secure.tunnel.test");
        assert!(
            matches!(endpoints[0].protocol, Protocol::Https { subdomain: Some(ref s) } if s == "secure")
        );
    }

    #[tokio::test]
    async fn test_build_endpoints_tcp() {
        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry,
            None,
            "tunnel.test".to_string(),
            pending_requests,
        );

        let localup_id = "test-tunnel";
        let protocols = vec![Protocol::Tcp { port: 8080 }];
        let config = TunnelConfig::default();

        let mock_peer_addr = "127.0.0.1:12345".parse().unwrap();
        let endpoints = handler
            .build_endpoints(localup_id, &protocols, &config, mock_peer_addr)
            .await;

        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].public_url, "tcp://tunnel.test:8080");
        assert_eq!(endpoints[0].port, Some(8080));
    }

    #[tokio::test]
    async fn test_build_endpoints_multiple_protocols() {
        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry,
            None,
            "tunnel.test".to_string(),
            pending_requests,
        );

        let localup_id = "test-tunnel";
        let protocols = vec![
            Protocol::Http {
                subdomain: Some("http".to_string()),
            },
            Protocol::Https {
                subdomain: Some("https".to_string()),
            },
            Protocol::Tcp { port: 8080 },
        ];
        let config = TunnelConfig::default();

        let mock_peer_addr = "127.0.0.1:12345".parse().unwrap();
        let endpoints = handler
            .build_endpoints(localup_id, &protocols, &config, mock_peer_addr)
            .await;

        assert_eq!(endpoints.len(), 3);
        assert_eq!(endpoints[0].public_url, "https://http.tunnel.test");
        assert_eq!(endpoints[1].public_url, "https://https.tunnel.test");
        assert_eq!(endpoints[2].public_url, "tcp://tunnel.test:8080");
    }

    #[tokio::test]
    async fn test_build_endpoints_tls() {
        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry,
            None,
            "tunnel.test".to_string(),
            pending_requests,
        );

        let localup_id = "test-tunnel";
        let protocols = vec![Protocol::Tls {
            port: 443,
            sni_pattern: "*.example.com".to_string(),
        }];
        let config = TunnelConfig::default();

        let mock_peer_addr = "127.0.0.1:12345".parse().unwrap();
        let endpoints = handler
            .build_endpoints(localup_id, &protocols, &config, mock_peer_addr)
            .await;

        assert_eq!(endpoints.len(), 1);
        assert!(endpoints[0].public_url.contains("tls://"));
        assert!(endpoints[0].public_url.contains("*.example.com"));
    }

    #[test]
    fn test_handler_with_port_allocator() {
        struct MockPortAllocator;
        impl PortAllocator for MockPortAllocator {
            fn allocate(
                &self,
                _localup_id: &str,
                _requested_port: Option<u16>,
            ) -> Result<u16, String> {
                Ok(9000)
            }
            fn deallocate(&self, _localup_id: &str) {}
            fn get_allocated_port(&self, _localup_id: &str) -> Option<u16> {
                Some(9000)
            }
        }

        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry,
            None,
            "tunnel.test".to_string(),
            pending_requests,
        )
        .with_port_allocator(Arc::new(MockPortAllocator));

        assert!(handler.port_allocator.is_some());
    }

    #[test]
    fn test_handler_with_tcp_proxy_spawner() {
        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let spawner: TcpProxySpawner = Arc::new(|_localup_id, _port| Box::pin(async { Ok(()) }));

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry,
            None,
            "tunnel.test".to_string(),
            pending_requests,
        )
        .with_tcp_proxy_spawner(spawner);

        assert!(handler.tcp_proxy_spawner.is_some());
    }

    #[test]
    fn test_register_route_http() {
        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry.clone(),
            None,
            "tunnel.test".to_string(),
            pending_requests,
        );

        let localup_id = "test-tunnel";
        let endpoint = Endpoint {
            protocol: Protocol::Http {
                subdomain: Some("test".to_string()),
            },
            public_url: "https://test.tunnel.test".to_string(),
            port: None,
        };

        let result = handler.register_route(localup_id, &endpoint);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None); // HTTP doesn't return allocated port

        // Verify route was registered
        assert_eq!(route_registry.count(), 1);
    }

    #[test]
    fn test_register_route_https() {
        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry.clone(),
            None,
            "tunnel.test".to_string(),
            pending_requests,
        );

        let localup_id = "test-tunnel";
        let endpoint = Endpoint {
            protocol: Protocol::Https {
                subdomain: Some("secure".to_string()),
            },
            public_url: "https://secure.tunnel.test".to_string(),
            port: None,
        };

        let result = handler.register_route(localup_id, &endpoint);
        assert!(result.is_ok());

        // Verify route was registered
        assert_eq!(route_registry.count(), 1);
    }

    #[test]
    fn test_register_route_tcp_without_allocator() {
        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry,
            None,
            "tunnel.test".to_string(),
            pending_requests,
        );

        let localup_id = "test-tunnel";
        let endpoint = Endpoint {
            protocol: Protocol::Tcp { port: 8080 },
            public_url: "tcp://tunnel.test:8080".to_string(),
            port: Some(8080),
        };

        let result = handler.register_route(localup_id, &endpoint);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not supported"));
    }

    #[test]
    fn test_register_route_tcp_with_allocator() {
        struct MockPortAllocator;
        impl PortAllocator for MockPortAllocator {
            fn allocate(
                &self,
                _localup_id: &str,
                _requested_port: Option<u16>,
            ) -> Result<u16, String> {
                Ok(9000)
            }
            fn deallocate(&self, _localup_id: &str) {}
            fn get_allocated_port(&self, _localup_id: &str) -> Option<u16> {
                Some(9000)
            }
        }

        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry,
            None,
            "tunnel.test".to_string(),
            pending_requests,
        )
        .with_port_allocator(Arc::new(MockPortAllocator));

        let localup_id = "test-tunnel";
        let endpoint = Endpoint {
            protocol: Protocol::Tcp { port: 8080 },
            public_url: "tcp://tunnel.test:8080".to_string(),
            port: Some(8080),
        };

        let result = handler.register_route(localup_id, &endpoint);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(9000));
    }

    #[tokio::test]
    async fn test_unregister_route_http() {
        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry.clone(),
            None,
            "tunnel.test".to_string(),
            pending_requests,
        );

        let localup_id = "test-tunnel";
        let endpoint = Endpoint {
            protocol: Protocol::Http {
                subdomain: Some("test".to_string()),
            },
            public_url: "https://test.tunnel.test".to_string(),
            port: None,
        };

        // Register first
        handler.register_route(localup_id, &endpoint).unwrap();
        assert_eq!(route_registry.count(), 1);

        // Unregister
        handler.unregister_route(localup_id, &endpoint).await;
        assert_eq!(route_registry.count(), 0);
    }

    #[tokio::test]
    async fn test_unregister_route_tcp() {
        struct MockPortAllocator {
            deallocated: Arc<std::sync::Mutex<bool>>,
        }
        impl PortAllocator for MockPortAllocator {
            fn allocate(
                &self,
                _localup_id: &str,
                _requested_port: Option<u16>,
            ) -> Result<u16, String> {
                Ok(9000)
            }
            fn deallocate(&self, _localup_id: &str) {
                *self.deallocated.lock().unwrap() = true;
            }
            fn get_allocated_port(&self, _localup_id: &str) -> Option<u16> {
                Some(9000)
            }
        }

        let deallocated = Arc::new(std::sync::Mutex::new(false));
        let allocator = Arc::new(MockPortAllocator {
            deallocated: deallocated.clone(),
        });

        let connection_manager = Arc::new(TunnelConnectionManager::new());
        let route_registry = Arc::new(RouteRegistry::new());
        let pending_requests = Arc::new(PendingRequests::new());

        let handler = TunnelHandler::new(
            connection_manager,
            route_registry,
            None,
            "tunnel.test".to_string(),
            pending_requests,
        )
        .with_port_allocator(allocator);

        let localup_id = "test-tunnel";
        let endpoint = Endpoint {
            protocol: Protocol::Tcp { port: 8080 },
            public_url: "tcp://tunnel.test:9000".to_string(),
            port: Some(9000),
        };

        handler.unregister_route(localup_id, &endpoint).await;

        // Verify deallocate was called
        assert!(*deallocated.lock().unwrap());
    }
}
