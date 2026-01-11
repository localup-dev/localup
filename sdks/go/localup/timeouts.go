package localup

import "time"

// Timeout and keepalive constants
const (
	// DefaultIdleTimeout is the maximum time a connection can be idle.
	DefaultIdleTimeout = 30 * time.Second

	// DefaultKeepAlive is the interval between keepalive pings.
	DefaultKeepAlive = 10 * time.Second

	// DefaultConnectTimeout is the timeout for establishing a connection.
	DefaultConnectTimeout = 10 * time.Second

	// DefaultRegisterTimeout is the timeout for tunnel registration.
	DefaultRegisterTimeout = 5 * time.Second

	// DefaultPingInterval is the interval between ping messages.
	DefaultPingInterval = 15 * time.Second

	// DefaultPingTimeout is the timeout waiting for a pong response.
	DefaultPingTimeout = 5 * time.Second
)
