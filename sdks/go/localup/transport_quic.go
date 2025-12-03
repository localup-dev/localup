package localup

import (
	"context"
	"crypto/tls"
	"fmt"
	"net"

	"github.com/quic-go/quic-go"
)

// QUICTransport implements Transport using QUIC.
type QUICTransport struct {
	conn       quic.Connection
	localAddr  string
	remoteAddr string
}

// NewQUICTransport creates a new QUIC transport to the relay.
func NewQUICTransport(ctx context.Context, config *AgentConfig) (*QUICTransport, error) {
	// Parse the relay address
	host, port, err := net.SplitHostPort(config.RelayAddr)
	if err != nil {
		// No port in address, use default QUIC port
		host = config.RelayAddr
		port = fmt.Sprintf("%d", DefaultQUICPort)
	}

	addr := net.JoinHostPort(host, port)

	// Resolve the address
	udpAddr, err := net.ResolveUDPAddr("udp", addr)
	if err != nil {
		return nil, fmt.Errorf("failed to resolve address %s: %w", addr, err)
	}

	// Create UDP connection
	udpConn, err := net.ListenUDP("udp", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to create UDP socket: %w", err)
	}

	// TLS configuration
	tlsConfig := config.TLSConfig
	if tlsConfig == nil {
		tlsConfig = &tls.Config{
			InsecureSkipVerify: true, // TODO: proper certificate verification
			NextProtos:         []string{"localup-v1"},
		}
	} else {
		// Clone and set ALPN
		tlsConfig = tlsConfig.Clone()
		if len(tlsConfig.NextProtos) == 0 {
			tlsConfig.NextProtos = []string{"localup-v1"}
		}
	}

	// Set server name for SNI
	if tlsConfig.ServerName == "" {
		tlsConfig.ServerName = host
	}

	// QUIC configuration
	quicConfig := &quic.Config{
		MaxIdleTimeout:  DefaultIdleTimeout,
		KeepAlivePeriod: DefaultKeepAlive,
	}

	// Dial the relay
	conn, err := quic.Dial(ctx, udpConn, udpAddr, tlsConfig, quicConfig)
	if err != nil {
		udpConn.Close()
		return nil, fmt.Errorf("failed to connect to relay: %w", err)
	}

	return &QUICTransport{
		conn:       conn,
		localAddr:  udpConn.LocalAddr().String(),
		remoteAddr: addr,
	}, nil
}

// OpenStream opens a new bidirectional stream.
func (t *QUICTransport) OpenStream(ctx context.Context) (Stream, error) {
	stream, err := t.conn.OpenStreamSync(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to open stream: %w", err)
	}
	return &QUICStream{stream: stream}, nil
}

// AcceptStream accepts an incoming stream from the relay.
func (t *QUICTransport) AcceptStream(ctx context.Context) (Stream, error) {
	stream, err := t.conn.AcceptStream(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to accept stream: %w", err)
	}
	return &QUICStream{stream: stream}, nil
}

// Close closes the transport connection.
func (t *QUICTransport) Close() error {
	return t.conn.CloseWithError(0, "closing")
}

// LocalAddr returns the local address.
func (t *QUICTransport) LocalAddr() string {
	return t.localAddr
}

// RemoteAddr returns the remote address.
func (t *QUICTransport) RemoteAddr() string {
	return t.remoteAddr
}

// QUICStream wraps a QUIC stream.
type QUICStream struct {
	stream quic.Stream
}

// Read reads data from the stream.
func (s *QUICStream) Read(p []byte) (int, error) {
	return s.stream.Read(p)
}

// Write writes data to the stream.
func (s *QUICStream) Write(p []byte) (int, error) {
	return s.stream.Write(p)
}

// Close closes the stream.
func (s *QUICStream) Close() error {
	return s.stream.Close()
}

// StreamID returns the stream ID.
func (s *QUICStream) StreamID() uint64 {
	return uint64(s.stream.StreamID())
}

// CloseWrite closes the write side of the stream.
func (s *QUICStream) CloseWrite() error {
	s.stream.CancelWrite(0)
	return nil
}
