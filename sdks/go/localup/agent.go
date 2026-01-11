// Package localup provides a Go SDK for creating tunnels to expose local services
// through the LocalUp relay infrastructure.
//
// Example usage:
//
//	agent, err := localup.NewAgent(localup.WithAuthtoken("your-token"))
//	if err != nil {
//	    log.Fatal(err)
//	}
//
//	ln, err := agent.Forward(ctx,
//	    localup.WithUpstream("http://localhost:8080"),
//	    localup.WithSubdomain("myapp"),
//	)
//	if err != nil {
//	    log.Fatal(err)
//	}
//
//	fmt.Println("Tunnel online:", ln.URL())
//	<-ln.Done()
package localup

import (
	"context"
	"crypto/tls"
	"errors"
	"fmt"
	"sync"
	"time"
)

// Agent manages connections to the LocalUp relay and creates tunnels.
type Agent struct {
	config    *AgentConfig
	mu        sync.RWMutex
	tunnels   map[string]*Tunnel
	transport Transport
}

// AgentConfig holds the configuration for an Agent.
type AgentConfig struct {
	// Authtoken is the JWT authentication token for the relay.
	Authtoken string

	// RelayAddr is the address of the LocalUp relay server.
	// Format: "host:port" (e.g., "relay.localup.io:4443")
	RelayAddr string

	// TLSConfig is optional TLS configuration for the connection.
	TLSConfig *tls.Config

	// Logger is an optional logger for debug output.
	Logger Logger

	// Metadata contains optional key-value pairs sent with tunnel registration.
	Metadata map[string]string

	// Reconnect enables automatic reconnection on connection failure.
	// Default: true
	Reconnect bool

	// ReconnectMaxRetries is the maximum number of reconnection attempts.
	// 0 means unlimited retries. Default: 0 (unlimited)
	ReconnectMaxRetries int

	// ReconnectInitialDelay is the initial delay before the first reconnection attempt.
	// Default: 1 second
	ReconnectInitialDelay time.Duration

	// ReconnectMaxDelay is the maximum delay between reconnection attempts.
	// Default: 30 seconds
	ReconnectMaxDelay time.Duration

	// ReconnectMultiplier is the multiplier for exponential backoff.
	// Default: 2.0
	ReconnectMultiplier float64
}

// AgentOption is a function that configures an AgentConfig.
type AgentOption func(*AgentConfig)

// WithAuthtoken sets the authentication token for the agent.
func WithAuthtoken(token string) AgentOption {
	return func(c *AgentConfig) {
		c.Authtoken = token
	}
}

// WithRelayAddr sets the relay server address.
// Format: "host:port" (e.g., "relay.localup.io:4443")
func WithRelayAddr(addr string) AgentOption {
	return func(c *AgentConfig) {
		c.RelayAddr = addr
	}
}

// WithTLSConfig sets custom TLS configuration.
func WithTLSConfig(tlsConfig *tls.Config) AgentOption {
	return func(c *AgentConfig) {
		c.TLSConfig = tlsConfig
	}
}

// WithLogger sets a custom logger for the agent.
func WithLogger(logger Logger) AgentOption {
	return func(c *AgentConfig) {
		c.Logger = logger
	}
}

// WithMetadata sets metadata key-value pairs for the agent.
func WithMetadata(metadata map[string]string) AgentOption {
	return func(c *AgentConfig) {
		c.Metadata = metadata
	}
}

// WithReconnect enables or disables automatic reconnection.
// Default: true (enabled)
func WithReconnect(enabled bool) AgentOption {
	return func(c *AgentConfig) {
		c.Reconnect = enabled
	}
}

// WithReconnectMaxRetries sets the maximum number of reconnection attempts.
// 0 means unlimited retries.
func WithReconnectMaxRetries(maxRetries int) AgentOption {
	return func(c *AgentConfig) {
		c.ReconnectMaxRetries = maxRetries
	}
}

// WithReconnectBackoff configures the exponential backoff for reconnection.
// initialDelay: delay before first retry (default: 1s)
// maxDelay: maximum delay between retries (default: 30s)
// multiplier: backoff multiplier (default: 2.0)
func WithReconnectBackoff(initialDelay, maxDelay time.Duration, multiplier float64) AgentOption {
	return func(c *AgentConfig) {
		c.ReconnectInitialDelay = initialDelay
		c.ReconnectMaxDelay = maxDelay
		c.ReconnectMultiplier = multiplier
	}
}

// NewAgent creates a new LocalUp agent with the given options.
func NewAgent(opts ...AgentOption) (*Agent, error) {
	config := &AgentConfig{
		RelayAddr:             DefaultRelayAddr,
		Logger:                &noopLogger{},
		Metadata:              make(map[string]string),
		Reconnect:             true, // Enabled by default
		ReconnectMaxRetries:   0,    // Unlimited
		ReconnectInitialDelay: 1 * time.Second,
		ReconnectMaxDelay:     30 * time.Second,
		ReconnectMultiplier:   2.0,
	}

	for _, opt := range opts {
		opt(config)
	}

	if config.Authtoken == "" {
		return nil, errors.New("authtoken is required: use WithAuthtoken option")
	}

	agent := &Agent{
		config:  config,
		tunnels: make(map[string]*Tunnel),
	}

	return agent, nil
}

// Forward creates a new tunnel that forwards traffic to the specified upstream.
// The tunnel is started immediately and traffic forwarding begins.
//
// Example:
//
//	ln, err := agent.Forward(ctx,
//	    localup.WithUpstream("http://localhost:8080"),
//	    localup.WithSubdomain("myapp"),
//	)
func (a *Agent) Forward(ctx context.Context, opts ...TunnelOption) (*Tunnel, error) {
	config := &TunnelConfig{
		Protocol: ProtocolHTTP,
	}

	for _, opt := range opts {
		opt(config)
	}

	if err := config.Validate(); err != nil {
		return nil, fmt.Errorf("invalid tunnel config: %w", err)
	}

	// Create the tunnel
	tunnel, err := a.createTunnel(ctx, config)
	if err != nil {
		return nil, err
	}

	// Store the tunnel
	a.mu.Lock()
	a.tunnels[tunnel.ID()] = tunnel
	a.mu.Unlock()

	return tunnel, nil
}

// Listen creates a tunnel that accepts incoming connections.
// Unlike Forward, you must manually accept and handle connections.
//
// Example:
//
//	ln, err := agent.Listen(ctx,
//	    localup.WithProtocol(localup.ProtocolTCP),
//	    localup.WithPort(0), // auto-assign
//	)
func (a *Agent) Listen(ctx context.Context, opts ...TunnelOption) (*Tunnel, error) {
	config := &TunnelConfig{
		Protocol: ProtocolTCP,
	}

	for _, opt := range opts {
		opt(config)
	}

	// For Listen mode, we don't auto-forward
	config.Upstream = ""

	if err := config.Validate(); err != nil {
		return nil, fmt.Errorf("invalid tunnel config: %w", err)
	}

	tunnel, err := a.createTunnel(ctx, config)
	if err != nil {
		return nil, err
	}

	a.mu.Lock()
	a.tunnels[tunnel.ID()] = tunnel
	a.mu.Unlock()

	return tunnel, nil
}

// Close closes all tunnels and disconnects from the relay.
func (a *Agent) Close() error {
	a.mu.Lock()
	defer a.mu.Unlock()

	var errs []error
	for _, tunnel := range a.tunnels {
		if err := tunnel.Close(); err != nil {
			errs = append(errs, err)
		}
	}
	a.tunnels = make(map[string]*Tunnel)

	if a.transport != nil {
		if err := a.transport.Close(); err != nil {
			errs = append(errs, err)
		}
		a.transport = nil
	}

	if len(errs) > 0 {
		return fmt.Errorf("errors closing agent: %v", errs)
	}
	return nil
}

// createTunnel establishes a tunnel connection to the relay.
func (a *Agent) createTunnel(ctx context.Context, config *TunnelConfig) (*Tunnel, error) {
	// Discover transport if not already connected
	if a.transport == nil {
		transport, err := a.connect(ctx)
		if err != nil {
			return nil, fmt.Errorf("failed to connect to relay: %w", err)
		}
		a.transport = transport
	}

	// Create and register the tunnel
	tunnel := newTunnel(ctx, a, config)

	// Register with the relay
	if err := tunnel.register(ctx); err != nil {
		return nil, fmt.Errorf("failed to register tunnel: %w", err)
	}

	// Start the tunnel's message handler
	go tunnel.run(ctx)

	return tunnel, nil
}

// connect establishes a QUIC connection to the relay server.
func (a *Agent) connect(ctx context.Context) (Transport, error) {
	a.config.Logger.Debug("connecting to relay via QUIC", "addr", a.config.RelayAddr)

	transport, err := NewQUICTransport(ctx, a.config)
	if err != nil {
		return nil, fmt.Errorf("QUIC connection failed: %w", err)
	}

	a.config.Logger.Debug("connected via QUIC", "addr", a.config.RelayAddr)
	return transport, nil
}
