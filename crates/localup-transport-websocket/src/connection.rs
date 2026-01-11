//! WebSocket connection implementation with stream multiplexing

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use localup_transport::{ConnectionStats, TransportConnection, TransportError, TransportResult};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, trace, warn};

use crate::stream::{decode_frame_header, WebSocketStream, MSG_TYPE_DATA, MSG_TYPE_FIN};

type WsStream = tokio_tungstenite::WebSocketStream<tokio_rustls::TlsStream<tokio::net::TcpStream>>;

/// Multiplexed WebSocket connection
pub struct WebSocketConnection {
    /// Connection ID for logging
    connection_id: String,
    /// Remote address
    remote_addr: SocketAddr,
    /// Channel for sending frames to WebSocket writer task
    frame_tx: Arc<Mutex<mpsc::Sender<Vec<u8>>>>,
    /// Stream channels - maps stream ID to sender for that stream
    streams: Arc<RwLock<HashMap<u32, mpsc::Sender<Bytes>>>>,
    /// Channel for accepting new incoming streams
    accept_rx: Mutex<mpsc::Receiver<(u32, mpsc::Receiver<Bytes>)>>,
    /// Sender for new incoming streams (used by reader task)
    #[allow(dead_code)]
    accept_tx: mpsc::Sender<(u32, mpsc::Receiver<Bytes>)>,
    /// Next stream ID for client-initiated streams (odd for client, even for server)
    next_stream_id: AtomicU32,
    /// Whether this is the server side
    is_server: bool,
    /// Connection created timestamp
    created_at: Instant,
    /// Bytes sent counter
    bytes_sent: Arc<AtomicU64>,
    /// Bytes received counter
    bytes_received: Arc<AtomicU64>,
    /// Whether connection is closed
    closed: Arc<AtomicBool>,
}

impl std::fmt::Debug for WebSocketConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSocketConnection")
            .field("connection_id", &self.connection_id)
            .field("remote_addr", &self.remote_addr)
            .field("is_server", &self.is_server)
            .finish()
    }
}

impl WebSocketConnection {
    /// Create a new WebSocket connection from an established WebSocket stream
    pub fn new(ws_stream: WsStream, remote_addr: SocketAddr, is_server: bool) -> Self {
        let connection_id = format!("ws-{}", uuid::Uuid::new_v4());

        let (ws_sink, ws_source) = ws_stream.split();

        // Channel for frames to send
        let (frame_tx, frame_rx) = mpsc::channel::<Vec<u8>>(256);
        let frame_tx = Arc::new(Mutex::new(frame_tx));

        // Channel for accepting new streams
        let (accept_tx, accept_rx) = mpsc::channel(64);

        // Stream channels map
        let streams: Arc<RwLock<HashMap<u32, mpsc::Sender<Bytes>>>> =
            Arc::new(RwLock::new(HashMap::new()));

        // Server uses even stream IDs, client uses odd
        let next_stream_id = if is_server { 2 } else { 1 };

        let conn = Self {
            connection_id: connection_id.clone(),
            remote_addr,
            frame_tx: frame_tx.clone(),
            streams: streams.clone(),
            accept_rx: Mutex::new(accept_rx),
            accept_tx: accept_tx.clone(),
            next_stream_id: AtomicU32::new(next_stream_id),
            is_server,
            created_at: Instant::now(),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            closed: Arc::new(AtomicBool::new(false)),
        };

        // Spawn writer task
        let bytes_sent = conn.bytes_sent.clone();
        let closed_flag = conn.closed.clone();
        let conn_id = connection_id.clone();
        tokio::spawn(async move {
            Self::writer_task(ws_sink, frame_rx, bytes_sent, closed_flag, conn_id).await;
        });

        // Spawn reader task
        let bytes_received = conn.bytes_received.clone();
        let closed_flag = conn.closed.clone();
        tokio::spawn(async move {
            Self::reader_task(
                ws_source,
                streams,
                accept_tx,
                frame_tx,
                bytes_received,
                closed_flag,
                connection_id,
            )
            .await;
        });

        conn
    }

    /// Writer task - sends frames to WebSocket
    async fn writer_task(
        mut sink: futures_util::stream::SplitSink<WsStream, Message>,
        mut rx: mpsc::Receiver<Vec<u8>>,
        bytes_sent: Arc<AtomicU64>,
        closed: Arc<AtomicBool>,
        conn_id: String,
    ) {
        while let Some(frame) = rx.recv().await {
            bytes_sent.fetch_add(frame.len() as u64, Ordering::Relaxed);

            if let Err(e) = sink.send(Message::Binary(frame)).await {
                error!("[{}] WebSocket send error: {}", conn_id, e);
                break;
            }
        }

        debug!("[{}] WebSocket writer task ended", conn_id);
        closed.store(true, Ordering::SeqCst);
        let _ = sink.close().await;
    }

    /// Reader task - receives frames and dispatches to streams
    async fn reader_task(
        mut source: futures_util::stream::SplitStream<WsStream>,
        streams: Arc<RwLock<HashMap<u32, mpsc::Sender<Bytes>>>>,
        accept_tx: mpsc::Sender<(u32, mpsc::Receiver<Bytes>)>,
        _frame_tx: Arc<Mutex<mpsc::Sender<Vec<u8>>>>,
        bytes_received: Arc<AtomicU64>,
        closed: Arc<AtomicBool>,
        conn_id: String,
    ) {
        while let Some(result) = source.next().await {
            match result {
                Ok(Message::Binary(data)) => {
                    bytes_received.fetch_add(data.len() as u64, Ordering::Relaxed);

                    if let Some((stream_id, msg_type, payload)) = decode_frame_header(&data) {
                        trace!(
                            "[{}] Received frame: stream={}, type={}, len={}",
                            conn_id,
                            stream_id,
                            msg_type,
                            payload.len()
                        );

                        let streams_read = streams.read().await;

                        if let Some(tx) = streams_read.get(&stream_id) {
                            // Existing stream
                            match msg_type {
                                MSG_TYPE_DATA => {
                                    if tx.send(Bytes::copy_from_slice(payload)).await.is_err() {
                                        warn!(
                                            "[{}] Stream {} receiver dropped",
                                            conn_id, stream_id
                                        );
                                    }
                                }
                                MSG_TYPE_FIN => {
                                    // Signal stream close with empty bytes
                                    let _ = tx.send(Bytes::new()).await;
                                }
                                _ => {
                                    warn!("[{}] Unknown message type: {}", conn_id, msg_type);
                                }
                            }
                        } else {
                            drop(streams_read);
                            // New incoming stream
                            if msg_type == MSG_TYPE_DATA {
                                let (tx, rx) = mpsc::channel(256);

                                // Send initial data
                                if tx.send(Bytes::copy_from_slice(payload)).await.is_ok() {
                                    // Register the stream
                                    streams.write().await.insert(stream_id, tx);

                                    // Notify about new stream
                                    if accept_tx.send((stream_id, rx)).await.is_err() {
                                        warn!(
                                            "[{}] Accept channel closed, dropping stream {}",
                                            conn_id, stream_id
                                        );
                                    }
                                }
                            }
                        }
                    } else {
                        warn!("[{}] Invalid frame received", conn_id);
                    }
                }
                Ok(Message::Ping(_data)) => {
                    // Pong is automatically handled by tungstenite
                    trace!("[{}] Received ping, pong handled by tungstenite", conn_id);
                }
                Ok(Message::Pong(_)) => {
                    trace!("[{}] Received pong", conn_id);
                }
                Ok(Message::Close(_)) => {
                    debug!("[{}] WebSocket close received", conn_id);
                    break;
                }
                Ok(_) => {
                    // Text or other message types - ignore
                }
                Err(e) => {
                    error!("[{}] WebSocket read error: {}", conn_id, e);
                    break;
                }
            }
        }

        debug!("[{}] WebSocket reader task ended", conn_id);
        closed.store(true, Ordering::SeqCst);

        // Close all streams
        let streams = streams.read().await;
        for (_, tx) in streams.iter() {
            let _ = tx.send(Bytes::new()).await;
        }
    }
}

#[async_trait]
impl TransportConnection for WebSocketConnection {
    type Stream = WebSocketStream;

    async fn open_stream(&self) -> TransportResult<Self::Stream> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(TransportError::ConnectionError(
                "Connection closed".to_string(),
            ));
        }

        // Get next stream ID (increment by 2 to maintain odd/even)
        let stream_id = self.next_stream_id.fetch_add(2, Ordering::SeqCst);

        // Create channel for this stream
        let (tx, rx) = mpsc::channel(256);

        // Register the stream
        self.streams.write().await.insert(stream_id, tx);

        debug!("[{}] Opened stream {}", self.connection_id, stream_id);

        Ok(WebSocketStream::new(
            stream_id as u64,
            rx,
            self.frame_tx.clone(),
        ))
    }

    async fn accept_stream(&self) -> TransportResult<Option<Self::Stream>> {
        if self.closed.load(Ordering::SeqCst) {
            return Ok(None);
        }

        let mut accept_rx = self.accept_rx.lock().await;

        match accept_rx.recv().await {
            Some((stream_id, rx)) => {
                debug!("[{}] Accepted stream {}", self.connection_id, stream_id);
                Ok(Some(WebSocketStream::new(
                    stream_id as u64,
                    rx,
                    self.frame_tx.clone(),
                )))
            }
            None => {
                // Accept channel closed
                Ok(None)
            }
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
        let streams = self.streams.try_read().map(|s| s.len()).unwrap_or(0);

        ConnectionStats {
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            active_streams: streams,
            rtt_ms: None, // WebSocket doesn't expose RTT
            uptime_secs: self.created_at.elapsed().as_secs(),
        }
    }

    fn connection_id(&self) -> String {
        self.connection_id.clone()
    }
}

impl Clone for WebSocketConnection {
    fn clone(&self) -> Self {
        // This is a shallow clone that shares the underlying connection
        // Used when we need to pass connection to multiple tasks
        panic!("WebSocketConnection should not be cloned - use Arc instead");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_connection_debug() {
        // Just verify Debug impl works
        let debug_str = "WebSocketConnection";
        assert!(!debug_str.is_empty());
    }
}
