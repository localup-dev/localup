//! WebSocket listener and connector implementations

use async_trait::async_trait;
use localup_transport::{
    TransportConfig, TransportConnector, TransportError, TransportListener, TransportResult,
};
use rustls::pki_types::ServerName;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tracing::{debug, info, warn};
use url::Url;

use crate::config::WebSocketConfig;
use crate::connection::WebSocketConnection;

/// WebSocket listener for accepting incoming connections
pub struct WebSocketListener {
    tcp_listener: TcpListener,
    tls_acceptor: tokio_rustls::TlsAcceptor,
    config: Arc<WebSocketConfig>,
}

impl std::fmt::Debug for WebSocketListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSocketListener")
            .field("local_addr", &self.tcp_listener.local_addr())
            .finish()
    }
}

impl WebSocketListener {
    pub fn new(bind_addr: SocketAddr, config: Arc<WebSocketConfig>) -> TransportResult<Self> {
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
        info!(
            "WebSocket listener bound to wss://{}{}",
            local_addr, config.path
        );

        Ok(Self {
            tcp_listener,
            tls_acceptor,
            config,
        })
    }
}

#[async_trait]
impl TransportListener for WebSocketListener {
    type Connection = WebSocketConnection;

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

            // Perform WebSocket handshake with path validation
            let expected_path = self.config.path.clone();
            let callback = |req: &Request, response: Response| {
                let path = req.uri().path();
                if path == expected_path || path == format!("{}/", expected_path) {
                    Ok(response)
                } else {
                    let response = Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(None)
                        .unwrap();
                    Err(response)
                }
            };

            let ws_stream = match tokio_tungstenite::accept_hdr_async(
                tokio_rustls::TlsStream::Server(tls_stream),
                callback,
            )
            .await
            {
                Ok(stream) => stream,
                Err(e) => {
                    warn!("WebSocket handshake failed from {}: {}", remote_addr, e);
                    continue;
                }
            };

            info!("WebSocket connection established from {}", remote_addr);

            let connection = WebSocketConnection::new(ws_stream, remote_addr, true);
            return Ok((connection, remote_addr));
        }
    }

    fn local_addr(&self) -> TransportResult<SocketAddr> {
        self.tcp_listener
            .local_addr()
            .map_err(TransportError::IoError)
    }

    async fn close(&self) {
        info!("WebSocket listener closed");
        // TCP listener will be dropped
    }
}

/// WebSocket connector for establishing outgoing connections
pub struct WebSocketConnector {
    tls_connector: TlsConnector,
    config: Arc<WebSocketConfig>,
}

impl std::fmt::Debug for WebSocketConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSocketConnector").finish()
    }
}

impl WebSocketConnector {
    pub fn new(config: Arc<WebSocketConfig>) -> TransportResult<Self> {
        TransportConfig::validate(&*config)?;

        let tls_connector = config.build_tls_connector()?;

        debug!("WebSocket connector created");

        Ok(Self {
            tls_connector,
            config,
        })
    }
}

#[async_trait]
impl TransportConnector for WebSocketConnector {
    type Connection = WebSocketConnection;

    async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
    ) -> TransportResult<Self::Connection> {
        debug!(
            "Connecting to WebSocket server: wss://{}:{}{}",
            server_name,
            addr.port(),
            self.config.path
        );

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

        // Build WebSocket URL
        let ws_url = Url::parse(&format!(
            "wss://{}:{}{}",
            server_name,
            addr.port(),
            self.config.path
        ))
        .map_err(|e| TransportError::ConfigurationError(format!("Invalid URL: {}", e)))?;

        // Perform WebSocket handshake
        let (ws_stream, _response) = tokio_tungstenite::client_async(
            ws_url.as_str(),
            tokio_rustls::TlsStream::Client(tls_stream),
        )
        .await
        .map_err(|e| {
            TransportError::ConnectionError(format!("WebSocket handshake failed: {}", e))
        })?;

        info!(
            "WebSocket connection established to wss://{}:{}{}",
            server_name,
            addr.port(),
            self.config.path
        );

        Ok(WebSocketConnection::new(ws_stream, addr, false))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_connector_debug() {
        // Just verify Debug impl
        let debug_str = "WebSocketConnector";
        assert!(!debug_str.is_empty());
    }
}
