//! WebSocket stream implementation with multiplexing
//!
//! WebSocket doesn't have native stream multiplexing, so we implement it
//! using a message framing protocol:
//!
//! Frame format:
//! - 4 bytes: stream ID (big-endian u32)
//! - 1 byte: message type (0=data, 1=fin)
//! - Rest: payload

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use localup_proto::{TunnelCodec, TunnelMessage};
use localup_transport::{TransportError, TransportResult, TransportStream};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::trace;

/// Message type constants for stream multiplexing
pub(crate) const MSG_TYPE_DATA: u8 = 0;
pub(crate) const MSG_TYPE_FIN: u8 = 1;

/// Encode a multiplexed frame
pub(crate) fn encode_frame(stream_id: u32, msg_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(5 + payload.len());
    frame.extend_from_slice(&stream_id.to_be_bytes());
    frame.push(msg_type);
    frame.extend_from_slice(payload);
    frame
}

/// Decode a multiplexed frame header
pub(crate) fn decode_frame_header(data: &[u8]) -> Option<(u32, u8, &[u8])> {
    if data.len() < 5 {
        return None;
    }
    let stream_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let msg_type = data[4];
    let payload = &data[5..];
    Some((stream_id, msg_type, payload))
}

/// A virtual stream over a multiplexed WebSocket connection
#[derive(Debug)]
pub struct WebSocketStream {
    stream_id: u64,
    /// Channel to receive data for this stream
    rx: mpsc::Receiver<Bytes>,
    /// Shared sender to the WebSocket (for sending frames)
    tx: Arc<Mutex<mpsc::Sender<Vec<u8>>>>,
    /// Buffer for incomplete message data
    recv_buffer: BytesMut,
    /// Whether this stream is closed
    closed: bool,
    /// Buffered received data chunks
    data_queue: VecDeque<Bytes>,
}

impl WebSocketStream {
    pub(crate) fn new(
        stream_id: u64,
        rx: mpsc::Receiver<Bytes>,
        tx: Arc<Mutex<mpsc::Sender<Vec<u8>>>>,
    ) -> Self {
        Self {
            stream_id,
            rx,
            tx,
            recv_buffer: BytesMut::with_capacity(8192),
            closed: false,
            data_queue: VecDeque::new(),
        }
    }
}

#[async_trait]
impl TransportStream for WebSocketStream {
    async fn send_message(&mut self, message: &TunnelMessage) -> TransportResult<()> {
        if self.closed {
            return Err(TransportError::StreamClosed);
        }

        let encoded = TunnelCodec::encode(message)
            .map_err(|e| TransportError::ProtocolError(e.to_string()))?;

        self.send_bytes(&encoded).await?;

        trace!("Sent message on stream {}: {:?}", self.stream_id, message);
        Ok(())
    }

    async fn recv_message(&mut self) -> TransportResult<Option<TunnelMessage>> {
        if self.closed && self.recv_buffer.is_empty() && self.data_queue.is_empty() {
            return Ok(None);
        }

        loop {
            // Try to decode a message from the buffer
            match TunnelCodec::decode(&mut self.recv_buffer)
                .map_err(|e| TransportError::ProtocolError(e.to_string()))?
            {
                Some(msg) => {
                    trace!("Received message on stream {}: {:?}", self.stream_id, msg);
                    return Ok(Some(msg));
                }
                None => {
                    // Try to get more data from queue or channel
                    if let Some(data) = self.data_queue.pop_front() {
                        self.recv_buffer.extend_from_slice(&data);
                        continue;
                    }

                    // Wait for more data from channel
                    match self.rx.recv().await {
                        Some(data) => {
                            if data.is_empty() {
                                // Empty data signals stream close
                                self.closed = true;
                                if self.recv_buffer.is_empty() {
                                    return Ok(None);
                                } else {
                                    return Err(TransportError::ProtocolError(
                                        "Incomplete message in buffer".to_string(),
                                    ));
                                }
                            }
                            self.recv_buffer.extend_from_slice(&data);
                        }
                        None => {
                            self.closed = true;
                            if self.recv_buffer.is_empty() {
                                return Ok(None);
                            } else {
                                return Err(TransportError::ProtocolError(
                                    "Channel closed with incomplete message".to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    async fn send_bytes(&mut self, data: &[u8]) -> TransportResult<()> {
        if self.closed {
            return Err(TransportError::StreamClosed);
        }

        let frame = encode_frame(self.stream_id as u32, MSG_TYPE_DATA, data);

        let tx = self.tx.lock().await;
        tx.send(frame)
            .await
            .map_err(|_| TransportError::ConnectionError("WebSocket send failed".to_string()))?;

        Ok(())
    }

    async fn recv_bytes(&mut self, max_size: usize) -> TransportResult<Bytes> {
        if self.closed && self.data_queue.is_empty() {
            return Ok(Bytes::new());
        }

        // Check queue first
        if let Some(data) = self.data_queue.pop_front() {
            if data.len() <= max_size {
                return Ok(data);
            }
            // Split if too large
            let (first, rest) = data.split_at(max_size);
            self.data_queue.push_front(Bytes::copy_from_slice(rest));
            return Ok(Bytes::copy_from_slice(first));
        }

        // Wait for data from channel
        match self.rx.recv().await {
            Some(data) => {
                if data.is_empty() {
                    self.closed = true;
                    return Ok(Bytes::new());
                }
                if data.len() <= max_size {
                    Ok(data)
                } else {
                    let (first, rest) = data.split_at(max_size);
                    self.data_queue.push_front(Bytes::copy_from_slice(rest));
                    Ok(Bytes::copy_from_slice(first))
                }
            }
            None => {
                self.closed = true;
                Ok(Bytes::new())
            }
        }
    }

    async fn finish(&mut self) -> TransportResult<()> {
        if self.closed {
            return Ok(());
        }

        // Send FIN frame
        let frame = encode_frame(self.stream_id as u32, MSG_TYPE_FIN, &[]);

        let tx = self.tx.lock().await;
        let _ = tx.send(frame).await; // Ignore error if connection is already closed

        self.closed = true;
        Ok(())
    }

    fn stream_id(&self) -> u64 {
        self.stream_id
    }

    fn is_closed(&self) -> bool {
        self.closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_encoding() {
        let frame = encode_frame(42, MSG_TYPE_DATA, b"hello");
        assert_eq!(frame.len(), 5 + 5); // 4 bytes stream_id + 1 byte type + 5 bytes payload

        let (stream_id, msg_type, payload) = decode_frame_header(&frame).unwrap();
        assert_eq!(stream_id, 42);
        assert_eq!(msg_type, MSG_TYPE_DATA);
        assert_eq!(payload, b"hello");
    }

    #[test]
    fn test_fin_frame() {
        let frame = encode_frame(1, MSG_TYPE_FIN, &[]);
        let (stream_id, msg_type, payload) = decode_frame_header(&frame).unwrap();
        assert_eq!(stream_id, 1);
        assert_eq!(msg_type, MSG_TYPE_FIN);
        assert!(payload.is_empty());
    }
}
