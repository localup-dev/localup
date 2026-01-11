package localup

import (
	"context"
	"io"
)

// Transport is the interface for a connection to the relay.
type Transport interface {
	// OpenStream opens a new bidirectional stream.
	OpenStream(ctx context.Context) (Stream, error)

	// AcceptStream accepts an incoming stream from the relay.
	AcceptStream(ctx context.Context) (Stream, error)

	// Close closes the transport connection.
	Close() error

	// LocalAddr returns the local address.
	LocalAddr() string

	// RemoteAddr returns the remote address.
	RemoteAddr() string
}

// Stream is a bidirectional stream within a transport.
type Stream interface {
	io.Reader
	io.Writer
	io.Closer

	// StreamID returns the unique identifier for this stream.
	StreamID() uint64

	// CloseWrite closes the write side of the stream.
	CloseWrite() error
}
