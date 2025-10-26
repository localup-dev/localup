//! Tunnel connection management

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tunnel_proto::Endpoint;
use tunnel_transport_quic::QuicConnection;

/// Callback for handling TCP data from tunnel to proxy
pub type TcpDataCallback = Arc<
    dyn Fn(u32, Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

/// Represents an active tunnel connection
pub struct TunnelConnection {
    pub tunnel_id: String,
    pub endpoints: Vec<Endpoint>,
    pub connection: Arc<QuicConnection>, // ✅ Store connection instead of sender
    pub tcp_data_callback: Option<TcpDataCallback>,
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
        tunnel_id: String,
        endpoints: Vec<Endpoint>,
        connection: Arc<QuicConnection>,
    ) {
        let tunnel_conn = TunnelConnection {
            tunnel_id: tunnel_id.clone(),
            endpoints,
            connection,
            tcp_data_callback: None,
        };

        self.connections
            .write()
            .await
            .insert(tunnel_id, tunnel_conn);
    }

    /// Register a TCP data callback for a tunnel
    pub async fn register_tcp_callback(&self, tunnel_id: &str, callback: TcpDataCallback) {
        if let Some(conn) = self.connections.write().await.get_mut(tunnel_id) {
            conn.tcp_data_callback = Some(callback);
        }
    }

    /// Get the TCP data callback for a tunnel
    pub async fn get_tcp_callback(&self, tunnel_id: &str) -> Option<TcpDataCallback> {
        self.connections
            .read()
            .await
            .get(tunnel_id)
            .and_then(|conn| conn.tcp_data_callback.clone())
    }

    /// Unregister a tunnel connection
    pub async fn unregister(&self, tunnel_id: &str) {
        self.connections.write().await.remove(tunnel_id);
    }

    /// Get a tunnel connection by ID
    pub async fn get(&self, tunnel_id: &str) -> Option<Arc<QuicConnection>> {
        self.connections
            .read()
            .await
            .get(tunnel_id)
            .map(|conn| conn.connection.clone())
    }

    /// List all active tunnel IDs
    pub async fn list_tunnels(&self) -> Vec<String> {
        self.connections.read().await.keys().cloned().collect()
    }

    /// Get all endpoints for a tunnel
    pub async fn get_endpoints(&self, tunnel_id: &str) -> Option<Vec<Endpoint>> {
        self.connections
            .read()
            .await
            .get(tunnel_id)
            .map(|conn| conn.endpoints.clone())
    }
}

impl Default for TunnelConnectionManager {
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
