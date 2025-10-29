//! Tunnel connection handler for exit nodes

use std::sync::Arc;
use tracing::{debug, error, info, warn};

use tunnel_auth::JwtValidator;
use tunnel_proto::{Endpoint, Protocol, TunnelMessage};
use tunnel_router::{RouteKey, RouteRegistry, RouteTarget};
use tunnel_transport::{TransportConnection, TransportStream};

use crate::connection::TunnelConnectionManager;
use crate::pending_requests::PendingRequests;

/// Trait for port allocation (TCP tunnels)
pub trait PortAllocator: Send + Sync {
    /// Allocate a port for the given tunnel_id
    /// If requested_port is Some, try to allocate that specific port
    /// If requested_port is None or unavailable, allocate any available port
    fn allocate(&self, tunnel_id: &str, requested_port: Option<u16>) -> Result<u16, String>;
    fn deallocate(&self, tunnel_id: &str);
    fn get_allocated_port(&self, tunnel_id: &str) -> Option<u16>;
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

/// Handles a tunnel connection from a client
pub struct TunnelHandler {
    connection_manager: Arc<TunnelConnectionManager>,
    route_registry: Arc<RouteRegistry>,
    jwt_validator: Option<Arc<JwtValidator>>,
    domain: String,
    #[allow(dead_code)] // Used for HTTP request/response handling (future work)
    pending_requests: Arc<PendingRequests>,
    port_allocator: Option<Arc<dyn PortAllocator>>,
    tcp_proxy_spawner: Option<TcpProxySpawner>,
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
            domain,
            pending_requests,
            port_allocator: None,
            tcp_proxy_spawner: None,
        }
    }

    pub fn with_port_allocator(mut self, port_allocator: Arc<dyn PortAllocator>) -> Self {
        self.port_allocator = Some(port_allocator);
        self
    }

    pub fn with_tcp_proxy_spawner(mut self, spawner: TcpProxySpawner) -> Self {
        self.tcp_proxy_spawner = Some(spawner);
        self
    }

    /// Handle an incoming tunnel connection
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

        // Read the Connect message
        let connect_result = match control_stream.recv_message().await {
            Ok(Some(TunnelMessage::Connect {
                tunnel_id,
                auth_token,
                protocols,
                config,
            })) => {
                debug!("Received Connect from tunnel_id: {}", tunnel_id);

                // Validate authentication
                if let Some(ref validator) = self.jwt_validator {
                    if let Err(e) = validator.validate(&auth_token) {
                        error!("Authentication failed for tunnel {}: {}", tunnel_id, e);
                        let _ = control_stream
                            .send_message(&TunnelMessage::Disconnect {
                                reason: format!("Authentication failed: {}", e),
                            })
                            .await;
                        return;
                    }
                }

                // Build endpoints based on requested protocols
                let mut endpoints =
                    self.build_endpoints(&tunnel_id, &protocols, &config, peer_addr);
                debug!(
                    "Built {} endpoints for tunnel {}",
                    endpoints.len(),
                    tunnel_id
                );

                // Register routes in the route registry
                // If any route registration fails (e.g., subdomain conflict), reject the connection
                // For TCP endpoints, update with allocated port
                for endpoint in &mut endpoints {
                    debug!("Registering endpoint: protocol={:?}", endpoint.protocol);
                    match self.register_route(&tunnel_id, endpoint) {
                        Ok(Some(allocated_port)) => {
                            // Update TCP endpoint with allocated port
                            endpoint.public_url =
                                format!("tcp://{}:{}", self.domain, allocated_port);
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
                            error!("Failed to register route for tunnel {}: {}", tunnel_id, e);

                            // Send error response and close connection
                            let error_msg = if e.to_string().contains("already exists") {
                                "Subdomain is already in use by another tunnel".to_string()
                            } else {
                                format!("Failed to register route: {}", e)
                            };

                            let _ = control_stream
                                .send_message(&TunnelMessage::Disconnect { reason: error_msg })
                                .await;
                            return;
                        }
                    }
                }

                // Register the tunnel connection
                // Note: We need to downcast to QuicConnection since connection_manager stores Arc<QuicConnection>
                // This is a temporary limitation until we make connection_manager fully generic
                let quic_conn = connection.clone();
                if let Ok(quic_conn) = (quic_conn as Arc<dyn std::any::Any + Send + Sync>)
                    .downcast::<tunnel_transport_quic::QuicConnection>()
                {
                    self.connection_manager
                        .register(tunnel_id.clone(), endpoints.clone(), quic_conn)
                        .await;
                } else {
                    error!("Failed to downcast connection to QuicConnection");
                    return;
                }

                info!(
                    "✅ Tunnel registered: {} with {} endpoints",
                    tunnel_id,
                    endpoints.len()
                );

                // Send Connected response
                if let Err(e) = control_stream
                    .send_message(&TunnelMessage::Connected {
                        tunnel_id: tunnel_id.clone(),
                        endpoints: endpoints.clone(),
                    })
                    .await
                {
                    error!("Failed to send Connected message: {}", e);
                    return;
                }

                Some((tunnel_id, endpoints))
            }
            Ok(Some(other)) => {
                warn!("Expected Connect message, got {:?}", other);
                return;
            }
            Ok(None) => {
                warn!("Connection closed before Connect message was received");
                return;
            }
            Err(e) => {
                error!("Failed to read Connect message: {}", e);
                return;
            }
        };

        let Some((tunnel_id, endpoints)) = connect_result else {
            return;
        };

        // Keep control stream open for ping/pong heartbeat
        // Server actively sends pings every 10 seconds, expects pongs within 5 seconds
        let tunnel_id_heartbeat = tunnel_id.clone();
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

                        debug!("Sending ping to tunnel {}", tunnel_id_heartbeat);
                        if let Err(e) = control_stream.send_message(&TunnelMessage::Ping { timestamp }).await {
                            error!("Failed to send ping to tunnel {}: {}", tunnel_id_heartbeat, e);
                            break;
                        }

                        // Start waiting for pong
                        waiting_for_pong = true;
                        pong_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
                    }

                    // Check for pong timeout
                    _ = tokio::time::sleep_until(pong_deadline), if waiting_for_pong => {
                        warn!("Pong timeout for tunnel {} (no response in 5s), assuming disconnected", tunnel_id_heartbeat);
                        break;
                    }

                    // Receive messages (always ready to receive)
                    result = control_stream.recv_message() => {
                        match result {
                            Ok(Some(TunnelMessage::Pong { .. })) => {
                                debug!("Received pong from tunnel {}", tunnel_id_heartbeat);
                                waiting_for_pong = false;
                            }
                            Ok(Some(TunnelMessage::Disconnect { reason })) => {
                                info!("Tunnel {} disconnected: {}", tunnel_id_heartbeat, reason);

                                // Send disconnect acknowledgment
                                if let Err(e) = control_stream.send_message(&TunnelMessage::DisconnectAck {
                                    tunnel_id: tunnel_id_heartbeat.clone(),
                                }).await {
                                    warn!("Failed to send disconnect ack: {}", e);
                                } else {
                                    debug!("Sent disconnect acknowledgment to tunnel {}", tunnel_id_heartbeat);
                                }

                                break;
                            }
                            Ok(None) => {
                                info!("Control stream closed for tunnel {}", tunnel_id_heartbeat);
                                break;
                            }
                            Err(e) => {
                                error!("Error on control stream for tunnel {}: {}", tunnel_id_heartbeat, e);
                                break;
                            }
                            Ok(Some(msg)) => {
                                warn!("Unexpected message on control stream from tunnel {}: {:?}", tunnel_id_heartbeat, msg);
                            }
                        }
                    }
                }
            }
            debug!("Heartbeat task ended for tunnel {}", tunnel_id_heartbeat);
        });

        // Wait for the heartbeat task to complete (signals disconnection)
        let _ = heartbeat_task.await;

        // Cleanup on disconnect
        debug!("Cleaning up tunnel {}", tunnel_id);

        self.connection_manager.unregister(&tunnel_id).await;

        // Unregister routes
        for endpoint in &endpoints {
            self.unregister_route(&tunnel_id, endpoint);
        }

        info!("Tunnel {} disconnected", tunnel_id);
    }

    /// Generate a deterministic subdomain from tunnel_id and peer IP hash
    /// This ensures uniqueness even when multiple users use the same local port
    /// by incorporating the client's IP address into the hash
    fn generate_subdomain(tunnel_id: &str, peer_addr: std::net::SocketAddr) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash tunnel_id
        tunnel_id.hash(&mut hasher);

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

    fn build_endpoints(
        &self,
        tunnel_id: &str,
        protocols: &[Protocol],
        _config: &tunnel_proto::TunnelConfig,
        peer_addr: std::net::SocketAddr,
    ) -> Vec<Endpoint> {
        let mut endpoints = Vec::new();

        for protocol in protocols {
            match protocol {
                Protocol::Http { subdomain } => {
                    // Use provided subdomain or generate deterministic one
                    let actual_subdomain = match subdomain {
                        Some(ref s) if !s.is_empty() => {
                            info!("Using user-provided subdomain: '{}' (http)", s);
                            s.clone()
                        }
                        _ => {
                            let generated = Self::generate_subdomain(tunnel_id, peer_addr);
                            info!(
                                "🎯 Auto-generated subdomain '{}' for tunnel {} (http)",
                                generated, tunnel_id
                            );
                            generated
                        }
                    };

                    let host = format!("{}.{}", actual_subdomain, self.domain);

                    // Create endpoint with actual subdomain used
                    let endpoint_protocol = Protocol::Http {
                        subdomain: Some(actual_subdomain),
                    };

                    endpoints.push(Endpoint {
                        protocol: endpoint_protocol,
                        // HTTP tunnels use HTTPS (TLS termination at exit node)
                        public_url: format!("https://{}", host),
                        port: None,
                    });
                }
                Protocol::Https { subdomain } => {
                    // Use provided subdomain or generate deterministic one
                    let actual_subdomain = match subdomain {
                        Some(ref s) if !s.is_empty() => {
                            info!("Using user-provided subdomain: '{}' (https)", s);
                            s.clone()
                        }
                        _ => {
                            let generated = Self::generate_subdomain(tunnel_id, peer_addr);
                            info!(
                                "🎯 Auto-generated subdomain '{}' for tunnel {} (https)",
                                generated, tunnel_id
                            );
                            generated
                        }
                    };

                    let host = format!("{}.{}", actual_subdomain, self.domain);

                    // Create endpoint with actual subdomain used
                    let endpoint_protocol = Protocol::Https {
                        subdomain: Some(actual_subdomain),
                    };

                    endpoints.push(Endpoint {
                        protocol: endpoint_protocol,
                        public_url: format!("https://{}", host),
                        port: None,
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
                    // TLS endpoint - port will be allocated during registration
                    endpoints.push(Endpoint {
                        protocol: protocol.clone(),
                        public_url: format!(
                            "tls://{}:{} (SNI: {})",
                            self.domain, port, sni_pattern
                        ),
                        port: Some(*port),
                    });
                }
            }
        }

        endpoints
    }

    fn register_route(&self, tunnel_id: &str, endpoint: &Endpoint) -> Result<Option<u16>, String> {
        match &endpoint.protocol {
            Protocol::Http { subdomain } | Protocol::Https { subdomain } => {
                let subdomain_str = subdomain
                    .as_ref()
                    .ok_or_else(|| "Subdomain is required for HTTP/HTTPS routes".to_string())?;
                let host = format!("{}.{}", subdomain_str, self.domain);

                let route_key = RouteKey::HttpHost(host.clone());
                let route_target = RouteTarget {
                    tunnel_id: tunnel_id.to_string(),
                    target_addr: format!("tunnel:{}", tunnel_id), // Special marker for tunnel routing
                    metadata: Some("via-tunnel".to_string()),
                };

                self.route_registry
                    .register(route_key, route_target)
                    .map_err(|e| e.to_string())?;

                debug!("Registered route: {} -> tunnel:{}", host, tunnel_id);
                Ok(None)
            }
            Protocol::Tcp { port } => {
                if let Some(ref allocator) = self.port_allocator {
                    // Allocate a port for this TCP tunnel
                    // If port is 0, auto-allocate; otherwise try to allocate the specific port
                    let requested_port = if *port == 0 { None } else { Some(*port) };
                    let allocated_port = allocator.allocate(tunnel_id, requested_port)?;

                    if requested_port.is_some() {
                        info!(
                            "✅ Allocated requested TCP port {} for tunnel {}",
                            allocated_port, tunnel_id
                        );
                    } else {
                        info!(
                            "🎯 Auto-allocated TCP port {} for tunnel {}",
                            allocated_port, tunnel_id
                        );
                    }

                    // Spawn TCP proxy server if spawner is configured
                    if let Some(ref spawner) = self.tcp_proxy_spawner {
                        let tunnel_id_clone = tunnel_id.to_string();
                        let spawner_future = spawner(tunnel_id_clone, allocated_port);

                        // Spawn the proxy server in a background task
                        tokio::spawn(async move {
                            if let Err(e) = spawner_future.await {
                                error!("Failed to spawn TCP proxy server: {}", e);
                            }
                        });

                        info!(
                            "Spawned TCP proxy server on port {} for tunnel {}",
                            allocated_port, tunnel_id
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
            _ => Ok(None),
        }
    }

    fn unregister_route(&self, tunnel_id: &str, endpoint: &Endpoint) {
        match &endpoint.protocol {
            Protocol::Http { subdomain } | Protocol::Https { subdomain } => {
                if let Some(subdomain_str) = subdomain {
                    let host = format!("{}.{}", subdomain_str, self.domain);

                    let route_key = RouteKey::HttpHost(host.clone());
                    let _ = self.route_registry.unregister(&route_key);
                    debug!("Unregistered route: {}", host);
                }
            }
            Protocol::Tcp { .. } => {
                if let Some(ref allocator) = self.port_allocator {
                    allocator.deallocate(tunnel_id);
                    info!("Deallocated TCP port for tunnel {}", tunnel_id);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tunnel_proto::{Protocol, TunnelConfig};

    #[test]
    fn test_generate_subdomain_deterministic() {
        let tunnel_id = "my-tunnel-123";
        let peer_addr = "192.168.1.100:50000".parse().unwrap();

        // Generate subdomain multiple times - should be identical with same tunnel_id and peer_addr
        let subdomain1 = TunnelHandler::generate_subdomain(tunnel_id, peer_addr);
        let subdomain2 = TunnelHandler::generate_subdomain(tunnel_id, peer_addr);
        let subdomain3 = TunnelHandler::generate_subdomain(tunnel_id, peer_addr);

        assert_eq!(subdomain1, subdomain2);
        assert_eq!(subdomain2, subdomain3);
    }

    #[test]
    fn test_generate_subdomain_different_ids() {
        let peer_addr = "192.168.1.100:50000".parse().unwrap();
        let subdomain1 = TunnelHandler::generate_subdomain("tunnel-1", peer_addr);
        let subdomain2 = TunnelHandler::generate_subdomain("tunnel-2", peer_addr);

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
        let tunnel_id = "tunnel-with-port-3000";
        let peer_addr1 = "192.168.1.100:50000".parse().unwrap();
        let peer_addr2 = "192.168.1.101:50000".parse().unwrap();

        // Same tunnel_id but different peer IPs should produce different subdomains
        let subdomain1 = TunnelHandler::generate_subdomain(tunnel_id, peer_addr1);
        let subdomain2 = TunnelHandler::generate_subdomain(tunnel_id, peer_addr2);

        // This ensures multiple users with same local port (e.g., port 3000) get unique subdomains
        assert_ne!(
            subdomain1, subdomain2,
            "Different IPs should produce different subdomains"
        );
    }

    #[test]
    fn test_build_endpoints_http() {
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

        let tunnel_id = "test-tunnel";
        let protocols = vec![Protocol::Http {
            subdomain: Some("custom".to_string()),
        }];
        let config = TunnelConfig::default();

        let mock_peer_addr = "127.0.0.1:12345".parse().unwrap();
        let endpoints = handler.build_endpoints(tunnel_id, &protocols, &config, mock_peer_addr);

        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].public_url, "https://custom.tunnel.test");
        assert!(
            matches!(endpoints[0].protocol, Protocol::Http { subdomain: Some(ref s) } if s == "custom")
        );
    }

    #[test]
    fn test_build_endpoints_http_auto_subdomain() {
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

        let tunnel_id = "test-tunnel";
        let protocols = vec![Protocol::Http { subdomain: None }]; // Auto-generate subdomain
        let config = TunnelConfig::default();

        let mock_peer_addr = "127.0.0.1:12345".parse().unwrap();
        let endpoints = handler.build_endpoints(tunnel_id, &protocols, &config, mock_peer_addr);

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

    #[test]
    fn test_build_endpoints_https() {
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

        let tunnel_id = "test-tunnel";
        let protocols = vec![Protocol::Https {
            subdomain: Some("secure".to_string()),
        }];
        let config = TunnelConfig::default();

        let mock_peer_addr = "127.0.0.1:12345".parse().unwrap();
        let endpoints = handler.build_endpoints(tunnel_id, &protocols, &config, mock_peer_addr);

        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].public_url, "https://secure.tunnel.test");
        assert!(
            matches!(endpoints[0].protocol, Protocol::Https { subdomain: Some(ref s) } if s == "secure")
        );
    }

    #[test]
    fn test_build_endpoints_tcp() {
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

        let tunnel_id = "test-tunnel";
        let protocols = vec![Protocol::Tcp { port: 8080 }];
        let config = TunnelConfig::default();

        let mock_peer_addr = "127.0.0.1:12345".parse().unwrap();
        let endpoints = handler.build_endpoints(tunnel_id, &protocols, &config, mock_peer_addr);

        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].public_url, "tcp://tunnel.test:8080");
        assert_eq!(endpoints[0].port, Some(8080));
    }

    #[test]
    fn test_build_endpoints_multiple_protocols() {
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

        let tunnel_id = "test-tunnel";
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
        let endpoints = handler.build_endpoints(tunnel_id, &protocols, &config, mock_peer_addr);

        assert_eq!(endpoints.len(), 3);
        assert_eq!(endpoints[0].public_url, "https://http.tunnel.test");
        assert_eq!(endpoints[1].public_url, "https://https.tunnel.test");
        assert_eq!(endpoints[2].public_url, "tcp://tunnel.test:8080");
    }

    #[test]
    fn test_build_endpoints_tls() {
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

        let tunnel_id = "test-tunnel";
        let protocols = vec![Protocol::Tls {
            port: 443,
            sni_pattern: "*.example.com".to_string(),
        }];
        let config = TunnelConfig::default();

        let mock_peer_addr = "127.0.0.1:12345".parse().unwrap();
        let endpoints = handler.build_endpoints(tunnel_id, &protocols, &config, mock_peer_addr);

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
                _tunnel_id: &str,
                _requested_port: Option<u16>,
            ) -> Result<u16, String> {
                Ok(9000)
            }
            fn deallocate(&self, _tunnel_id: &str) {}
            fn get_allocated_port(&self, _tunnel_id: &str) -> Option<u16> {
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

        let spawner: TcpProxySpawner = Arc::new(|_tunnel_id, _port| Box::pin(async { Ok(()) }));

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

        let tunnel_id = "test-tunnel";
        let endpoint = Endpoint {
            protocol: Protocol::Http {
                subdomain: Some("test".to_string()),
            },
            public_url: "https://test.tunnel.test".to_string(),
            port: None,
        };

        let result = handler.register_route(tunnel_id, &endpoint);
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

        let tunnel_id = "test-tunnel";
        let endpoint = Endpoint {
            protocol: Protocol::Https {
                subdomain: Some("secure".to_string()),
            },
            public_url: "https://secure.tunnel.test".to_string(),
            port: None,
        };

        let result = handler.register_route(tunnel_id, &endpoint);
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

        let tunnel_id = "test-tunnel";
        let endpoint = Endpoint {
            protocol: Protocol::Tcp { port: 8080 },
            public_url: "tcp://tunnel.test:8080".to_string(),
            port: Some(8080),
        };

        let result = handler.register_route(tunnel_id, &endpoint);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not supported"));
    }

    #[test]
    fn test_register_route_tcp_with_allocator() {
        struct MockPortAllocator;
        impl PortAllocator for MockPortAllocator {
            fn allocate(
                &self,
                _tunnel_id: &str,
                _requested_port: Option<u16>,
            ) -> Result<u16, String> {
                Ok(9000)
            }
            fn deallocate(&self, _tunnel_id: &str) {}
            fn get_allocated_port(&self, _tunnel_id: &str) -> Option<u16> {
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

        let tunnel_id = "test-tunnel";
        let endpoint = Endpoint {
            protocol: Protocol::Tcp { port: 8080 },
            public_url: "tcp://tunnel.test:8080".to_string(),
            port: Some(8080),
        };

        let result = handler.register_route(tunnel_id, &endpoint);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(9000));
    }

    #[test]
    fn test_unregister_route_http() {
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

        let tunnel_id = "test-tunnel";
        let endpoint = Endpoint {
            protocol: Protocol::Http {
                subdomain: Some("test".to_string()),
            },
            public_url: "https://test.tunnel.test".to_string(),
            port: None,
        };

        // Register first
        handler.register_route(tunnel_id, &endpoint).unwrap();
        assert_eq!(route_registry.count(), 1);

        // Unregister
        handler.unregister_route(tunnel_id, &endpoint);
        assert_eq!(route_registry.count(), 0);
    }

    #[test]
    fn test_unregister_route_tcp() {
        struct MockPortAllocator {
            deallocated: Arc<tokio::sync::Mutex<bool>>,
        }
        impl PortAllocator for MockPortAllocator {
            fn allocate(
                &self,
                _tunnel_id: &str,
                _requested_port: Option<u16>,
            ) -> Result<u16, String> {
                Ok(9000)
            }
            fn deallocate(&self, _tunnel_id: &str) {
                *self.deallocated.blocking_lock() = true;
            }
            fn get_allocated_port(&self, _tunnel_id: &str) -> Option<u16> {
                Some(9000)
            }
        }

        let deallocated = Arc::new(tokio::sync::Mutex::new(false));
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

        let tunnel_id = "test-tunnel";
        let endpoint = Endpoint {
            protocol: Protocol::Tcp { port: 8080 },
            public_url: "tcp://tunnel.test:9000".to_string(),
            port: Some(9000),
        };

        handler.unregister_route(tunnel_id, &endpoint);

        // Verify deallocate was called
        assert!(*deallocated.blocking_lock());
    }
}
