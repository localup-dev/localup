package localup

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io"
)

// MessageCodec encodes and decodes TunnelMessages.
type MessageCodec struct{}

// NewMessageCodec creates a new message codec.
func NewMessageCodec() *MessageCodec {
	return &MessageCodec{}
}

// EncodeMessage encodes a TunnelMessage to bytes.
// Format: [4-byte big-endian length][bincode payload]
func (c *MessageCodec) EncodeMessage(msg TunnelMessage) ([]byte, error) {
	enc := NewBincodeEncoder()

	// Write the enum variant index
	enc.WriteU32(uint32(msg.MessageType()))

	// Encode the message-specific fields
	switch m := msg.(type) {
	case *ConnectMessage:
		c.encodeConnect(enc, m)
	case *ConnectedMessage:
		c.encodeConnected(enc, m)
	case *PingMessage:
		enc.WriteU64(m.Timestamp)
	case *PongMessage:
		enc.WriteU64(m.Timestamp)
	case *DisconnectMessage:
		enc.WriteString(m.Reason)
	case *DisconnectAckMessage:
		enc.WriteString(m.TunnelID)
	case *TcpConnectMessage:
		enc.WriteU32(m.StreamID)
		enc.WriteString(m.RemoteAddr)
		enc.WriteU16(m.RemotePort)
	case *TcpDataMessage:
		enc.WriteU32(m.StreamID)
		enc.WriteBytes(m.Data)
	case *TcpCloseMessage:
		enc.WriteU32(m.StreamID)
	case *TlsConnectMessage:
		enc.WriteU32(m.StreamID)
		enc.WriteString(m.SNI)
		enc.WriteBytes(m.ClientHello)
	case *TlsDataMessage:
		enc.WriteU32(m.StreamID)
		enc.WriteBytes(m.Data)
	case *TlsCloseMessage:
		enc.WriteU32(m.StreamID)
	case *HttpRequestMessage:
		c.encodeHttpRequest(enc, m)
	case *HttpResponseMessage:
		c.encodeHttpResponse(enc, m)
	case *HttpChunkMessage:
		enc.WriteU32(m.StreamID)
		enc.WriteBytes(m.Chunk)
		enc.WriteBool(m.IsFinal)
	case *HttpStreamConnectMessage:
		enc.WriteU32(m.StreamID)
		enc.WriteString(m.Host)
		enc.WriteBytes(m.InitialData)
	case *HttpStreamDataMessage:
		enc.WriteU32(m.StreamID)
		enc.WriteBytes(m.Data)
	case *HttpStreamCloseMessage:
		enc.WriteU32(m.StreamID)
	default:
		return nil, fmt.Errorf("unknown message type: %T", msg)
	}

	payload := enc.Bytes()

	// Prepend length (big-endian)
	result := make([]byte, LengthPrefixSize+len(payload))
	binary.BigEndian.PutUint32(result[:LengthPrefixSize], uint32(len(payload)))
	copy(result[LengthPrefixSize:], payload)

	return result, nil
}

// DecodeMessage decodes a TunnelMessage from a reader.
// Expects: [4-byte big-endian length][bincode payload]
func (c *MessageCodec) DecodeMessage(r io.Reader) (TunnelMessage, error) {
	// Read length prefix
	var lengthBuf [LengthPrefixSize]byte
	if _, err := io.ReadFull(r, lengthBuf[:]); err != nil {
		return nil, fmt.Errorf("failed to read length: %w", err)
	}
	length := binary.BigEndian.Uint32(lengthBuf[:])

	if length > MaxFrameSize {
		return nil, fmt.Errorf("message too large: %d bytes", length)
	}

	// Read payload
	payload := make([]byte, length)
	if _, err := io.ReadFull(r, payload); err != nil {
		return nil, fmt.Errorf("failed to read payload: %w", err)
	}

	return c.DecodeMessageBytes(payload)
}

// DecodeMessageBytes decodes a TunnelMessage from bytes (without length prefix).
func (c *MessageCodec) DecodeMessageBytes(data []byte) (TunnelMessage, error) {
	dec := NewBincodeDecoderBytes(data)

	// Read the enum variant index
	variant, err := dec.ReadU32()
	if err != nil {
		return nil, fmt.Errorf("failed to read message type: %w", err)
	}

	msgType := MessageType(variant)

	switch msgType {
	case MessageTypeConnect:
		return c.decodeConnect(dec)
	case MessageTypeConnected:
		return c.decodeConnected(dec)
	case MessageTypePing:
		ts, err := dec.ReadU64()
		if err != nil {
			return nil, err
		}
		return &PingMessage{Timestamp: ts}, nil
	case MessageTypePong:
		ts, err := dec.ReadU64()
		if err != nil {
			return nil, err
		}
		return &PongMessage{Timestamp: ts}, nil
	case MessageTypeDisconnect:
		reason, err := dec.ReadString()
		if err != nil {
			return nil, err
		}
		return &DisconnectMessage{Reason: reason}, nil
	case MessageTypeDisconnectAck:
		id, err := dec.ReadString()
		if err != nil {
			return nil, err
		}
		return &DisconnectAckMessage{TunnelID: id}, nil
	case MessageTypeTcpConnect:
		return c.decodeTcpConnect(dec)
	case MessageTypeTcpData:
		return c.decodeTcpData(dec)
	case MessageTypeTcpClose:
		id, err := dec.ReadU32()
		if err != nil {
			return nil, err
		}
		return &TcpCloseMessage{StreamID: id}, nil
	case MessageTypeTlsConnect:
		return c.decodeTlsConnect(dec)
	case MessageTypeTlsData:
		return c.decodeTlsData(dec)
	case MessageTypeTlsClose:
		id, err := dec.ReadU32()
		if err != nil {
			return nil, err
		}
		return &TlsCloseMessage{StreamID: id}, nil
	case MessageTypeHttpRequest:
		return c.decodeHttpRequest(dec)
	case MessageTypeHttpResponse:
		return c.decodeHttpResponse(dec)
	case MessageTypeHttpChunk:
		return c.decodeHttpChunk(dec)
	case MessageTypeHttpStreamConnect:
		return c.decodeHttpStreamConnect(dec)
	case MessageTypeHttpStreamData:
		return c.decodeHttpStreamData(dec)
	case MessageTypeHttpStreamClose:
		id, err := dec.ReadU32()
		if err != nil {
			return nil, err
		}
		return &HttpStreamCloseMessage{StreamID: id}, nil
	default:
		return nil, fmt.Errorf("unknown message type: %d", variant)
	}
}

// encodeConnect encodes a ConnectMessage.
func (c *MessageCodec) encodeConnect(enc *BincodeEncoder, m *ConnectMessage) {
	enc.WriteString(m.TunnelID)
	enc.WriteString(m.AuthToken)

	// Encode protocols as Vec<Protocol>
	enc.WriteVecLen(len(m.Protocols))
	for _, p := range m.Protocols {
		c.encodeProtocolSpec(enc, &p)
	}

	// Encode config
	c.encodeTunnelConfig(enc, &m.Config)
}

// encodeProtocolSpec encodes a protocol specification.
// This matches the Rust Protocol enum.
func (c *MessageCodec) encodeProtocolSpec(enc *BincodeEncoder, p *ProtocolSpec) {
	switch p.Type {
	case "tcp":
		enc.WriteU32(0) // Tcp variant
		enc.WriteU16(p.Port)
	case "tls":
		enc.WriteU32(1) // Tls variant
		enc.WriteU16(p.Port)
		enc.WriteString(p.SNIPattern)
	case "http":
		enc.WriteU32(2) // Http variant
		enc.WriteOptionString(p.Subdomain)
	case "https":
		enc.WriteU32(3) // Https variant
		enc.WriteOptionString(p.Subdomain)
	}
}

// encodeTunnelConfig encodes a tunnel configuration.
func (c *MessageCodec) encodeTunnelConfig(enc *BincodeEncoder, cfg *TunnelConfigMsg) {
	enc.WriteString(cfg.LocalHost)
	enc.WriteOptionU16(cfg.LocalPort)
	enc.WriteBool(cfg.LocalHTTPS)

	// Encode ExitNodeConfig
	c.encodeExitNodeConfig(enc, &cfg.ExitNode)

	enc.WriteBool(cfg.Failover)

	// IP allowlist
	enc.WriteVecLen(len(cfg.IPAllowlist))
	for _, ip := range cfg.IPAllowlist {
		enc.WriteString(ip)
	}

	enc.WriteBool(cfg.EnableCompression)
	enc.WriteBool(cfg.EnableMultiplexing)
}

// encodeExitNodeConfig encodes an exit node configuration.
func (c *MessageCodec) encodeExitNodeConfig(enc *BincodeEncoder, cfg *ExitNodeConfig) {
	switch cfg.Type {
	case "auto", "":
		enc.WriteU32(0) // Auto variant
	case "nearest":
		enc.WriteU32(1) // Nearest variant
	case "specific":
		enc.WriteU32(2) // Specific variant
		enc.WriteString(cfg.Region)
	case "multi_region":
		enc.WriteU32(3) // MultiRegion variant
		enc.WriteVecLen(len(cfg.Regions))
		for _, r := range cfg.Regions {
			enc.WriteString(r)
		}
	case "custom":
		enc.WriteU32(4) // Custom variant
		enc.WriteString(cfg.Custom)
	default:
		enc.WriteU32(0) // Default to Auto
	}
}

// encodeConnected encodes a ConnectedMessage.
func (c *MessageCodec) encodeConnected(enc *BincodeEncoder, m *ConnectedMessage) {
	enc.WriteString(m.TunnelID)
	enc.WriteVecLen(len(m.Endpoints))
	for _, ep := range m.Endpoints {
		enc.WriteString(ep.Protocol)
		enc.WriteString(ep.URL)
		enc.WriteU16(ep.Port)
	}
}

// encodeHttpRequest encodes an HttpRequestMessage.
func (c *MessageCodec) encodeHttpRequest(enc *BincodeEncoder, m *HttpRequestMessage) {
	enc.WriteU32(m.StreamID)
	enc.WriteString(m.Method)
	enc.WriteString(m.URI)

	// Headers as Vec<(String, String)>
	enc.WriteVecLen(len(m.Headers))
	for k, v := range m.Headers {
		enc.WriteString(k)
		enc.WriteString(v)
	}

	enc.WriteOptionBytes(m.Body)
}

// encodeHttpResponse encodes an HttpResponseMessage.
func (c *MessageCodec) encodeHttpResponse(enc *BincodeEncoder, m *HttpResponseMessage) {
	enc.WriteU32(m.StreamID)
	enc.WriteU16(m.Status)

	// Headers as Vec<(String, String)>
	enc.WriteVecLen(len(m.Headers))
	for k, v := range m.Headers {
		enc.WriteString(k)
		enc.WriteString(v)
	}

	enc.WriteOptionBytes(m.Body)
}

// decodeConnect decodes a ConnectMessage.
func (c *MessageCodec) decodeConnect(dec *BincodeDecoder) (*ConnectMessage, error) {
	tunnelID, err := dec.ReadString()
	if err != nil {
		return nil, err
	}
	authToken, err := dec.ReadString()
	if err != nil {
		return nil, err
	}

	// Decode protocols
	protocolCount, err := dec.ReadVecLen()
	if err != nil {
		return nil, err
	}
	protocols := make([]ProtocolSpec, protocolCount)
	for i := range protocols {
		p, err := c.decodeProtocolSpec(dec)
		if err != nil {
			return nil, err
		}
		protocols[i] = *p
	}

	// Decode config
	config, err := c.decodeTunnelConfig(dec)
	if err != nil {
		return nil, err
	}

	return &ConnectMessage{
		TunnelID:  tunnelID,
		AuthToken: authToken,
		Protocols: protocols,
		Config:    *config,
	}, nil
}

// decodeProtocolSpec decodes a protocol specification.
func (c *MessageCodec) decodeProtocolSpec(dec *BincodeDecoder) (*ProtocolSpec, error) {
	variant, err := dec.ReadU32()
	if err != nil {
		return nil, err
	}

	spec := &ProtocolSpec{}
	switch variant {
	case 0: // Tcp
		spec.Type = "tcp"
		spec.Port, err = dec.ReadU16()
	case 1: // Tls
		spec.Type = "tls"
		spec.Port, err = dec.ReadU16()
		if err != nil {
			return nil, err
		}
		spec.SNIPattern, err = dec.ReadString()
	case 2: // Http
		spec.Type = "http"
		spec.Subdomain, err = dec.ReadOptionString()
	case 3: // Https
		spec.Type = "https"
		spec.Subdomain, err = dec.ReadOptionString()
	default:
		return nil, fmt.Errorf("unknown protocol variant: %d", variant)
	}

	if err != nil {
		return nil, err
	}
	return spec, nil
}

// decodeTunnelConfig decodes a tunnel configuration.
func (c *MessageCodec) decodeTunnelConfig(dec *BincodeDecoder) (*TunnelConfigMsg, error) {
	localHost, err := dec.ReadString()
	if err != nil {
		return nil, err
	}
	localPort, err := dec.ReadOptionU16()
	if err != nil {
		return nil, err
	}
	localHTTPS, err := dec.ReadBool()
	if err != nil {
		return nil, err
	}

	exitNode, err := c.decodeExitNodeConfig(dec)
	if err != nil {
		return nil, err
	}

	failover, err := dec.ReadBool()
	if err != nil {
		return nil, err
	}

	ipCount, err := dec.ReadVecLen()
	if err != nil {
		return nil, err
	}
	ipAllowlist := make([]string, ipCount)
	for i := range ipAllowlist {
		ipAllowlist[i], err = dec.ReadString()
		if err != nil {
			return nil, err
		}
	}

	enableCompression, err := dec.ReadBool()
	if err != nil {
		return nil, err
	}
	enableMultiplexing, err := dec.ReadBool()
	if err != nil {
		return nil, err
	}

	return &TunnelConfigMsg{
		LocalHost:          localHost,
		LocalPort:          localPort,
		LocalHTTPS:         localHTTPS,
		ExitNode:           *exitNode,
		Failover:           failover,
		IPAllowlist:        ipAllowlist,
		EnableCompression:  enableCompression,
		EnableMultiplexing: enableMultiplexing,
	}, nil
}

// decodeExitNodeConfig decodes an exit node configuration.
func (c *MessageCodec) decodeExitNodeConfig(dec *BincodeDecoder) (*ExitNodeConfig, error) {
	variant, err := dec.ReadU32()
	if err != nil {
		return nil, err
	}

	cfg := &ExitNodeConfig{}
	switch variant {
	case 0: // Auto
		cfg.Type = "auto"
	case 1: // Nearest
		cfg.Type = "nearest"
	case 2: // Specific
		cfg.Type = "specific"
		cfg.Region, err = dec.ReadString()
	case 3: // MultiRegion
		cfg.Type = "multi_region"
		count, err := dec.ReadVecLen()
		if err != nil {
			return nil, err
		}
		cfg.Regions = make([]string, count)
		for i := range cfg.Regions {
			cfg.Regions[i], err = dec.ReadString()
			if err != nil {
				return nil, err
			}
		}
	case 4: // Custom
		cfg.Type = "custom"
		cfg.Custom, err = dec.ReadString()
	default:
		return nil, fmt.Errorf("unknown exit node variant: %d", variant)
	}

	if err != nil {
		return nil, err
	}
	return cfg, nil
}

// decodeConnected decodes a ConnectedMessage.
func (c *MessageCodec) decodeConnected(dec *BincodeDecoder) (*ConnectedMessage, error) {
	tunnelID, err := dec.ReadString()
	if err != nil {
		return nil, err
	}

	count, err := dec.ReadVecLen()
	if err != nil {
		return nil, err
	}
	endpoints := make([]Endpoint, count)
	for i := range endpoints {
		// Decode Protocol enum (same as ProtocolSpec)
		protocolSpec, err := c.decodeProtocolSpec(dec)
		if err != nil {
			return nil, err
		}

		// Extract protocol type string and port from the spec
		protocol := protocolSpec.Type
		var port uint16
		if protocolSpec.Type == "tcp" || protocolSpec.Type == "tls" {
			port = protocolSpec.Port
		}

		url, err := dec.ReadString()
		if err != nil {
			return nil, err
		}

		// port field is Option<u16>
		optPort, err := dec.ReadOptionU16()
		if err != nil {
			return nil, err
		}
		if optPort != nil {
			port = *optPort
		}

		endpoints[i] = Endpoint{
			Protocol: protocol,
			URL:      url,
			Port:     port,
		}
	}

	return &ConnectedMessage{
		TunnelID:  tunnelID,
		Endpoints: endpoints,
	}, nil
}

// decodeTcpConnect decodes a TcpConnectMessage.
func (c *MessageCodec) decodeTcpConnect(dec *BincodeDecoder) (*TcpConnectMessage, error) {
	streamID, err := dec.ReadU32()
	if err != nil {
		return nil, err
	}
	remoteAddr, err := dec.ReadString()
	if err != nil {
		return nil, err
	}
	remotePort, err := dec.ReadU16()
	if err != nil {
		return nil, err
	}
	return &TcpConnectMessage{
		StreamID:   streamID,
		RemoteAddr: remoteAddr,
		RemotePort: remotePort,
	}, nil
}

// decodeTcpData decodes a TcpDataMessage.
func (c *MessageCodec) decodeTcpData(dec *BincodeDecoder) (*TcpDataMessage, error) {
	streamID, err := dec.ReadU32()
	if err != nil {
		return nil, err
	}
	data, err := dec.ReadBytes()
	if err != nil {
		return nil, err
	}
	return &TcpDataMessage{
		StreamID: streamID,
		Data:     data,
	}, nil
}

// decodeTlsConnect decodes a TlsConnectMessage.
func (c *MessageCodec) decodeTlsConnect(dec *BincodeDecoder) (*TlsConnectMessage, error) {
	streamID, err := dec.ReadU32()
	if err != nil {
		return nil, err
	}
	sni, err := dec.ReadString()
	if err != nil {
		return nil, err
	}
	clientHello, err := dec.ReadBytes()
	if err != nil {
		return nil, err
	}
	return &TlsConnectMessage{
		StreamID:    streamID,
		SNI:         sni,
		ClientHello: clientHello,
	}, nil
}

// decodeTlsData decodes a TlsDataMessage.
func (c *MessageCodec) decodeTlsData(dec *BincodeDecoder) (*TlsDataMessage, error) {
	streamID, err := dec.ReadU32()
	if err != nil {
		return nil, err
	}
	data, err := dec.ReadBytes()
	if err != nil {
		return nil, err
	}
	return &TlsDataMessage{
		StreamID: streamID,
		Data:     data,
	}, nil
}

// decodeHttpRequest decodes an HttpRequestMessage.
func (c *MessageCodec) decodeHttpRequest(dec *BincodeDecoder) (*HttpRequestMessage, error) {
	streamID, err := dec.ReadU32()
	if err != nil {
		return nil, err
	}
	method, err := dec.ReadString()
	if err != nil {
		return nil, err
	}
	uri, err := dec.ReadString()
	if err != nil {
		return nil, err
	}

	headerCount, err := dec.ReadVecLen()
	if err != nil {
		return nil, err
	}
	headers := make(map[string]string, headerCount)
	for i := uint64(0); i < headerCount; i++ {
		key, err := dec.ReadString()
		if err != nil {
			return nil, err
		}
		value, err := dec.ReadString()
		if err != nil {
			return nil, err
		}
		headers[key] = value
	}

	body, err := dec.ReadOptionBytes()
	if err != nil {
		return nil, err
	}

	return &HttpRequestMessage{
		StreamID: streamID,
		Method:   method,
		URI:      uri,
		Headers:  headers,
		Body:     body,
	}, nil
}

// decodeHttpResponse decodes an HttpResponseMessage.
func (c *MessageCodec) decodeHttpResponse(dec *BincodeDecoder) (*HttpResponseMessage, error) {
	streamID, err := dec.ReadU32()
	if err != nil {
		return nil, err
	}
	status, err := dec.ReadU16()
	if err != nil {
		return nil, err
	}

	headerCount, err := dec.ReadVecLen()
	if err != nil {
		return nil, err
	}
	headers := make(map[string]string, headerCount)
	for i := uint64(0); i < headerCount; i++ {
		key, err := dec.ReadString()
		if err != nil {
			return nil, err
		}
		value, err := dec.ReadString()
		if err != nil {
			return nil, err
		}
		headers[key] = value
	}

	body, err := dec.ReadOptionBytes()
	if err != nil {
		return nil, err
	}

	return &HttpResponseMessage{
		StreamID: streamID,
		Status:   status,
		Headers:  headers,
		Body:     body,
	}, nil
}

// decodeHttpChunk decodes an HttpChunkMessage.
func (c *MessageCodec) decodeHttpChunk(dec *BincodeDecoder) (*HttpChunkMessage, error) {
	streamID, err := dec.ReadU32()
	if err != nil {
		return nil, err
	}
	chunk, err := dec.ReadBytes()
	if err != nil {
		return nil, err
	}
	isFinal, err := dec.ReadBool()
	if err != nil {
		return nil, err
	}
	return &HttpChunkMessage{
		StreamID: streamID,
		Chunk:    chunk,
		IsFinal:  isFinal,
	}, nil
}

// decodeHttpStreamConnect decodes an HttpStreamConnectMessage.
func (c *MessageCodec) decodeHttpStreamConnect(dec *BincodeDecoder) (*HttpStreamConnectMessage, error) {
	streamID, err := dec.ReadU32()
	if err != nil {
		return nil, err
	}
	host, err := dec.ReadString()
	if err != nil {
		return nil, err
	}
	initialData, err := dec.ReadBytes()
	if err != nil {
		return nil, err
	}
	return &HttpStreamConnectMessage{
		StreamID:    streamID,
		Host:        host,
		InitialData: initialData,
	}, nil
}

// decodeHttpStreamData decodes an HttpStreamDataMessage.
func (c *MessageCodec) decodeHttpStreamData(dec *BincodeDecoder) (*HttpStreamDataMessage, error) {
	streamID, err := dec.ReadU32()
	if err != nil {
		return nil, err
	}
	data, err := dec.ReadBytes()
	if err != nil {
		return nil, err
	}
	return &HttpStreamDataMessage{
		StreamID: streamID,
		Data:     data,
	}, nil
}

// ErrConnectionClosed is returned when the connection is closed.
var ErrConnectionClosed = errors.New("connection closed")
