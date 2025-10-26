//! Codec for encoding/decoding tunnel messages

use crate::messages::TunnelMessage;
use bytes::{Bytes, BytesMut};
use thiserror::Error;

/// Codec errors
#[derive(Debug, Error)]
pub enum CodecError {
    #[error("Serialization error: {0}")]
    SerializationError(#[from] bincode::Error),

    #[error("Message too large: {0} bytes")]
    MessageTooLarge(usize),

    #[error("Incomplete message")]
    IncompleteMessage,
}

/// Tunnel message codec
pub struct TunnelCodec;

impl TunnelCodec {
    /// Maximum message size (16MB)
    pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

    /// Encode a tunnel message to bytes
    ///
    /// Format: [length: u32][payload: bincode serialized message]
    pub fn encode(msg: &TunnelMessage) -> Result<Bytes, CodecError> {
        let payload = bincode::serialize(msg)?;

        if payload.len() > Self::MAX_MESSAGE_SIZE {
            return Err(CodecError::MessageTooLarge(payload.len()));
        }

        let mut buf = BytesMut::with_capacity(4 + payload.len());
        buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        buf.extend_from_slice(&payload);

        Ok(buf.freeze())
    }

    /// Decode a tunnel message from bytes
    ///
    /// Returns Ok(Some(message)) if a complete message was decoded,
    /// Ok(None) if more data is needed,
    /// Err on error
    pub fn decode(buf: &mut BytesMut) -> Result<Option<TunnelMessage>, CodecError> {
        // Need at least 4 bytes for length header
        if buf.len() < 4 {
            return Ok(None);
        }

        // Read length
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&buf[..4]);
        let length = u32::from_be_bytes(length_bytes) as usize;

        if length > Self::MAX_MESSAGE_SIZE {
            return Err(CodecError::MessageTooLarge(length));
        }

        // Check if we have the full message
        if buf.len() < 4 + length {
            return Ok(None);
        }

        // Remove length header
        let _ = buf.split_to(4);

        // Extract message bytes
        let msg_bytes = buf.split_to(length);

        // Deserialize
        let msg: TunnelMessage = bincode::deserialize(&msg_bytes)?;

        Ok(Some(msg))
    }

    /// Try to decode multiple messages from buffer
    pub fn decode_all(buf: &mut BytesMut) -> Result<Vec<TunnelMessage>, CodecError> {
        let mut messages = Vec::new();

        while let Some(msg) = Self::decode(buf)? {
            messages.push(msg);
        }

        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode() {
        let msg = TunnelMessage::Ping { timestamp: 12345 };

        let encoded = TunnelCodec::encode(&msg).unwrap();
        let mut buf = BytesMut::from(encoded.as_ref());

        let decoded = TunnelCodec::decode(&mut buf).unwrap();
        assert_eq!(decoded, Some(msg));
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_decode_incomplete() {
        let msg = TunnelMessage::Pong { timestamp: 67890 };
        let encoded = TunnelCodec::encode(&msg).unwrap();

        // Only provide length header
        let mut buf = BytesMut::from(&encoded[..4]);
        let result = TunnelCodec::decode(&mut buf).unwrap();
        assert_eq!(result, None);

        // Provide rest of message
        buf.extend_from_slice(&encoded[4..]);
        let result = TunnelCodec::decode(&mut buf).unwrap();
        assert_eq!(result, Some(msg));
    }

    #[test]
    fn test_decode_multiple() {
        let msg1 = TunnelMessage::Ping { timestamp: 111 };
        let msg2 = TunnelMessage::Pong { timestamp: 222 };

        let encoded1 = TunnelCodec::encode(&msg1).unwrap();
        let encoded2 = TunnelCodec::encode(&msg2).unwrap();

        let mut buf = BytesMut::new();
        buf.extend_from_slice(&encoded1);
        buf.extend_from_slice(&encoded2);

        let messages = TunnelCodec::decode_all(&mut buf).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], msg1);
        assert_eq!(messages[1], msg2);
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_tcp_data_encode_decode() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let msg = TunnelMessage::TcpData {
            stream_id: 42,
            data,
        };

        let encoded = TunnelCodec::encode(&msg).unwrap();
        let mut buf = BytesMut::from(encoded.as_ref());

        let decoded = TunnelCodec::decode(&mut buf).unwrap();
        assert!(decoded.is_some());

        if let TunnelMessage::TcpData { stream_id, data } = decoded.unwrap() {
            assert_eq!(stream_id, 42);
            assert_eq!(data, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        } else {
            panic!("Expected TcpData message");
        }
    }
}
