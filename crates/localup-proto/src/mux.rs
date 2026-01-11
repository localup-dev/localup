//! Multiplexing primitives for tunnel protocol

use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Stream identifier
pub type StreamId = u32;

/// Frame types for multiplexing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum FrameType {
    Control = 0,
    Data = 1,
    Close = 2,
    WindowUpdate = 3,
}

impl TryFrom<u8> for FrameType {
    type Error = MuxError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(FrameType::Control),
            1 => Ok(FrameType::Data),
            2 => Ok(FrameType::Close),
            3 => Ok(FrameType::WindowUpdate),
            _ => Err(MuxError::InvalidFrameType(value)),
        }
    }
}

/// Frame flags
#[derive(Debug, Clone, Copy)]
pub struct FrameFlags(u8);

impl FrameFlags {
    pub const FIN: u8 = 0b0000_0001;
    pub const ACK: u8 = 0b0000_0010;
    pub const RST: u8 = 0b0000_0100;

    pub fn new() -> Self {
        Self(0)
    }

    pub fn with_fin(mut self) -> Self {
        self.0 |= Self::FIN;
        self
    }

    pub fn with_ack(mut self) -> Self {
        self.0 |= Self::ACK;
        self
    }

    pub fn with_rst(mut self) -> Self {
        self.0 |= Self::RST;
        self
    }

    pub fn has_fin(&self) -> bool {
        self.0 & Self::FIN != 0
    }

    pub fn has_ack(&self) -> bool {
        self.0 & Self::ACK != 0
    }

    pub fn has_rst(&self) -> bool {
        self.0 & Self::RST != 0
    }

    pub fn as_u8(&self) -> u8 {
        self.0
    }

    pub fn from_u8(value: u8) -> Self {
        Self(value)
    }
}

impl Default for FrameFlags {
    fn default() -> Self {
        Self::new()
    }
}

/// Multiplexed frame
#[derive(Debug, Clone)]
pub struct Frame {
    pub stream_id: StreamId,
    pub frame_type: FrameType,
    pub flags: FrameFlags,
    pub payload: Bytes,
}

impl Frame {
    /// Frame header size: stream_id (4) + frame_type (1) + flags (1) + length (4) = 10 bytes
    pub const HEADER_SIZE: usize = 10;

    pub fn new(stream_id: StreamId, frame_type: FrameType, payload: Bytes) -> Self {
        Self {
            stream_id,
            frame_type,
            flags: FrameFlags::new(),
            payload,
        }
    }

    pub fn control(payload: Bytes) -> Self {
        Self::new(crate::CONTROL_STREAM_ID, FrameType::Control, payload)
    }

    pub fn data(stream_id: StreamId, payload: Bytes) -> Self {
        Self::new(stream_id, FrameType::Data, payload)
    }

    pub fn close(stream_id: StreamId) -> Self {
        Self::new(stream_id, FrameType::Close, Bytes::new())
    }

    pub fn with_flags(mut self, flags: FrameFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Encode frame to bytes
    pub fn encode(&self) -> Result<Bytes, MuxError> {
        let payload_len = self.payload.len();
        if payload_len > crate::MAX_FRAME_SIZE as usize {
            return Err(MuxError::FrameTooLarge(payload_len));
        }

        let mut buf = BytesMut::with_capacity(Self::HEADER_SIZE + payload_len);

        buf.put_u32(self.stream_id);
        buf.put_u8(self.frame_type as u8);
        buf.put_u8(self.flags.as_u8());
        buf.put_u32(payload_len as u32);
        buf.put(self.payload.clone());

        Ok(buf.freeze())
    }

    /// Decode frame from bytes
    pub fn decode(mut buf: Bytes) -> Result<Self, MuxError> {
        if buf.len() < Self::HEADER_SIZE {
            return Err(MuxError::IncompleteFrame);
        }

        let stream_id = buf.get_u32();
        let frame_type = FrameType::try_from(buf.get_u8())?;
        let flags = FrameFlags::from_u8(buf.get_u8());
        let length = buf.get_u32();

        if length > crate::MAX_FRAME_SIZE {
            return Err(MuxError::FrameTooLarge(length as usize));
        }

        if buf.remaining() < length as usize {
            return Err(MuxError::IncompleteFrame);
        }

        let payload = buf.split_to(length as usize);

        Ok(Self {
            stream_id,
            frame_type,
            flags,
            payload,
        })
    }
}

/// Multiplexer errors
#[derive(Debug, Error)]
pub enum MuxError {
    #[error("Invalid frame type: {0}")]
    InvalidFrameType(u8),

    #[error("Frame too large: {0} bytes")]
    FrameTooLarge(usize),

    #[error("Incomplete frame")]
    IncompleteFrame,

    #[error("Stream not found: {0}")]
    StreamNotFound(StreamId),

    #[error("Stream already exists: {0}")]
    StreamAlreadyExists(StreamId),

    #[error("No available stream IDs")]
    NoAvailableStreamIds,
}

/// Stream state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Open,
    HalfClosed,
    Closed,
}

/// Multiplexer for managing streams
pub struct Multiplexer {
    next_stream_id: Arc<Mutex<StreamId>>,
    streams: Arc<Mutex<HashMap<StreamId, StreamState>>>,
}

impl Multiplexer {
    pub fn new() -> Self {
        Self {
            next_stream_id: Arc::new(Mutex::new(1)), // Stream 0 is reserved for control
            streams: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Allocate a new stream ID
    pub fn allocate_stream(&self) -> Result<StreamId, MuxError> {
        let mut next_id = self.next_stream_id.lock().unwrap();
        let mut streams = self.streams.lock().unwrap();

        // Find next available stream ID
        let start_id = *next_id;
        loop {
            let id = *next_id;

            // Skip control stream
            if id == crate::CONTROL_STREAM_ID {
                *next_id = 1;
                continue;
            }

            if let std::collections::hash_map::Entry::Vacant(e) = streams.entry(id) {
                e.insert(StreamState::Open);
                *next_id = id.wrapping_add(1);
                return Ok(id);
            }

            *next_id = id.wrapping_add(1);

            // Prevent infinite loop
            if *next_id == start_id {
                return Err(MuxError::NoAvailableStreamIds);
            }
        }
    }

    /// Register an incoming stream
    pub fn register_stream(&self, stream_id: StreamId) -> Result<(), MuxError> {
        let mut streams = self.streams.lock().unwrap();

        if streams.contains_key(&stream_id) {
            return Err(MuxError::StreamAlreadyExists(stream_id));
        }

        streams.insert(stream_id, StreamState::Open);
        Ok(())
    }

    /// Close a stream
    pub fn close_stream(&self, stream_id: StreamId) -> Result<(), MuxError> {
        let mut streams = self.streams.lock().unwrap();

        if !streams.contains_key(&stream_id) {
            return Err(MuxError::StreamNotFound(stream_id));
        }

        streams.insert(stream_id, StreamState::Closed);
        Ok(())
    }

    /// Remove a closed stream
    pub fn remove_stream(&self, stream_id: StreamId) {
        let mut streams = self.streams.lock().unwrap();
        streams.remove(&stream_id);
    }

    /// Get stream state
    pub fn get_stream_state(&self, stream_id: StreamId) -> Option<StreamState> {
        let streams = self.streams.lock().unwrap();
        streams.get(&stream_id).copied()
    }

    /// Get number of active streams
    pub fn active_streams(&self) -> usize {
        let streams = self.streams.lock().unwrap();
        streams
            .values()
            .filter(|&&s| s == StreamState::Open)
            .count()
    }
}

impl Default for Multiplexer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_encode_decode() {
        let payload = Bytes::from("hello world");
        let frame = Frame::data(42, payload.clone());

        let encoded = frame.encode().unwrap();
        let decoded = Frame::decode(encoded).unwrap();

        assert_eq!(decoded.stream_id, 42);
        assert_eq!(decoded.frame_type, FrameType::Data);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_frame_with_flags() {
        let frame = Frame::close(10).with_flags(FrameFlags::new().with_fin());

        assert!(frame.flags.has_fin());
        assert!(!frame.flags.has_ack());

        let encoded = frame.encode().unwrap();
        let decoded = Frame::decode(encoded).unwrap();

        assert!(decoded.flags.has_fin());
    }

    #[test]
    fn test_multiplexer_allocate() {
        let mux = Multiplexer::new();

        let stream1 = mux.allocate_stream().unwrap();
        let stream2 = mux.allocate_stream().unwrap();

        assert_ne!(stream1, stream2);
        assert_ne!(stream1, crate::CONTROL_STREAM_ID);
        assert_ne!(stream2, crate::CONTROL_STREAM_ID);
    }

    #[test]
    fn test_multiplexer_close_stream() {
        let mux = Multiplexer::new();

        let stream_id = mux.allocate_stream().unwrap();
        assert_eq!(mux.get_stream_state(stream_id), Some(StreamState::Open));

        mux.close_stream(stream_id).unwrap();
        assert_eq!(mux.get_stream_state(stream_id), Some(StreamState::Closed));
    }

    #[test]
    fn test_multiplexer_active_streams() {
        let mux = Multiplexer::new();

        let stream1 = mux.allocate_stream().unwrap();
        let _stream2 = mux.allocate_stream().unwrap();

        assert_eq!(mux.active_streams(), 2);

        mux.close_stream(stream1).unwrap();
        assert_eq!(mux.active_streams(), 1);
    }
}
