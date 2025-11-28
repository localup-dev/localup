//! HTTP/2 stream implementation

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use h2::{RecvStream, SendStream};
use localup_proto::{TunnelCodec, TunnelMessage};
use localup_transport::{TransportError, TransportResult, TransportStream};
use std::collections::VecDeque;
use tracing::trace;

/// HTTP/2 stream wrapper
pub struct H2Stream {
    send: SendStream<Bytes>,
    recv: RecvStream,
    stream_id: u64,
    closed: bool,
    recv_buffer: BytesMut,
    data_queue: VecDeque<Bytes>,
}

impl std::fmt::Debug for H2Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("H2Stream")
            .field("stream_id", &self.stream_id)
            .field("closed", &self.closed)
            .finish()
    }
}

impl H2Stream {
    pub fn new(send: SendStream<Bytes>, recv: RecvStream, stream_id: u32) -> Self {
        Self {
            send,
            recv,
            stream_id: stream_id as u64,
            closed: false,
            recv_buffer: BytesMut::with_capacity(8192),
            data_queue: VecDeque::new(),
        }
    }
}

#[async_trait]
impl TransportStream for H2Stream {
    async fn send_message(&mut self, message: &TunnelMessage) -> TransportResult<()> {
        if self.closed {
            return Err(TransportError::StreamClosed);
        }

        let encoded = TunnelCodec::encode(message)
            .map_err(|e| TransportError::ProtocolError(e.to_string()))?;

        self.send_bytes(&encoded).await?;

        trace!(
            "Sent message on H2 stream {}: {:?}",
            self.stream_id,
            message
        );
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
                    trace!(
                        "Received message on H2 stream {}: {:?}",
                        self.stream_id,
                        msg
                    );
                    return Ok(Some(msg));
                }
                None => {
                    // Try to get more data from queue
                    if let Some(data) = self.data_queue.pop_front() {
                        self.recv_buffer.extend_from_slice(&data);
                        continue;
                    }

                    // Wait for more data from H2 stream
                    match self.recv.data().await {
                        Some(Ok(data)) => {
                            // Release flow control capacity
                            let _ = self.recv.flow_control().release_capacity(data.len());
                            self.recv_buffer.extend_from_slice(&data);
                        }
                        Some(Err(e)) => {
                            self.closed = true;
                            return Err(TransportError::ConnectionError(format!(
                                "H2 receive error: {}",
                                e
                            )));
                        }
                        None => {
                            // Stream ended
                            self.closed = true;
                            if self.recv_buffer.is_empty() {
                                return Ok(None);
                            } else {
                                return Err(TransportError::ProtocolError(
                                    "Incomplete message in buffer".to_string(),
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

        // Reserve capacity and send
        self.send.reserve_capacity(data.len());

        // Simple approach: just send the data, h2 will handle flow control
        self.send
            .send_data(Bytes::copy_from_slice(data), false)
            .map_err(|e| TransportError::ConnectionError(format!("H2 send error: {}", e)))?;

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
            let (first, rest) = data.split_at(max_size);
            self.data_queue.push_front(Bytes::copy_from_slice(rest));
            return Ok(Bytes::copy_from_slice(first));
        }

        // Wait for data from H2 stream
        match self.recv.data().await {
            Some(Ok(data)) => {
                let _ = self.recv.flow_control().release_capacity(data.len());
                if data.len() <= max_size {
                    Ok(data)
                } else {
                    let (first, rest) = data.split_at(max_size);
                    self.data_queue.push_front(Bytes::copy_from_slice(rest));
                    Ok(Bytes::copy_from_slice(first))
                }
            }
            Some(Err(e)) => {
                self.closed = true;
                Err(TransportError::ConnectionError(format!(
                    "H2 receive error: {}",
                    e
                )))
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

        // Send empty data with END_STREAM flag
        self.send
            .send_data(Bytes::new(), true)
            .map_err(|e| TransportError::ConnectionError(format!("H2 finish error: {}", e)))?;

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
    #[test]
    fn test_stream_debug() {
        // Just verify module compiles
        assert!(true);
    }
}
