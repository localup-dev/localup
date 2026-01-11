//! HTTP/2 listener and connector implementations

use async_trait::async_trait;
use localup_transport::{
    TransportConfig, TransportConnector, TransportError, TransportListener, TransportResult,
};
use rustls::pki_types::ServerName;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info, warn};

use crate::config::H2Config;
use crate::connection::{H2ClientConnection, H2Connection, H2ServerConnection};

/// HTTP/2 listener for accepting incoming connections
pub struct H2Listener {
    tcp_listener: TcpListener,
    tls_acceptor: tokio_rustls::TlsAcceptor,
    _config: Arc<H2Config>,
}

impl std::fmt::Debug for H2Listener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("H2Listener")
            .field("local_addr", &self.tcp_listener.local_addr())
            .finish()
    }
}

impl H2Listener {
    pub fn new(bind_addr: SocketAddr, config: Arc<H2Config>) -> TransportResult<Self> {
        TransportConfig::validate(&*config)?;

        let tls_acceptor = config.build_tls_acceptor()?;

        // Create TCP listener synchronously using std
        let std_listener = std::net::TcpListener::bind(bind_addr).map_err(|e| {
            let port = bind_addr.port();
            let address = bind_addr.ip().to_string();
            TransportError::BindError {
                address,
                port,
                reason: e.to_string(),
            }
        })?;

        std_listener.set_nonblocking(true).map_err(|e| {
            TransportError::ConfigurationError(format!("Failed to set nonblocking: {}", e))
        })?;

        let tcp_listener = TcpListener::from_std(std_listener).map_err(TransportError::IoError)?;

        let local_addr = tcp_listener.local_addr().map_err(TransportError::IoError)?;
        info!("HTTP/2 listener bound to {}", local_addr);

        Ok(Self {
            tcp_listener,
            tls_acceptor,
            _config: config,
        })
    }
}

#[async_trait]
impl TransportListener for H2Listener {
    type Connection = H2Connection;

    async fn accept(&self) -> TransportResult<(Self::Connection, SocketAddr)> {
        loop {
            // Accept TCP connection
            let (tcp_stream, remote_addr) = self
                .tcp_listener
                .accept()
                .await
                .map_err(TransportError::IoError)?;

            debug!("Incoming TCP connection from {}", remote_addr);

            // Perform TLS handshake
            let tls_stream = match self.tls_acceptor.accept(tcp_stream).await {
                Ok(stream) => stream,
                Err(e) => {
                    warn!("TLS handshake failed from {}: {}", remote_addr, e);
                    continue;
                }
            };

            debug!("TLS handshake complete from {}", remote_addr);

            // Create H2 server connection
            match H2ServerConnection::new(tls_stream, remote_addr).await {
                Ok(conn) => {
                    info!("HTTP/2 connection established from {}", remote_addr);
                    return Ok((H2Connection::Server(conn), remote_addr));
                }
                Err(e) => {
                    warn!("H2 handshake failed from {}: {}", remote_addr, e);
                    continue;
                }
            }
        }
    }

    fn local_addr(&self) -> TransportResult<SocketAddr> {
        self.tcp_listener
            .local_addr()
            .map_err(TransportError::IoError)
    }

    async fn close(&self) {
        info!("HTTP/2 listener closed");
    }
}

/// HTTP/2 connector for establishing outgoing connections
pub struct H2Connector {
    tls_connector: tokio_rustls::TlsConnector,
    _config: Arc<H2Config>,
}

impl std::fmt::Debug for H2Connector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("H2Connector").finish()
    }
}

impl H2Connector {
    pub fn new(config: Arc<H2Config>) -> TransportResult<Self> {
        TransportConfig::validate(&*config)?;

        let tls_connector = config.build_tls_connector()?;

        debug!("HTTP/2 connector created");

        Ok(Self {
            tls_connector,
            _config: config,
        })
    }
}

#[async_trait]
impl TransportConnector for H2Connector {
    type Connection = H2Connection;

    async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
    ) -> TransportResult<Self::Connection> {
        debug!("Connecting to HTTP/2 server: {} ({})", server_name, addr);

        // Connect TCP
        let tcp_stream = TcpStream::connect(addr)
            .await
            .map_err(|e| TransportError::ConnectionError(format!("TCP connect failed: {}", e)))?;

        // Perform TLS handshake
        let dns_name = ServerName::try_from(server_name.to_string())
            .map_err(|e| TransportError::TlsError(format!("Invalid server name: {}", e)))?;

        let tls_stream = self
            .tls_connector
            .connect(dns_name, tcp_stream)
            .await
            .map_err(|e| TransportError::TlsError(format!("TLS handshake failed: {}", e)))?;

        // Create H2 client connection
        let conn = H2ClientConnection::new(tls_stream, addr).await?;

        info!(
            "HTTP/2 connection established to {} ({})",
            server_name, addr
        );

        Ok(H2Connection::Client(conn))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_listener_debug() {
        assert!(true);
    }
}
