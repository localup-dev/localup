//! TCP Proxy Server Implementation
//!
//! Listens on a specific port and forwards all TCP data through a tunnel.
//! Each tunnel gets its own dedicated TcpProxyServer instance.

use sea_orm::DatabaseConnection;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};
use tunnel_control::TunnelConnectionManager;
use tunnel_proto::TunnelMessage;
use tunnel_transport::{TransportConnection, TransportStream};

#[derive(Debug, Error)]
pub enum TcpProxyServerError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Port allocation failed: {0}")]
    PortAllocationError(String),

    #[error("Tunnel error: {0}")]
    TunnelError(String),
}

#[derive(Debug, Clone)]
pub struct TcpProxyServerConfig {
    pub bind_addr: SocketAddr,
    pub tunnel_id: String,
}

/// Simple stream ID generator for logging/metrics
#[derive(Clone)]
pub struct StreamIdGenerator {
    next_stream_id: Arc<AtomicU32>,
}

impl StreamIdGenerator {
    pub fn new() -> Self {
        Self {
            next_stream_id: Arc::new(AtomicU32::new(1)),
        }
    }

    pub fn generate(&self) -> u32 {
        self.next_stream_id.fetch_add(1, Ordering::SeqCst)
    }
}

/// Tracks metrics for an individual TCP connection
struct ConnectionMetrics {
    connection_id: String,
    bytes_received: Arc<AtomicU64>,
    bytes_sent: Arc<AtomicU64>,
    connected_at: chrono::DateTime<chrono::Utc>,
}

pub struct TcpProxyServer {
    config: TcpProxyServerConfig,
    tunnel_manager: Arc<TunnelConnectionManager>,
    stream_id_gen: StreamIdGenerator,
    db: Option<DatabaseConnection>,
}

impl TcpProxyServer {
    pub fn new(config: TcpProxyServerConfig, tunnel_manager: Arc<TunnelConnectionManager>) -> Self {
        Self {
            config,
            tunnel_manager,
            stream_id_gen: StreamIdGenerator::new(),
            db: None,
        }
    }

    pub fn with_database(mut self, db: DatabaseConnection) -> Self {
        self.db = Some(db);
        self
    }

    pub async fn start(self) -> Result<(), TcpProxyServerError> {
        let listener = TcpListener::bind(&self.config.bind_addr).await?;
        let addr = listener.local_addr()?;
        let target_port = addr.port();

        info!(
            "TCP proxy server listening on {} for tunnel {}",
            addr, self.config.tunnel_id
        );

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    debug!(
                        "New TCP connection from {} for tunnel {}",
                        peer_addr, self.config.tunnel_id
                    );

                    let tunnel_id = self.config.tunnel_id.clone();
                    let tunnel_manager = self.tunnel_manager.clone();
                    let stream_id_gen = self.stream_id_gen.clone();
                    let db = self.db.clone();

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_tcp_connection(
                            stream,
                            peer_addr,
                            tunnel_id,
                            target_port,
                            tunnel_manager,
                            stream_id_gen,
                            db,
                        )
                        .await
                        {
                            error!("Error handling TCP connection from {}: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept TCP connection: {}", e);
                }
            }
        }
    }

    async fn handle_tcp_connection(
        client_stream: TcpStream,
        peer_addr: SocketAddr,
        tunnel_id: String,
        target_port: u16,
        tunnel_manager: Arc<TunnelConnectionManager>,
        stream_id_gen: StreamIdGenerator,
        db: Option<DatabaseConnection>,
    ) -> Result<(), TcpProxyServerError> {
        // Get tunnel QUIC connection (not sender!)
        let tunnel_connection = match tunnel_manager.get(&tunnel_id).await {
            Some(conn) => conn,
            None => {
                warn!("Tunnel {} not found", tunnel_id);
                return Err(TcpProxyServerError::TunnelError(
                    "Tunnel not connected".to_string(),
                ));
            }
        };

        // Open a NEW QUIC stream for this TCP connection
        let mut quic_stream = match tunnel_connection.open_stream().await {
            Ok(stream) => stream,
            Err(e) => {
                error!("Failed to open QUIC stream for TCP connection: {}", e);
                return Err(TcpProxyServerError::TunnelError(format!(
                    "Failed to open stream: {}",
                    e
                )));
            }
        };

        // Generate stream ID for logging/metrics (QUIC stream ID is separate)
        let stream_id = stream_id_gen.generate();

        debug!(
            "TCP connection {} established for tunnel {} (stream {}, QUIC stream {})",
            peer_addr,
            tunnel_id,
            stream_id,
            quic_stream.stream_id()
        );

        // Create connection metrics tracker
        let connection_id = uuid::Uuid::new_v4().to_string();
        let metrics = ConnectionMetrics {
            connection_id: connection_id.clone(),
            bytes_received: Arc::new(AtomicU64::new(0)),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            connected_at: chrono::Utc::now(),
        };

        // Send TcpConnect message on THIS stream
        let connect_msg = TunnelMessage::TcpConnect {
            stream_id,
            remote_addr: peer_addr.ip().to_string(),
            remote_port: peer_addr.port(),
        };

        debug!(
            "📨 Sending TcpConnect on QUIC stream {} for tunnel {}",
            quic_stream.stream_id(),
            tunnel_id
        );
        if let Err(e) = quic_stream.send_message(&connect_msg).await {
            error!("Failed to send TcpConnect: {}", e);
            return Err(TcpProxyServerError::TunnelError(format!(
                "Send error: {}",
                e
            )));
        }

        debug!("✅ TcpConnect sent on stream {}", quic_stream.stream_id());

        // Split BOTH streams for true bidirectional communication WITHOUT MUTEXES!
        let (mut client_read, mut client_write) = client_stream.into_split();
        let (mut quic_send, mut quic_recv) = quic_stream.split();

        // Task to read from TCP client and send to QUIC stream
        // Now owns quic_send exclusively - no mutex needed!
        let bytes_received_clone = metrics.bytes_received.clone();
        let client_to_tunnel = tokio::spawn(async move {
            let mut buffer = vec![0u8; 8192];
            loop {
                match client_read.read(&mut buffer).await {
                    Ok(0) => {
                        // Client closed connection
                        debug!("Client closed TCP connection (stream {})", stream_id);
                        let close_msg = TunnelMessage::TcpClose { stream_id };
                        let _ = quic_send.send_message(&close_msg).await;
                        let _ = quic_send.finish().await;
                        break;
                    }
                    Ok(n) => {
                        debug!("Read {} bytes from TCP client (stream {})", n, stream_id);

                        // Track bytes received from client
                        bytes_received_clone.fetch_add(n as u64, Ordering::Relaxed);

                        // Send data on QUIC stream - NO MUTEX!
                        let data_msg = TunnelMessage::TcpData {
                            stream_id,
                            data: buffer[..n].to_vec(),
                        };

                        debug!(
                            "Sending {} bytes to tunnel via QUIC (stream {})",
                            n, stream_id
                        );
                        if let Err(e) = quic_send.send_message(&data_msg).await {
                            error!("Failed to send TcpData on QUIC stream: {}", e);
                            break;
                        }
                        debug!("✅ TcpData sent successfully (stream {})", stream_id);
                    }
                    Err(e) => {
                        error!("Error reading from TCP client: {}", e);
                        break;
                    }
                }
            }
        });

        // Task to receive from QUIC stream and send to TCP client
        // Now owns quic_recv exclusively - no mutex needed!
        let bytes_sent_clone = metrics.bytes_sent.clone();
        let client_to_tunnel_handle = client_to_tunnel.abort_handle();
        let tunnel_to_client = tokio::spawn(async move {
            loop {
                // NO MUTEX - direct access to quic_recv!
                let msg = quic_recv.recv_message().await;

                match msg {
                    Ok(Some(TunnelMessage::TcpData { stream_id: _, data })) => {
                        if data.is_empty() {
                            // Empty data means close
                            debug!("Received close signal from tunnel (stream {})", stream_id);
                            client_to_tunnel_handle.abort();
                            break;
                        }

                        debug!(
                            "Received {} bytes from tunnel (stream {})",
                            data.len(),
                            stream_id
                        );

                        // Track bytes sent to client
                        bytes_sent_clone.fetch_add(data.len() as u64, Ordering::Relaxed);

                        if let Err(e) = client_write.write_all(&data).await {
                            error!("Failed to write to TCP client: {}", e);
                            break;
                        }

                        if let Err(e) = client_write.flush().await {
                            error!("Failed to flush TCP client stream: {}", e);
                            break;
                        }
                    }
                    Ok(Some(TunnelMessage::TcpClose { stream_id: _ })) => {
                        debug!("Received TcpClose from tunnel (stream {})", stream_id);
                        break;
                    }
                    Ok(None) => {
                        debug!("QUIC stream closed (stream {})", stream_id);
                        break;
                    }
                    Err(e) => {
                        error!("Error reading from QUIC stream: {}", e);
                        break;
                    }
                    Ok(Some(msg)) => {
                        warn!("Unexpected message on QUIC stream: {:?}", msg);
                    }
                }
            }
        });

        // Wait for both tasks to complete
        let _ = tokio::join!(client_to_tunnel, tunnel_to_client);

        debug!("TCP connection closed (stream {})", stream_id);

        // Save connection metrics to database
        if let Some(ref db_conn) = db {
            let disconnected_at = chrono::Utc::now();
            let duration_ms = (disconnected_at - metrics.connected_at).num_milliseconds() as i32;
            let bytes_received = metrics.bytes_received.load(Ordering::Relaxed) as i64;
            let bytes_sent = metrics.bytes_sent.load(Ordering::Relaxed) as i64;

            let captured_connection =
                tunnel_relay_db::entities::captured_tcp_connection::ActiveModel {
                    id: sea_orm::Set(metrics.connection_id.clone()),
                    tunnel_id: sea_orm::Set(tunnel_id.clone()),
                    client_addr: sea_orm::Set(peer_addr.to_string()),
                    target_port: sea_orm::Set(target_port as i32),
                    bytes_received: sea_orm::Set(bytes_received),
                    bytes_sent: sea_orm::Set(bytes_sent),
                    connected_at: sea_orm::Set(metrics.connected_at.into()),
                    disconnected_at: sea_orm::Set(Some(disconnected_at.into())),
                    duration_ms: sea_orm::Set(Some(duration_ms)),
                    disconnect_reason: sea_orm::Set(Some("client_closed".to_string())),
                };

            use sea_orm::EntityTrait;
            if let Err(e) = tunnel_relay_db::entities::prelude::CapturedTcpConnection::insert(
                captured_connection,
            )
            .exec(db_conn)
            .await
            {
                warn!(
                    "Failed to save TCP connection {}: {}",
                    metrics.connection_id, e
                );
            } else {
                debug!(
                    "Captured TCP connection {} to database",
                    metrics.connection_id
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcp_proxy_server_config() {
        let config = TcpProxyServerConfig {
            bind_addr: "127.0.0.1:8080".parse().unwrap(),
            tunnel_id: "test-tunnel".to_string(),
        };
        assert_eq!(config.bind_addr.port(), 8080);
        assert_eq!(config.tunnel_id, "test-tunnel");
    }

    #[test]
    fn test_stream_id_generator() {
        let gen = StreamIdGenerator::new();
        let stream_id = gen.generate();
        assert_eq!(stream_id, 1);

        let stream_id2 = gen.generate();
        assert_eq!(stream_id2, 2);
    }
}
