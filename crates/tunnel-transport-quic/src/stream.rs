//! QUIC stream implementation

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use quinn::{RecvStream, SendStream};
use tracing::trace;
use tunnel_proto::{TunnelCodec, TunnelMessage};
use tunnel_transport::{TransportError, TransportResult, TransportStream};

/// QUIC stream wrapper
#[derive(Debug)]
pub struct QuicStream {
    send: SendStream,
    recv: RecvStream,
    stream_id: u64,
    closed: bool,
    // Buffer for accumulating received data for message decoding
    recv_buffer: BytesMut,
}

impl QuicStream {
    pub fn new(send: SendStream, recv: RecvStream) -> Self {
        let stream_id = send.id().index();
        Self {
            send,
            recv,
            stream_id,
            closed: false,
            recv_buffer: BytesMut::with_capacity(8192),
        }
    }

    /// Split the stream into separate send and receive halves
    /// This allows concurrent reading and writing without mutexes!
    pub fn split(self) -> (QuicSendHalf, QuicRecvHalf) {
        let send_half = QuicSendHalf {
            send: self.send,
            stream_id: self.stream_id,
            closed: false,
        };
        let recv_half = QuicRecvHalf {
            recv: self.recv,
            stream_id: self.stream_id,
            closed: false,
            recv_buffer: self.recv_buffer,
        };
        (send_half, recv_half)
    }
}

#[async_trait]
impl TransportStream for QuicStream {
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
        if self.closed {
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
                    // Need more data - read from stream
                    match self.recv.read_chunk(8192, true).await {
                        Ok(Some(chunk)) => {
                            self.recv_buffer.extend_from_slice(&chunk.bytes);
                        }
                        Ok(None) => {
                            // Stream finished
                            self.closed = true;
                            if self.recv_buffer.is_empty() {
                                return Ok(None);
                            } else {
                                return Err(TransportError::ProtocolError(
                                    "Incomplete message in buffer".to_string(),
                                ));
                            }
                        }
                        Err(e) => {
                            self.closed = true;
                            return Err(TransportError::ConnectionError(e.to_string()));
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

        self.send
            .write_all(data)
            .await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;

        Ok(())
    }

    async fn recv_bytes(&mut self, max_size: usize) -> TransportResult<Bytes> {
        if self.closed {
            return Ok(Bytes::new());
        }

        // Use read_chunk instead of read_to_end for bidirectional communication
        match self.recv.read_chunk(max_size, true).await {
            Ok(Some(chunk)) => Ok(chunk.bytes),
            Ok(None) => {
                // Stream finished gracefully
                self.closed = true;
                Ok(Bytes::new())
            }
            Err(quinn::ReadError::ConnectionLost(e)) => {
                self.closed = true;
                Err(TransportError::ConnectionError(format!(
                    "Connection lost: {}",
                    e
                )))
            }
            Err(e) => Err(TransportError::ConnectionError(e.to_string())),
        }
    }

    async fn finish(&mut self) -> TransportResult<()> {
        if self.closed {
            return Ok(());
        }

        self.send
            .finish()
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;

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

/// Send half of a split QUIC stream
pub struct QuicSendHalf {
    send: SendStream,
    stream_id: u64,
    closed: bool,
}

impl QuicSendHalf {
    pub async fn send_message(&mut self, message: &TunnelMessage) -> TransportResult<()> {
        if self.closed {
            return Err(TransportError::StreamClosed);
        }

        let encoded = TunnelCodec::encode(message)
            .map_err(|e| TransportError::ProtocolError(e.to_string()))?;

        self.send
            .write_all(&encoded)
            .await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;

        trace!("Sent message on stream {}: {:?}", self.stream_id, message);
        Ok(())
    }

    pub async fn finish(mut self) -> TransportResult<()> {
        self.send
            .finish()
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
        self.closed = true;
        Ok(())
    }

    pub fn stream_id(&self) -> u64 {
        self.stream_id
    }
}

/// Receive half of a split QUIC stream
pub struct QuicRecvHalf {
    recv: RecvStream,
    stream_id: u64,
    closed: bool,
    recv_buffer: BytesMut,
}

impl QuicRecvHalf {
    pub async fn recv_message(&mut self) -> TransportResult<Option<TunnelMessage>> {
        if self.closed {
            return Ok(None);
        }

        loop {
            // Try to decode a message from the buffer
            match TunnelCodec::decode(&mut self.recv_buffer)
                .map_err(|e| TransportError::ProtocolError(e.to_string()))?
            {
                Some(message) => {
                    trace!(
                        "Received message on stream {}: {:?}",
                        self.stream_id,
                        message
                    );
                    return Ok(Some(message));
                }
                None => {
                    // Need more data - read from stream
                    match self.recv.read_chunk(8192, true).await {
                        Ok(Some(chunk)) => {
                            self.recv_buffer.extend_from_slice(&chunk.bytes);
                            // Continue loop to try decoding again
                        }
                        Ok(None) => {
                            // Stream finished gracefully
                            self.closed = true;
                            return Ok(None);
                        }
                        Err(quinn::ReadError::ConnectionLost(e)) => {
                            self.closed = true;
                            return Err(TransportError::ConnectionError(format!(
                                "Connection lost: {}",
                                e
                            )));
                        }
                        Err(e) => {
                            return Err(TransportError::ConnectionError(e.to_string()));
                        }
                    }
                }
            }
        }
    }

    pub fn stream_id(&self) -> u64 {
        self.stream_id
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_stream_id() {
        // We can't easily create a real QUIC stream in unit tests,
        // Integration tests cover the full QUIC stream functionality
        // This test exists to maintain the test module structure
    }
}
