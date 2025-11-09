//! TLS server with SNI routing
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info};

use localup_cert::generate_self_signed_cert;
use localup_proto::TunnelMessage;
use localup_router::{RouteRegistry, SniRouter};
use localup_transport::{TransportConnection, TransportStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

#[derive(Debug, Error)]
pub enum TlsServerError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Certificate error: {0}")]
    CertificateError(String),

    #[error("SNI extraction failed")]
    SniExtractionFailed,

    #[error("No route found for SNI: {0}")]
    NoRoute(String),

    #[error("Transport error: {0}")]
    TransportError(String),

    #[error("TLS error: {0}")]
    TlsError(String),

    #[error("Failed to bind to {address}: {reason}\n\nTroubleshooting:\n  • Check if another process is using this port: lsof -i :{port}\n  • Try using a different address or port")]
    BindError {
        address: String,
        port: u16,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct TlsServerConfig {
    pub bind_addr: SocketAddr,
}

impl Default for TlsServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:443".parse().unwrap(),
        }
    }
}

pub struct TlsServer {
    config: TlsServerConfig,
    sni_router: Arc<SniRouter>,
}

impl TlsServer {
    /// Create a new TLS server with SNI routing
    pub fn new(config: TlsServerConfig, route_registry: Arc<RouteRegistry>) -> Self {
        let sni_router = Arc::new(SniRouter::new(route_registry));
        Self { config, sni_router }
    }

    /// Get reference to SNI router for registering routes
    pub fn sni_router(&self) -> Arc<SniRouter> {
        self.sni_router.clone()
    }

    /// Start the TLS server
    /// This server accepts incoming TLS connections and routes them based on SNI
    pub async fn start(&self) -> Result<(), TlsServerError> {
        info!("TLS server starting on {}", self.config.bind_addr);

        // Generate self-signed certificate for the relay
        let cert = generate_self_signed_cert()
            .map_err(|e| TlsServerError::CertificateError(e.to_string()))?;

        // Create rustls ServerConfig
        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.cert_der.clone()], cert.key_der)
            .map_err(|e| TlsServerError::TlsError(e.to_string()))?;

        let tls_acceptor = TlsAcceptor::from(Arc::new(server_config));

        // Create TCP listener
        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| {
                let port = self.config.bind_addr.port();
                let address = self.config.bind_addr.ip().to_string();
                let reason = e.to_string();
                TlsServerError::BindError {
                    address,
                    port,
                    reason,
                }
            })?;
        info!(
            "✅ TLS server listening on {} (with SNI-based routing)",
            self.config.bind_addr
        );

        // Accept incoming connections
        loop {
            match listener.accept().await {
                Ok((socket, peer_addr)) => {
                    debug!("New connection from {}", peer_addr);

                    let acceptor = tls_acceptor.clone();
                    let sni_router = self.sni_router.clone();

                    tokio::spawn(async move {
                        match acceptor.accept(socket).await {
                            Ok(mut tls_stream) => {
                                debug!("TLS handshake completed with {}", peer_addr);
                                // Forward the TLS stream to the appropriate backend
                                if let Err(e) = Self::forward_tls_stream(
                                    &mut tls_stream,
                                    &sni_router,
                                    peer_addr,
                                )
                                .await
                                {
                                    error!("Error forwarding TLS stream: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("TLS handshake error from {}: {}", peer_addr, e);
                            }
                        }
                    });
                }
                Err(e) => {
                    error!("TLS listener accept error: {}", e);
                }
            }
        }
    }

    /// Forward TLS stream to backend based on SNI
    async fn forward_tls_stream(
        tls_stream: &mut (impl AsyncReadExt + AsyncWriteExt + Unpin),
        _sni_router: &Arc<SniRouter>,
        peer_addr: SocketAddr,
    ) -> Result<(), TlsServerError> {
        // For SNI-based routing, we need to extract the SNI from the connection
        // In tokio_rustls, the ServerConnection holds the SNI, but we need to get it from somewhere else
        // For now, we'll use a simple approach: read the first bytes and look for SNI in ClientHello
        // This is a simplified version - a full implementation would properly extract SNI

        debug!("Forwarding TLS stream from {} through router", peer_addr);

        // Read data from the TLS stream and check for SNI
        let mut buf = [0; 1024];
        match tls_stream.read(&mut buf).await {
            Ok(0) => {
                debug!("TLS stream closed by client");
                Ok(())
            }
            Ok(n) => {
                debug!("Received {} bytes from TLS stream", n);
                // In a full implementation, we would:
                // 1. Extract SNI from the TLS connection metadata
                // 2. Look up the route
                // 3. Create a connection to the backend
                // 4. Bidirectionally forward data
                // For now, just close the connection gracefully
                Ok(())
            }
            Err(e) => Err(TlsServerError::IoError(e)),
        }
    }

    /// Handle a single TLS connection
    #[allow(dead_code)]
    async fn handle_connection<C: TransportConnection>(
        connection: C,
        sni_router: Arc<SniRouter>,
        _peer_addr: SocketAddr,
    ) -> Result<(), TlsServerError> {
        // Accept a stream from the connection
        if let Ok(Some(mut stream)) = connection.accept_stream().await {
            if let Ok(Some(TunnelMessage::TlsConnect {
                stream_id,
                sni,
                client_hello,
            })) = stream.recv_message().await
            {
                // Verify SNI from ClientHello matches
                match localup_router::SniRouter::extract_sni(&client_hello) {
                    Ok(extracted_sni) => {
                        debug!(
                            "Extracted SNI: {} from ClientHello for stream {}",
                            extracted_sni, stream_id
                        );

                        // Lookup route for this SNI
                        match sni_router.lookup(&sni) {
                            Ok(target) => {
                                info!(
                                    "SNI {} routed to tunnel {} ({})",
                                    sni, target.localup_id, target.target_addr
                                );

                                // Forward the TLS connection to the appropriate tunnel
                                Self::forward_tls_connection(
                                    stream,
                                    &target.target_addr,
                                    client_hello,
                                    stream_id,
                                )
                                .await?;
                            }
                            Err(_) => {
                                error!("No route found for SNI: {}", sni);
                                return Err(TlsServerError::NoRoute(sni));
                            }
                        }
                    }
                    Err(_) => {
                        debug!(
                            "Could not extract SNI from ClientHello, using provided SNI: {}",
                            sni
                        );
                        // Fallback to provided SNI if extraction fails

                        match sni_router.lookup(&sni) {
                            Ok(target) => {
                                info!(
                                    "SNI {} routed to tunnel {} ({})",
                                    sni, target.localup_id, target.target_addr
                                );

                                Self::forward_tls_connection(
                                    stream,
                                    &target.target_addr,
                                    client_hello,
                                    stream_id,
                                )
                                .await?;
                            }
                            Err(_) => {
                                error!("No route found for SNI: {}", sni);
                                return Err(TlsServerError::NoRoute(sni));
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Forward TLS connection to the target tunnel
    #[allow(dead_code)]
    async fn forward_tls_connection<S: TransportStream>(
        mut stream: S,
        _target_addr: &str,
        client_hello: Vec<u8>,
        stream_id: u32,
    ) -> Result<(), TlsServerError> {
        // Send TlsData with the ClientHello
        let msg = TunnelMessage::TlsData {
            stream_id,
            data: client_hello,
        };

        stream
            .send_message(&msg)
            .await
            .map_err(|e| TlsServerError::TransportError(e.to_string()))?;

        // Keep the stream open for bidirectional forwarding
        // (This would continue in a real implementation)

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_server_config() {
        let config = TlsServerConfig::default();
        assert_eq!(config.bind_addr.port(), 443);
    }

    #[test]
    fn test_tls_server_creation() {
        let config = TlsServerConfig::default();
        let route_registry = Arc::new(RouteRegistry::new());
        let server = TlsServer::new(config, route_registry);

        // Verify server was created
        assert_eq!(server.config.bind_addr.port(), 443);
    }

    #[tokio::test]
    async fn test_sni_routing() {
        use localup_router::{RouteKey, RouteTarget};

        let route_registry = Arc::new(RouteRegistry::new());

        // Register a route for example.com
        let key = RouteKey::TlsSni("example.com".to_string());
        let target = RouteTarget {
            localup_id: "test-tunnel".to_string(),
            target_addr: "localhost:9443".to_string(),
            metadata: None,
        };
        route_registry.register(key, target).unwrap();

        let config = TlsServerConfig::default();
        let server = TlsServer::new(config, route_registry);

        // Verify SNI router has the route
        assert!(server.sni_router().has_route("example.com"));
        assert!(!server.sni_router().has_route("unknown.com"));
    }
}
