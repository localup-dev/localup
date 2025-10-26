//! Tunnel client implementation

use crate::config::TunnelConfig;
use crate::metrics::MetricsStore;
use crate::tunnel::{TunnelConnection, TunnelConnector};
use thiserror::Error;
use tunnel_proto::Endpoint;

/// Tunnel client errors
#[derive(Debug, Error)]
pub enum TunnelError {
    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Tunnel closed: {0}")]
    TunnelClosed(String),
}

impl TunnelError {
    /// Returns true if this error is non-recoverable and retrying won't help
    pub fn is_non_recoverable(&self) -> bool {
        matches!(
            self,
            TunnelError::AuthenticationFailed(_) | TunnelError::ConfigError(_)
        )
    }

    /// Returns true if this error is recoverable and retrying might succeed
    pub fn is_recoverable(&self) -> bool {
        !self.is_non_recoverable()
    }
}

/// Tunnel client
pub struct TunnelClient {
    connection: TunnelConnection,
}

impl TunnelClient {
    /// Connect to tunnel service
    pub async fn connect(config: TunnelConfig) -> Result<Self, TunnelError> {
        // Use TunnelConnector to establish connection
        let connector = TunnelConnector::new(config);
        let connection = connector.connect().await?;

        Ok(Self { connection })
    }

    /// Get the public endpoints
    pub fn endpoints(&self) -> &[Endpoint] {
        self.connection.endpoints()
    }

    /// Get the first public URL (convenience method)
    pub fn public_url(&self) -> Option<&str> {
        self.connection.public_url()
    }

    /// Get the tunnel ID
    pub fn tunnel_id(&self) -> &str {
        self.connection.tunnel_id()
    }

    /// Get access to metrics store
    pub fn metrics(&self) -> &MetricsStore {
        self.connection.metrics()
    }

    /// Send graceful disconnect message (does not consume self)
    pub async fn disconnect(&self) -> Result<(), TunnelError> {
        self.connection.disconnect().await
    }

    /// Get a handle that can send disconnect without owning the client
    /// This allows sending disconnect before calling wait()
    pub fn disconnect_handle(
        &self,
    ) -> impl std::future::Future<Output = Result<(), TunnelError>> + Send + 'static {
        let connection = self.connection.clone();
        async move { connection.disconnect().await }
    }

    /// Wait for tunnel to close
    pub async fn wait(self) -> Result<(), TunnelError> {
        // Run the tunnel connection loop
        self.connection.run().await
    }

    /// Close the tunnel gracefully
    pub async fn close(self) -> Result<(), TunnelError> {
        // Send disconnect message to exit node for immediate cleanup
        self.connection.disconnect().await?;

        // Give the disconnect message time to be sent
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // The connection will be dropped, closing the socket
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProtocolConfig;
    use tunnel_proto::ExitNodeConfig;

    #[tokio::test]
    #[ignore] // Requires a running exit node
    async fn test_tunnel_client_connection() {
        let config = TunnelConfig::builder()
            .protocol(ProtocolConfig::Http {
                local_port: 3000,
                subdomain: Some("test".to_string()),
            })
            .auth_token("test-token".to_string())
            .exit_node(ExitNodeConfig::Custom("localhost:9000".to_string()))
            .build()
            .unwrap();

        let client = TunnelClient::connect(config).await.unwrap();
        assert!(client.public_url().is_some());
    }
}
