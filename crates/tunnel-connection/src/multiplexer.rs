//! Multiplexed connection implementation

use crate::transport::{Transport, TransportError};
use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error, trace, warn};
use tunnel_proto::{Frame, FrameType, Multiplexer, StreamId, TunnelCodec, TunnelMessage};

/// Multiplexed connection errors
#[derive(Debug, Error)]
pub enum MultiplexError {
    #[error("Transport error: {0}")]
    TransportError(#[from] TransportError),

    #[error("Stream not found: {0}")]
    StreamNotFound(StreamId),

    #[error("Stream closed: {0}")]
    StreamClosed(StreamId),

    #[error("Multiplexer error: {0}")]
    MuxError(#[from] tunnel_proto::mux::MuxError),

    #[error("Codec error: {0}")]
    CodecError(#[from] tunnel_proto::CodecError),

    #[error("Channel send error")]
    ChannelSendError,
}

/// Stream data receiver
pub type StreamReceiver = mpsc::UnboundedReceiver<Bytes>;

/// Stream data sender
pub type StreamSender = mpsc::UnboundedSender<Bytes>;

/// Multiplexed stream handle
pub struct MultiplexedStream {
    stream_id: StreamId,
    tx: StreamSender,
    rx: StreamReceiver,
    connection: Arc<MultiplexedConnection>,
}

impl MultiplexedStream {
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    pub async fn send(&self, data: Bytes) -> Result<(), MultiplexError> {
        self.connection.send_data(self.stream_id, data).await
    }

    pub async fn recv(&mut self) -> Option<Bytes> {
        self.rx.recv().await
    }

    pub async fn close(self) -> Result<(), MultiplexError> {
        self.connection.close_stream(self.stream_id).await
    }
}

/// Multiplexed connection
pub struct MultiplexedConnection {
    transport: Arc<Mutex<Box<dyn Transport>>>,
    mux: Arc<Multiplexer>,
    streams: Arc<RwLock<HashMap<StreamId, StreamSender>>>,
    control_tx: mpsc::UnboundedSender<TunnelMessage>,
    control_rx: Arc<Mutex<mpsc::UnboundedReceiver<TunnelMessage>>>,
}

impl MultiplexedConnection {
    /// Create a new multiplexed connection
    pub fn new(transport: Box<dyn Transport>) -> Self {
        let (control_tx, control_rx) = mpsc::unbounded_channel();

        Self {
            transport: Arc::new(Mutex::new(transport)),
            mux: Arc::new(Multiplexer::new()),
            streams: Arc::new(RwLock::new(HashMap::new())),
            control_tx,
            control_rx: Arc::new(Mutex::new(control_rx)),
        }
    }

    /// Open a new stream
    pub async fn open_stream(&self) -> Result<MultiplexedStream, MultiplexError> {
        let stream_id = self.mux.allocate_stream()?;
        let (tx, rx) = mpsc::unbounded_channel();

        {
            let mut streams = self.streams.write().await;
            streams.insert(stream_id, tx.clone());
        }

        debug!("Opened stream: {}", stream_id);

        Ok(MultiplexedStream {
            stream_id,
            tx,
            rx,
            connection: Arc::new(Self {
                transport: self.transport.clone(),
                mux: self.mux.clone(),
                streams: self.streams.clone(),
                control_tx: self.control_tx.clone(),
                control_rx: self.control_rx.clone(),
            }),
        })
    }

    /// Register an incoming stream
    pub async fn register_stream(&self, stream_id: StreamId) -> Result<StreamReceiver, MultiplexError> {
        self.mux.register_stream(stream_id)?;
        let (tx, rx) = mpsc::unbounded_channel();

        {
            let mut streams = self.streams.write().await;
            streams.insert(stream_id, tx);
        }

        debug!("Registered incoming stream: {}", stream_id);
        Ok(rx)
    }

    /// Send data on a stream
    pub async fn send_data(&self, stream_id: StreamId, data: Bytes) -> Result<(), MultiplexError> {
        trace!("Sending {} bytes on stream {}", data.len(), stream_id);

        let frame = Frame::data(stream_id, data);
        let encoded = frame.encode()?;

        let mut transport = self.transport.lock().await;
        transport.send(encoded).await?;

        Ok(())
    }

    /// Send control message
    pub async fn send_control(&self, msg: TunnelMessage) -> Result<(), MultiplexError> {
        trace!("Sending control message: {:?}", msg);

        let encoded = TunnelCodec::encode(&msg)?;
        let frame = Frame::control(encoded);
        let frame_data = frame.encode()?;

        let mut transport = self.transport.lock().await;
        transport.send(frame_data).await?;

        Ok(())
    }

    /// Receive control message
    pub async fn recv_control(&self) -> Option<TunnelMessage> {
        let mut rx = self.control_rx.lock().await;
        rx.recv().await
    }

    /// Close a stream
    pub async fn close_stream(&self, stream_id: StreamId) -> Result<(), MultiplexError> {
        debug!("Closing stream: {}", stream_id);

        // Send close frame
        let frame = Frame::close(stream_id);
        let encoded = frame.encode()?;

        {
            let mut transport = self.transport.lock().await;
            transport.send(encoded).await?;
        }

        // Remove from streams
        {
            let mut streams = self.streams.write().await;
            streams.remove(&stream_id);
        }

        // Mark as closed in multiplexer
        self.mux.close_stream(stream_id)?;

        Ok(())
    }

    /// Run the receive loop (processes incoming frames)
    pub async fn run_receive_loop(self: Arc<Self>) -> Result<(), MultiplexError> {
        debug!("Starting receive loop");

        let mut buf = BytesMut::new();

        loop {
            // Receive data from transport
            let data = {
                let mut transport = self.transport.lock().await;
                match transport.recv().await? {
                    Some(data) => data,
                    None => {
                        debug!("Transport closed");
                        break;
                    }
                }
            };

            buf.extend_from_slice(&data);

            // Try to decode frames
            while buf.len() >= Frame::HEADER_SIZE {
                // Peek at the frame header to get the full size
                let frame_result = Frame::decode(buf.clone().freeze());

                match frame_result {
                    Ok(frame) => {
                        // Successfully decoded a frame, remove it from buffer
                        let frame_size = Frame::HEADER_SIZE + frame.payload.len();
                        buf.split_to(frame_size);

                        // Process the frame
                        if let Err(e) = self.process_frame(frame).await {
                            error!("Error processing frame: {}", e);
                        }
                    }
                    Err(tunnel_proto::mux::MuxError::IncompleteFrame) => {
                        // Need more data
                        break;
                    }
                    Err(e) => {
                        error!("Error decoding frame: {}", e);
                        return Err(MultiplexError::MuxError(e));
                    }
                }
            }
        }

        debug!("Receive loop ended");
        Ok(())
    }

    /// Process a received frame
    async fn process_frame(&self, frame: Frame) -> Result<(), MultiplexError> {
        trace!(
            "Processing frame: stream_id={}, type={:?}, size={}",
            frame.stream_id,
            frame.frame_type,
            frame.payload.len()
        );

        match frame.frame_type {
            FrameType::Control => {
                // Decode control message
                let mut payload_buf = BytesMut::from(frame.payload.as_ref());
                if let Some(msg) = TunnelCodec::decode(&mut payload_buf)? {
                    self.control_tx
                        .send(msg)
                        .map_err(|_| MultiplexError::ChannelSendError)?;
                }
            }
            FrameType::Data => {
                // Route data to stream
                let streams = self.streams.read().await;
                if let Some(tx) = streams.get(&frame.stream_id) {
                    if tx.send(frame.payload).is_err() {
                        warn!("Failed to send data to stream {}", frame.stream_id);
                    }
                } else {
                    warn!("Received data for unknown stream: {}", frame.stream_id);
                }
            }
            FrameType::Close => {
                // Close stream
                let mut streams = self.streams.write().await;
                streams.remove(&frame.stream_id);
                self.mux.close_stream(frame.stream_id)?;
                debug!("Stream {} closed by remote", frame.stream_id);
            }
            FrameType::WindowUpdate => {
                // TODO: Implement flow control
                trace!("Received window update for stream {}", frame.stream_id);
            }
        }

        Ok(())
    }

    /// Get number of active streams
    pub async fn active_streams(&self) -> usize {
        self.mux.active_streams()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Transport;
    use async_trait::async_trait;
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    // Mock transport for testing
    struct MockTransport {
        send_buffer: Arc<TokioMutex<Vec<Bytes>>>,
        recv_buffer: Arc<TokioMutex<Vec<Bytes>>>,
        connected: bool,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                send_buffer: Arc::new(TokioMutex::new(Vec::new())),
                recv_buffer: Arc::new(TokioMutex::new(Vec::new())),
                connected: true,
            }
        }
    }

    #[async_trait]
    impl Transport for MockTransport {
        async fn send(&mut self, data: Bytes) -> Result<(), TransportError> {
            let mut buffer = self.send_buffer.lock().await;
            buffer.push(data);
            Ok(())
        }

        async fn recv(&mut self) -> Result<Option<Bytes>, TransportError> {
            let mut buffer = self.recv_buffer.lock().await;
            if buffer.is_empty() {
                // Simulate waiting
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                return Ok(None);
            }
            Ok(Some(buffer.remove(0)))
        }

        async fn close(&mut self) -> Result<(), TransportError> {
            self.connected = false;
            Ok(())
        }

        fn is_connected(&self) -> bool {
            self.connected
        }
    }

    #[tokio::test]
    async fn test_multiplexed_connection_open_stream() {
        let transport = Box::new(MockTransport::new());
        let conn = MultiplexedConnection::new(transport);

        let stream = conn.open_stream().await.unwrap();
        assert!(stream.stream_id() > 0);
    }

    #[tokio::test]
    async fn test_multiplexed_connection_multiple_streams() {
        let transport = Box::new(MockTransport::new());
        let conn = MultiplexedConnection::new(transport);

        let stream1 = conn.open_stream().await.unwrap();
        let stream2 = conn.open_stream().await.unwrap();

        assert_ne!(stream1.stream_id(), stream2.stream_id());
        assert_eq!(conn.active_streams().await, 2);
    }
}
