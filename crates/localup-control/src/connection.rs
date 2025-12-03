//! Tunnel connection management

use localup_http_auth::HttpAuthenticator;
use localup_proto::{Endpoint, HttpAuthConfig};
use localup_transport_quic::QuicConnection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Callback for handling TCP data from tunnel to proxy
pub type TcpDataCallback = Arc<
    dyn Fn(u32, Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

/// Represents an active tunnel connection
pub struct TunnelConnection {
    pub localup_id: String,
    pub endpoints: Vec<Endpoint>,
    pub connection: Arc<QuicConnection>, // ✅ Store connection instead of sender
    pub tcp_data_callback: Option<TcpDataCallback>,
    /// HTTP authentication configuration for this tunnel
    pub http_auth: HttpAuthConfig,
}

/// Manages all active tunnel connections
pub struct TunnelConnectionManager {
    connections: Arc<RwLock<HashMap<String, TunnelConnection>>>,
}

impl TunnelConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new tunnel connection
    pub async fn register(
        &self,
        localup_id: String,
        endpoints: Vec<Endpoint>,
        connection: Arc<QuicConnection>,
    ) {
        self.register_with_auth(localup_id, endpoints, connection, HttpAuthConfig::None)
            .await;
    }

    /// Register a new tunnel connection with HTTP authentication configuration
    pub async fn register_with_auth(
        &self,
        localup_id: String,
        endpoints: Vec<Endpoint>,
        connection: Arc<QuicConnection>,
        http_auth: HttpAuthConfig,
    ) {
        let localup_conn = TunnelConnection {
            localup_id: localup_id.clone(),
            endpoints,
            connection,
            tcp_data_callback: None,
            http_auth,
        };

        self.connections
            .write()
            .await
            .insert(localup_id, localup_conn);
    }

    /// Register a TCP data callback for a tunnel
    pub async fn register_tcp_callback(&self, localup_id: &str, callback: TcpDataCallback) {
        if let Some(conn) = self.connections.write().await.get_mut(localup_id) {
            conn.tcp_data_callback = Some(callback);
        }
    }

    /// Get the TCP data callback for a tunnel
    pub async fn get_tcp_callback(&self, localup_id: &str) -> Option<TcpDataCallback> {
        self.connections
            .read()
            .await
            .get(localup_id)
            .and_then(|conn| conn.tcp_data_callback.clone())
    }

    /// Unregister a tunnel connection
    pub async fn unregister(&self, localup_id: &str) {
        self.connections.write().await.remove(localup_id);
    }

    /// Get a tunnel connection by ID
    pub async fn get(&self, localup_id: &str) -> Option<Arc<QuicConnection>> {
        self.connections
            .read()
            .await
            .get(localup_id)
            .map(|conn| conn.connection.clone())
    }

    /// List all active tunnel IDs
    pub async fn list_tunnels(&self) -> Vec<String> {
        self.connections.read().await.keys().cloned().collect()
    }

    /// Get all endpoints for a tunnel
    pub async fn get_endpoints(&self, localup_id: &str) -> Option<Vec<Endpoint>> {
        self.connections
            .read()
            .await
            .get(localup_id)
            .map(|conn| conn.endpoints.clone())
    }

    /// Get the HTTP authenticator for a tunnel
    ///
    /// Returns an `HttpAuthenticator` configured with the tunnel's authentication settings.
    /// If no auth is configured, returns an authenticator that allows all requests.
    pub async fn get_http_authenticator(&self, localup_id: &str) -> Option<HttpAuthenticator> {
        self.connections
            .read()
            .await
            .get(localup_id)
            .map(|conn| HttpAuthenticator::from_config(&conn.http_auth))
    }

    /// Get the raw HTTP auth configuration for a tunnel
    pub async fn get_http_auth_config(&self, localup_id: &str) -> Option<HttpAuthConfig> {
        self.connections
            .read()
            .await
            .get(localup_id)
            .map(|conn| conn.http_auth.clone())
    }
}

impl Default for TunnelConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents an active agent connection
pub struct AgentConnection {
    pub agent_id: String,
    pub connection: Arc<QuicConnection>,
}

/// Manages all active agent connections for reverse tunnels
pub struct AgentConnectionManager {
    connections: Arc<RwLock<HashMap<String, AgentConnection>>>,
}

impl AgentConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new agent connection
    pub async fn register(&self, agent_id: String, connection: Arc<QuicConnection>) {
        let agent_conn = AgentConnection {
            agent_id: agent_id.clone(),
            connection,
        };

        tracing::debug!("Agent connection registered: {}", agent_id);

        self.connections.write().await.insert(agent_id, agent_conn);
    }

    /// Unregister an agent connection
    pub async fn unregister(&self, agent_id: &str) {
        self.connections.write().await.remove(agent_id);
        tracing::debug!("Agent connection unregistered: {}", agent_id);
    }

    /// Get an agent connection by ID
    pub async fn get(&self, agent_id: &str) -> Option<Arc<QuicConnection>> {
        self.connections
            .read()
            .await
            .get(agent_id)
            .map(|conn| conn.connection.clone())
    }

    /// List all connected agent IDs
    pub async fn list_agents(&self) -> Vec<String> {
        self.connections.read().await.keys().cloned().collect()
    }

    /// Get the count of connected agents
    pub async fn count(&self) -> usize {
        self.connections.read().await.len()
    }
}

impl Default for AgentConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock connection creation removed - use integration tests for real QUIC connections

    #[tokio::test]
    async fn test_connection_manager_new() {
        let manager = TunnelConnectionManager::new();
        assert_eq!(manager.list_tunnels().await.len(), 0);
    }

    // Most tests require actual QUIC connections and will be in integration tests

    #[tokio::test]
    async fn test_get_nonexistent_tunnel() {
        let manager = TunnelConnectionManager::new();
        let result = manager.get("nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_endpoints_nonexistent() {
        let manager = TunnelConnectionManager::new();
        let result = manager.get_endpoints("nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_tcp_callback_nonexistent() {
        let manager = TunnelConnectionManager::new();
        let result = manager.get_tcp_callback("nonexistent").await;
        assert!(result.is_none());
    }
}
