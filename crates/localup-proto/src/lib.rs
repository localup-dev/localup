//! Tunnel Protocol Definitions
//!
//! This crate defines the core protocol types, messages, and multiplexing primitives
//! for the geo-distributed tunnel system.

pub mod codec;
pub mod discovery;
pub mod messages;
pub mod mux;

pub use codec::{CodecError, TunnelCodec};
pub use discovery::{
    ProtocolDiscoveryResponse, TransportEndpoint, TransportProtocol, WELL_KNOWN_PATH,
};
pub use messages::*;
pub use mux::{Frame, FrameType, Multiplexer, StreamId};

/// Protocol version
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum frame size (16MB)
pub const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024;

/// Reserved stream ID for control messages
pub const CONTROL_STREAM_ID: u32 = 0;
