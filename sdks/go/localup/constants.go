package localup

// Version constants
const (
	// SDKVersion is the version of this SDK.
	SDKVersion = "0.1.0"

	// ProtocolVersion is the LocalUp protocol version supported by this SDK.
	ProtocolVersion = 1
)

// Default values
const (
	// DefaultRelayAddr is the default relay server address.
	DefaultRelayAddr = "relay.localup.io:4443"

	// DefaultQUICPort is the default port for QUIC connections.
	DefaultQUICPort = 4443

	// DefaultHTTPSPort is the default port for HTTPS/H2 connections.
	DefaultHTTPSPort = 443

	// MaxFrameSize is the maximum size of a single protocol frame.
	MaxFrameSize = 16 * 1024 * 1024 // 16MB

	// ControlStreamID is the stream ID reserved for control messages.
	ControlStreamID = 0
)

// Frame header constants
const (
	// FrameHeaderSize is the size of the frame header in bytes.
	// Format: stream_id(4) + type(1) + flags(1) + length(4)
	FrameHeaderSize = 10

	// LengthPrefixSize is the size of the message length prefix.
	LengthPrefixSize = 4
)

// Frame types
const (
	FrameTypeControl      uint8 = 0
	FrameTypeData         uint8 = 1
	FrameTypeClose        uint8 = 2
	FrameTypeWindowUpdate uint8 = 3
)

// Frame flags
const (
	FrameFlagFin uint8 = 0x01
	FrameFlagAck uint8 = 0x02
	FrameFlagRst uint8 = 0x04
)

// Protocol identifies the type of tunnel protocol.
type Protocol string

const (
	// ProtocolTCP creates a TCP tunnel with port-based routing.
	ProtocolTCP Protocol = "tcp"

	// ProtocolTLS creates a TLS tunnel with SNI-based routing (passthrough).
	ProtocolTLS Protocol = "tls"

	// ProtocolHTTP creates an HTTP tunnel with host-based routing.
	ProtocolHTTP Protocol = "http"

	// ProtocolHTTPS creates an HTTPS tunnel with TLS termination at the relay.
	ProtocolHTTPS Protocol = "https"
)

// Well-known endpoints
const (
	// WellKnownProtocolsPath is the path for protocol discovery.
	WellKnownProtocolsPath = "/.well-known/localup-protocols"
)
