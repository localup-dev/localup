package localup

// MessageType represents the type of a TunnelMessage.
// These must match the Rust enum variant indices for bincode compatibility.
// The order matches the Rust TunnelMessage enum in localup-proto/src/messages.rs
type MessageType uint32

const (
	// Control messages (Stream 0) - matches Rust enum order
	MessageTypePing          MessageType = 0
	MessageTypePong          MessageType = 1
	MessageTypeConnect       MessageType = 2
	MessageTypeConnected     MessageType = 3
	MessageTypeDisconnect    MessageType = 4
	MessageTypeDisconnectAck MessageType = 5

	// TCP messages
	MessageTypeTcpConnect MessageType = 6
	MessageTypeTcpData    MessageType = 7
	MessageTypeTcpClose   MessageType = 8

	// TLS/SNI messages
	MessageTypeTlsConnect MessageType = 9
	MessageTypeTlsData    MessageType = 10
	MessageTypeTlsClose   MessageType = 11

	// HTTP messages
	MessageTypeHttpRequest  MessageType = 12
	MessageTypeHttpResponse MessageType = 13
	MessageTypeHttpChunk    MessageType = 14

	// HTTP Stream messages (transparent pass-through)
	MessageTypeHttpStreamConnect MessageType = 15
	MessageTypeHttpStreamData    MessageType = 16
	MessageTypeHttpStreamClose   MessageType = 17
)

// TunnelMessage is the base interface for all protocol messages.
type TunnelMessage interface {
	MessageType() MessageType
}

// ConnectMessage is sent by the client to register a tunnel.
type ConnectMessage struct {
	TunnelID  string          `json:"localup_id"`
	AuthToken string          `json:"auth_token"`
	Protocols []ProtocolSpec  `json:"protocols"`
	Config    TunnelConfigMsg `json:"config"`
}

func (m *ConnectMessage) MessageType() MessageType { return MessageTypeConnect }

// ProtocolSpec specifies a protocol configuration in the Connect message.
type ProtocolSpec struct {
	Type       string  `json:"type"` // "tcp", "tls", "http", "https"
	Port       uint16  `json:"port,omitempty"`
	SNIPattern string  `json:"sni_pattern,omitempty"`
	Subdomain  *string `json:"subdomain,omitempty"`
}

// TunnelConfigMsg is the tunnel configuration sent in Connect message.
type TunnelConfigMsg struct {
	LocalHost           string         `json:"local_host"`
	LocalPort           *uint16        `json:"local_port,omitempty"`
	LocalHTTPS          bool           `json:"local_https"`
	ExitNode            ExitNodeConfig `json:"exit_node"`
	Failover            bool           `json:"failover"`
	IPAllowlist         []string       `json:"ip_allowlist"`
	EnableCompression   bool           `json:"enable_compression"`
	EnableMultiplexing  bool           `json:"enable_multiplexing"`
}

// ExitNodeConfig specifies how to select an exit node.
type ExitNodeConfig struct {
	Type    string   `json:"type"` // "auto", "nearest", "specific", "multi_region", "custom"
	Region  string   `json:"region,omitempty"`
	Regions []string `json:"regions,omitempty"`
	Custom  string   `json:"custom,omitempty"`
}

// ConnectedMessage is sent by the relay after successful registration.
type ConnectedMessage struct {
	TunnelID  string     `json:"localup_id"`
	Endpoints []Endpoint `json:"endpoints"`
}

func (m *ConnectedMessage) MessageType() MessageType { return MessageTypeConnected }

// Endpoint represents a public endpoint allocated by the relay.
type Endpoint struct {
	Protocol string `json:"protocol"` // "tcp", "tls", "http", "https"
	URL      string `json:"url"`      // e.g., "https://myapp.localup.io"
	Port     uint16 `json:"port,omitempty"`
}

// PingMessage is a heartbeat message from client to relay.
type PingMessage struct {
	Timestamp uint64 `json:"timestamp"`
}

func (m *PingMessage) MessageType() MessageType { return MessageTypePing }

// PongMessage is a heartbeat response from relay to client.
type PongMessage struct {
	Timestamp uint64 `json:"timestamp"`
}

func (m *PongMessage) MessageType() MessageType { return MessageTypePong }

// DisconnectMessage is sent to terminate a tunnel.
type DisconnectMessage struct {
	Reason string `json:"reason"`
}

func (m *DisconnectMessage) MessageType() MessageType { return MessageTypeDisconnect }

// DisconnectAckMessage acknowledges a disconnect.
type DisconnectAckMessage struct {
	TunnelID string `json:"localup_id"`
}

func (m *DisconnectAckMessage) MessageType() MessageType { return MessageTypeDisconnectAck }

// TcpConnectMessage is sent when a new TCP connection arrives.
type TcpConnectMessage struct {
	StreamID   uint32 `json:"stream_id"`
	RemoteAddr string `json:"remote_addr"`
	RemotePort uint16 `json:"remote_port"`
}

func (m *TcpConnectMessage) MessageType() MessageType { return MessageTypeTcpConnect }

// TcpDataMessage carries TCP data.
type TcpDataMessage struct {
	StreamID uint32 `json:"stream_id"`
	Data     []byte `json:"data"`
}

func (m *TcpDataMessage) MessageType() MessageType { return MessageTypeTcpData }

// TcpCloseMessage closes a TCP stream.
type TcpCloseMessage struct {
	StreamID uint32 `json:"stream_id"`
}

func (m *TcpCloseMessage) MessageType() MessageType { return MessageTypeTcpClose }

// TlsConnectMessage is sent when a new TLS connection arrives (SNI-based).
type TlsConnectMessage struct {
	StreamID    uint32 `json:"stream_id"`
	SNI         string `json:"sni"`
	ClientHello []byte `json:"client_hello"`
}

func (m *TlsConnectMessage) MessageType() MessageType { return MessageTypeTlsConnect }

// TlsDataMessage carries TLS data (passthrough).
type TlsDataMessage struct {
	StreamID uint32 `json:"stream_id"`
	Data     []byte `json:"data"`
}

func (m *TlsDataMessage) MessageType() MessageType { return MessageTypeTlsData }

// TlsCloseMessage closes a TLS stream.
type TlsCloseMessage struct {
	StreamID uint32 `json:"stream_id"`
}

func (m *TlsCloseMessage) MessageType() MessageType { return MessageTypeTlsClose }

// HttpRequestMessage carries an HTTP request.
type HttpRequestMessage struct {
	StreamID uint32            `json:"stream_id"`
	Method   string            `json:"method"`
	URI      string            `json:"uri"`
	Headers  map[string]string `json:"headers"`
	Body     []byte            `json:"body,omitempty"`
}

func (m *HttpRequestMessage) MessageType() MessageType { return MessageTypeHttpRequest }

// HttpResponseMessage carries an HTTP response.
type HttpResponseMessage struct {
	StreamID uint32            `json:"stream_id"`
	Status   uint16            `json:"status"`
	Headers  map[string]string `json:"headers"`
	Body     []byte            `json:"body,omitempty"`
}

func (m *HttpResponseMessage) MessageType() MessageType { return MessageTypeHttpResponse }

// HttpChunkMessage carries a chunk of HTTP body data.
type HttpChunkMessage struct {
	StreamID uint32 `json:"stream_id"`
	Chunk    []byte `json:"chunk"`
	IsFinal  bool   `json:"is_final"`
}

func (m *HttpChunkMessage) MessageType() MessageType { return MessageTypeHttpChunk }

// HttpStreamConnectMessage is sent for HTTP stream passthrough.
type HttpStreamConnectMessage struct {
	StreamID    uint32 `json:"stream_id"`
	Host        string `json:"host"`
	InitialData []byte `json:"initial_data"`
}

func (m *HttpStreamConnectMessage) MessageType() MessageType { return MessageTypeHttpStreamConnect }

// HttpStreamDataMessage carries HTTP stream data.
type HttpStreamDataMessage struct {
	StreamID uint32 `json:"stream_id"`
	Data     []byte `json:"data"`
}

func (m *HttpStreamDataMessage) MessageType() MessageType { return MessageTypeHttpStreamData }

// HttpStreamCloseMessage closes an HTTP stream.
type HttpStreamCloseMessage struct {
	StreamID uint32 `json:"stream_id"`
}

func (m *HttpStreamCloseMessage) MessageType() MessageType { return MessageTypeHttpStreamClose }
