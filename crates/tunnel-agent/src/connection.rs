use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Information about an active forwarding connection
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// Unique identifier for the tunnel
    pub tunnel_id: String,
    /// Stream ID within the tunnel
    pub stream_id: u32,
    /// Remote address being forwarded to
    pub remote_address: String,
    /// Timestamp when connection was established
    pub established_at: std::time::Instant,
}

/// Manages active connections and their lifecycle
#[derive(Clone)]
pub struct ConnectionManager {
    /// Map of stream_id -> ConnectionInfo
    connections: Arc<RwLock<HashMap<u32, ConnectionInfo>>>,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new active connection
    ///
    /// # Arguments
    /// * `stream_id` - The stream ID for this connection
    /// * `info` - Connection information
    pub async fn register(&self, stream_id: u32, info: ConnectionInfo) {
        tracing::debug!(
            stream_id = stream_id,
            tunnel_id = %info.tunnel_id,
            remote_address = %info.remote_address,
            "Registering connection"
        );

        let mut connections = self.connections.write().await;
        connections.insert(stream_id, info);

        tracing::info!(
            active_connections = connections.len(),
            "Connection registered"
        );
    }

    /// Unregister a connection when it's closed
    ///
    /// # Arguments
    /// * `stream_id` - The stream ID to remove
    pub async fn unregister(&self, stream_id: u32) {
        let mut connections = self.connections.write().await;

        if let Some(info) = connections.remove(&stream_id) {
            let duration = info.established_at.elapsed();

            tracing::info!(
                stream_id = stream_id,
                tunnel_id = %info.tunnel_id,
                remote_address = %info.remote_address,
                duration_secs = duration.as_secs(),
                active_connections = connections.len(),
                "Connection unregistered"
            );
        } else {
            tracing::warn!(
                stream_id = stream_id,
                "Attempted to unregister unknown connection"
            );
        }
    }

    /// Get information about a specific connection
    ///
    /// # Arguments
    /// * `stream_id` - The stream ID to look up
    ///
    /// # Returns
    /// Connection info if found
    pub async fn get(&self, stream_id: u32) -> Option<ConnectionInfo> {
        let connections = self.connections.read().await;
        connections.get(&stream_id).cloned()
    }

    /// Get the number of active connections
    pub async fn count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }

    /// Get all active connections
    pub async fn list(&self) -> Vec<ConnectionInfo> {
        let connections = self.connections.read().await;
        connections.values().cloned().collect()
    }

    /// Clear all connections (used for cleanup/shutdown)
    pub async fn clear(&self) {
        let mut connections = self.connections.write().await;
        let count = connections.len();
        connections.clear();

        tracing::info!(cleared_connections = count, "All connections cleared");
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_manager_register_unregister() {
        let manager = ConnectionManager::new();

        let info = ConnectionInfo {
            tunnel_id: "test-tunnel".to_string(),
            stream_id: 1,
            remote_address: "192.168.1.10:8080".to_string(),
            established_at: std::time::Instant::now(),
        };

        // Register
        manager.register(1, info.clone()).await;
        assert_eq!(manager.count().await, 1);

        // Get
        let retrieved = manager.get(1).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().tunnel_id, "test-tunnel");

        // Unregister
        manager.unregister(1).await;
        assert_eq!(manager.count().await, 0);
    }

    #[tokio::test]
    async fn test_connection_manager_list() {
        let manager = ConnectionManager::new();

        for i in 1..=3 {
            let info = ConnectionInfo {
                tunnel_id: format!("tunnel-{}", i),
                stream_id: i,
                remote_address: format!("192.168.1.{}:8080", i),
                established_at: std::time::Instant::now(),
            };
            manager.register(i, info).await;
        }

        let list = manager.list().await;
        assert_eq!(list.len(), 3);
    }

    #[tokio::test]
    async fn test_connection_manager_clear() {
        let manager = ConnectionManager::new();

        for i in 1..=5 {
            let info = ConnectionInfo {
                tunnel_id: format!("tunnel-{}", i),
                stream_id: i,
                remote_address: format!("192.168.1.{}:8080", i),
                established_at: std::time::Instant::now(),
            };
            manager.register(i, info).await;
        }

        assert_eq!(manager.count().await, 5);

        manager.clear().await;
        assert_eq!(manager.count().await, 0);
    }
}
