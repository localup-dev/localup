//! HTTP/2 connection implementation

use async_trait::async_trait;
use bytes::Bytes;
use h2::client::SendRequest;
use h2::server::SendResponse;
use h2::RecvStream;
use localup_transport::{ConnectionStats, TransportConnection, TransportError, TransportResult};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error};

use crate::stream::H2Stream;

/// Server-side HTTP/2 connection
pub struct H2ServerConnection {
    connection_id: String,
    remote_addr: SocketAddr,
    /// Channel for accepting new streams
    accept_rx: Mutex<mpsc::Receiver<(SendResponse<Bytes>, RecvStream)>>,
    created_at: Instant,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    closed: Arc<AtomicBool>,
    active_streams: Arc<RwLock<usize>>,
}

impl std::fmt::Debug for H2ServerConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("H2ServerConnection")
            .field("connection_id", &self.connection_id)
            .field("remote_addr", &self.remote_addr)
            .finish()
    }
}

impl H2ServerConnection {
    pub async fn new<T>(io: T, remote_addr: SocketAddr) -> TransportResult<Self>
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let connection_id = format!("h2-server-{}", uuid::Uuid::new_v4());

        let mut h2_conn = h2::server::handshake(io)
            .await
            .map_err(|e| TransportError::ConnectionError(format!("H2 handshake failed: {}", e)))?;

        let (accept_tx, accept_rx) = mpsc::channel(64);
        let closed = Arc::new(AtomicBool::new(false));
        let bytes_sent = Arc::new(AtomicU64::new(0));
        let bytes_received = Arc::new(AtomicU64::new(0));

        // Spawn connection driver
        let closed_clone = closed.clone();
        let conn_id = connection_id.clone();
        tokio::spawn(async move {
            loop {
                match h2_conn.accept().await {
                    Some(Ok((request, send_response))) => {
                        debug!("[{}] Accepted H2 stream", conn_id);
                        let recv_stream = request.into_body();
                        if accept_tx.send((send_response, recv_stream)).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        error!("[{}] H2 accept error: {}", conn_id, e);
                        break;
                    }
                    None => {
                        debug!("[{}] H2 connection closed", conn_id);
                        break;
                    }
                }
            }
            closed_clone.store(true, Ordering::SeqCst);
        });

        Ok(Self {
            connection_id,
            remote_addr,
            accept_rx: Mutex::new(accept_rx),
            created_at: Instant::now(),
            bytes_sent,
            bytes_received,
            closed,
            active_streams: Arc::new(RwLock::new(0)),
        })
    }
}

#[async_trait]
impl TransportConnection for H2ServerConnection {
    type Stream = H2Stream;

    async fn open_stream(&self) -> TransportResult<Self::Stream> {
        // Server cannot initiate streams in HTTP/2
        Err(TransportError::ProtocolError(
            "Server cannot initiate HTTP/2 streams".to_string(),
        ))
    }

    async fn accept_stream(&self) -> TransportResult<Option<Self::Stream>> {
        if self.closed.load(Ordering::SeqCst) {
            return Ok(None);
        }

        let mut accept_rx = self.accept_rx.lock().await;

        match accept_rx.recv().await {
            Some((mut send_response, recv_stream)) => {
                // Send response headers to establish bidirectional stream
                let response = http::Response::builder().status(200).body(()).unwrap();

                let send_stream = send_response.send_response(response, false).map_err(|e| {
                    TransportError::ConnectionError(format!("Failed to send response: {}", e))
                })?;

                let stream_id = send_stream.stream_id().as_u32();

                *self.active_streams.write().await += 1;

                debug!("[{}] Accepted stream {}", self.connection_id, stream_id);

                Ok(Some(H2Stream::new(send_stream, recv_stream, stream_id)))
            }
            None => Ok(None),
        }
    }

    async fn close(&self, _error_code: u32, reason: &str) {
        debug!("[{}] Closing connection: {}", self.connection_id, reason);
        self.closed.store(true, Ordering::SeqCst);
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn remote_address(&self) -> SocketAddr {
        self.remote_addr
    }

    fn stats(&self) -> ConnectionStats {
        let streams = self.active_streams.try_read().map(|s| *s).unwrap_or(0);

        ConnectionStats {
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            active_streams: streams,
            rtt_ms: None,
            uptime_secs: self.created_at.elapsed().as_secs(),
        }
    }

    fn connection_id(&self) -> String {
        self.connection_id.clone()
    }
}

/// Client-side HTTP/2 connection
pub struct H2ClientConnection {
    connection_id: String,
    remote_addr: SocketAddr,
    /// Send request handle for opening streams
    send_request: Mutex<SendRequest<Bytes>>,
    /// Channel for accepting pushed streams (server push)
    accept_rx: Mutex<mpsc::Receiver<H2Stream>>,
    created_at: Instant,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    closed: Arc<AtomicBool>,
    active_streams: Arc<RwLock<usize>>,
}

impl std::fmt::Debug for H2ClientConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("H2ClientConnection")
            .field("connection_id", &self.connection_id)
            .field("remote_addr", &self.remote_addr)
            .finish()
    }
}

impl H2ClientConnection {
    pub async fn new<T>(io: T, remote_addr: SocketAddr) -> TransportResult<Self>
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let connection_id = format!("h2-client-{}", uuid::Uuid::new_v4());

        let (send_request, h2_conn) = h2::client::handshake(io)
            .await
            .map_err(|e| TransportError::ConnectionError(format!("H2 handshake failed: {}", e)))?;

        let (_accept_tx, accept_rx) = mpsc::channel::<H2Stream>(64);
        let closed = Arc::new(AtomicBool::new(false));
        let bytes_sent = Arc::new(AtomicU64::new(0));
        let bytes_received = Arc::new(AtomicU64::new(0));

        // Spawn connection driver
        let closed_clone = closed.clone();
        let conn_id = connection_id.clone();
        tokio::spawn(async move {
            if let Err(e) = h2_conn.await {
                if !e.is_go_away() && !e.is_io() {
                    error!("[{}] H2 connection error: {}", conn_id, e);
                }
            }
            debug!("[{}] H2 connection closed", conn_id);
            closed_clone.store(true, Ordering::SeqCst);
        });

        Ok(Self {
            connection_id,
            remote_addr,
            send_request: Mutex::new(send_request),
            accept_rx: Mutex::new(accept_rx),
            created_at: Instant::now(),
            bytes_sent,
            bytes_received,
            closed,
            active_streams: Arc::new(RwLock::new(0)),
        })
    }
}

#[async_trait]
impl TransportConnection for H2ClientConnection {
    type Stream = H2Stream;

    async fn open_stream(&self) -> TransportResult<Self::Stream> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(TransportError::ConnectionError(
                "Connection closed".to_string(),
            ));
        }

        let send_request_guard = self.send_request.lock().await;

        // Clone to get a ready handle (SendRequest is Clone)
        let send_request = send_request_guard.clone();
        drop(send_request_guard);

        // Wait for the connection to be ready
        let mut ready_request = send_request.ready().await.map_err(|e| {
            TransportError::ConnectionError(format!("H2 connection not ready: {}", e))
        })?;

        // Create a POST request to open a stream
        let request = http::Request::builder()
            .method("POST")
            .uri("https://localup/stream")
            .body(())
            .unwrap();

        let (response, send_stream) = ready_request.send_request(request, false).map_err(|e| {
            TransportError::ConnectionError(format!("Failed to open stream: {}", e))
        })?;

        let stream_id = send_stream.stream_id().as_u32();

        // Wait for response
        let response = response.await.map_err(|e| {
            TransportError::ConnectionError(format!("Failed to get response: {}", e))
        })?;

        if !response.status().is_success() {
            return Err(TransportError::ConnectionError(format!(
                "Server returned {}",
                response.status()
            )));
        }

        let recv_stream = response.into_body();

        *self.active_streams.write().await += 1;

        debug!("[{}] Opened stream {}", self.connection_id, stream_id);

        Ok(H2Stream::new(send_stream, recv_stream, stream_id))
    }

    async fn accept_stream(&self) -> TransportResult<Option<Self::Stream>> {
        // Client typically doesn't accept streams (server push is rare)
        let mut accept_rx = self.accept_rx.lock().await;
        match accept_rx.recv().await {
            Some(stream) => Ok(Some(stream)),
            None => Ok(None),
        }
    }

    async fn close(&self, error_code: u32, reason: &str) {
        debug!(
            "[{}] Closing connection: {} (code: {})",
            self.connection_id, reason, error_code
        );
        self.closed.store(true, Ordering::SeqCst);
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn remote_address(&self) -> SocketAddr {
        self.remote_addr
    }

    fn stats(&self) -> ConnectionStats {
        let streams = self.active_streams.try_read().map(|s| *s).unwrap_or(0);

        ConnectionStats {
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            active_streams: streams,
            rtt_ms: None,
            uptime_secs: self.created_at.elapsed().as_secs(),
        }
    }

    fn connection_id(&self) -> String {
        self.connection_id.clone()
    }
}

/// Unified H2 connection type for the transport layer
pub enum H2Connection {
    Server(H2ServerConnection),
    Client(H2ClientConnection),
}

impl std::fmt::Debug for H2Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            H2Connection::Server(c) => c.fmt(f),
            H2Connection::Client(c) => c.fmt(f),
        }
    }
}

#[async_trait]
impl TransportConnection for H2Connection {
    type Stream = H2Stream;

    async fn open_stream(&self) -> TransportResult<Self::Stream> {
        match self {
            H2Connection::Server(c) => c.open_stream().await,
            H2Connection::Client(c) => c.open_stream().await,
        }
    }

    async fn accept_stream(&self) -> TransportResult<Option<Self::Stream>> {
        match self {
            H2Connection::Server(c) => c.accept_stream().await,
            H2Connection::Client(c) => c.accept_stream().await,
        }
    }

    async fn close(&self, error_code: u32, reason: &str) {
        match self {
            H2Connection::Server(c) => c.close(error_code, reason).await,
            H2Connection::Client(c) => c.close(error_code, reason).await,
        }
    }

    fn is_closed(&self) -> bool {
        match self {
            H2Connection::Server(c) => c.is_closed(),
            H2Connection::Client(c) => c.is_closed(),
        }
    }

    fn remote_address(&self) -> SocketAddr {
        match self {
            H2Connection::Server(c) => c.remote_address(),
            H2Connection::Client(c) => c.remote_address(),
        }
    }

    fn stats(&self) -> ConnectionStats {
        match self {
            H2Connection::Server(c) => c.stats(),
            H2Connection::Client(c) => c.stats(),
        }
    }

    fn connection_id(&self) -> String {
        match self {
            H2Connection::Server(c) => c.connection_id(),
            H2Connection::Client(c) => c.connection_id(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_connection_types() {
        // Just verify module compiles
        assert!(true);
    }
}
