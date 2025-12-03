package localup

import (
	"context"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

// Tunnel represents an active tunnel to the LocalUp relay.
type Tunnel struct {
	agent     *Agent
	config    *TunnelConfig
	id        string
	url       string
	endpoints []Endpoint

	// Control stream (Stream 0)
	controlStream Stream
	codec         *MessageCodec

	// State
	ctx       context.Context
	cancel    context.CancelFunc
	done      chan struct{}
	closeOnce sync.Once
	closed    atomic.Bool

	// Stream management
	streams   map[uint32]Stream
	streamsMu sync.RWMutex
	nextID    atomic.Uint32

	// Handlers
	forwarder *httpForwarder

	// Metrics
	bytesIn  atomic.Uint64
	bytesOut atomic.Uint64

	// Reconnection state
	reconnecting   atomic.Bool
	reconnectCount int
}

// newTunnel creates a new tunnel instance.
func newTunnel(ctx context.Context, agent *Agent, config *TunnelConfig) *Tunnel {
	tunnelCtx, cancel := context.WithCancel(ctx)

	t := &Tunnel{
		agent:   agent,
		config:  config,
		id:      generateTunnelID(),
		codec:   NewMessageCodec(),
		ctx:     tunnelCtx,
		cancel:  cancel,
		done:    make(chan struct{}),
		streams: make(map[uint32]Stream),
	}

	// Set up HTTP forwarder if upstream is configured
	if config.Upstream != "" {
		t.forwarder = newHTTPForwarder(config)
	}

	return t
}

// ID returns the tunnel's unique identifier.
func (t *Tunnel) ID() string {
	return t.id
}

// URL returns the public URL for the tunnel.
func (t *Tunnel) URL() string {
	return t.url
}

// Endpoints returns all public endpoints for the tunnel.
func (t *Tunnel) Endpoints() []Endpoint {
	return t.endpoints
}

// Done returns a channel that is closed when the tunnel is closed.
func (t *Tunnel) Done() <-chan struct{} {
	return t.done
}

// Close closes the tunnel.
func (t *Tunnel) Close() error {
	t.closeOnce.Do(func() {
		t.closed.Store(true)
		t.cancel()

		// Send disconnect message
		if t.controlStream != nil {
			msg := &DisconnectMessage{Reason: "client closing"}
			if data, err := t.codec.EncodeMessage(msg); err == nil {
				t.controlStream.Write(data)
			}
			t.controlStream.Close()
		}

		// Close all data streams
		t.streamsMu.Lock()
		for _, stream := range t.streams {
			stream.Close()
		}
		t.streams = make(map[uint32]Stream)
		t.streamsMu.Unlock()

		close(t.done)
	})
	return nil
}

// BytesIn returns the total bytes received.
func (t *Tunnel) BytesIn() uint64 {
	return t.bytesIn.Load()
}

// BytesOut returns the total bytes sent.
func (t *Tunnel) BytesOut() uint64 {
	return t.bytesOut.Load()
}

// register registers the tunnel with the relay.
func (t *Tunnel) register(ctx context.Context) error {
	t.agent.config.Logger.Debug("opening control stream")

	// Open control stream (Stream 0)
	stream, err := t.agent.transport.OpenStream(ctx)
	if err != nil {
		return fmt.Errorf("failed to open control stream: %w", err)
	}
	t.controlStream = stream
	t.agent.config.Logger.Debug("control stream opened")

	// Build Connect message
	protocols := t.buildProtocols()
	config := t.buildTunnelConfig()

	t.agent.config.Logger.Debug("building Connect message",
		"tunnel_id", t.id,
		"protocols", fmt.Sprintf("%+v", protocols),
		"config", fmt.Sprintf("%+v", config))

	connectMsg := &ConnectMessage{
		TunnelID:  t.id,
		AuthToken: t.agent.config.Authtoken,
		Protocols: protocols,
		Config:    config,
	}

	// Send Connect message
	data, err := t.codec.EncodeMessage(connectMsg)
	if err != nil {
		return fmt.Errorf("failed to encode Connect message: %w", err)
	}

	t.agent.config.Logger.Debug("sending Connect message", "bytes", len(data))

	if _, err := t.controlStream.Write(data); err != nil {
		return fmt.Errorf("failed to send Connect message: %w", err)
	}

	t.agent.config.Logger.Debug("sent Connect message", "tunnel_id", t.id)

	// Wait for Connected response
	ctx, cancel := context.WithTimeout(ctx, DefaultRegisterTimeout)
	defer cancel()

	t.agent.config.Logger.Debug("waiting for response...")

	// Read response
	response, err := t.codec.DecodeMessage(t.controlStream)
	if err != nil {
		return fmt.Errorf("failed to read response: %w", err)
	}

	t.agent.config.Logger.Debug("received response", "type", fmt.Sprintf("%T", response))

	switch msg := response.(type) {
	case *ConnectedMessage:
		t.endpoints = msg.Endpoints
		if len(msg.Endpoints) > 0 {
			t.url = msg.Endpoints[0].URL
		}
		t.agent.config.Logger.Info("tunnel connected", "url", t.url, "endpoints", len(t.endpoints))
		return nil

	case *DisconnectMessage:
		return fmt.Errorf("registration rejected: %s", msg.Reason)

	default:
		return fmt.Errorf("unexpected response type: %T", response)
	}
}

// run handles incoming messages and data streams.
func (t *Tunnel) run(ctx context.Context) {
	defer t.Close()

	for {
		// Start control message handler (handles Ping/Pong from server)
		controlDone := make(chan struct{})
		go func() {
			t.handleControlMessages(ctx)
			close(controlDone)
		}()

		// Accept and handle data streams
		disconnected := t.acceptStreams(ctx, controlDone)

		// Check if we should reconnect
		if !disconnected || t.closed.Load() {
			return
		}

		if !t.agent.config.Reconnect {
			t.agent.config.Logger.Info("reconnection disabled, closing tunnel")
			return
		}

		// Attempt reconnection
		if !t.reconnect(ctx) {
			return
		}
	}
}

// acceptStreams accepts and handles data streams until disconnection.
// Returns true if disconnected (should attempt reconnect), false if closed intentionally.
func (t *Tunnel) acceptStreams(ctx context.Context, controlDone <-chan struct{}) bool {
	for {
		select {
		case <-ctx.Done():
			return false
		case <-controlDone:
			// Control stream closed - likely disconnected
			if t.closed.Load() {
				return false
			}
			return true
		default:
		}

		stream, err := t.agent.transport.AcceptStream(ctx)
		if err != nil {
			if t.closed.Load() {
				return false
			}
			// Transport error - likely disconnected
			t.agent.config.Logger.Error("failed to accept stream", "error", err)
			return true
		}

		go t.handleDataStream(ctx, stream)
	}
}

// reconnect attempts to reconnect to the relay with exponential backoff.
// Returns true if reconnection succeeded, false if we should give up.
func (t *Tunnel) reconnect(ctx context.Context) bool {
	if !t.reconnecting.CompareAndSwap(false, true) {
		// Already reconnecting from another goroutine
		return false
	}
	defer t.reconnecting.Store(false)

	config := t.agent.config
	delay := config.ReconnectInitialDelay

	for {
		t.reconnectCount++

		// Check max retries
		if config.ReconnectMaxRetries > 0 && t.reconnectCount > config.ReconnectMaxRetries {
			t.agent.config.Logger.Error("max reconnection attempts reached",
				"attempts", t.reconnectCount-1,
				"max", config.ReconnectMaxRetries)
			return false
		}

		t.agent.config.Logger.Info("attempting to reconnect",
			"attempt", t.reconnectCount,
			"delay", delay)

		// Wait before attempting reconnection
		select {
		case <-ctx.Done():
			return false
		case <-time.After(delay):
		}

		// Close old transport
		if t.agent.transport != nil {
			t.agent.transport.Close()
			t.agent.transport = nil
		}

		// Attempt to connect
		transport, err := t.agent.connect(ctx)
		if err != nil {
			t.agent.config.Logger.Error("reconnection failed", "error", err)

			// Exponential backoff
			delay = time.Duration(float64(delay) * config.ReconnectMultiplier)
			if delay > config.ReconnectMaxDelay {
				delay = config.ReconnectMaxDelay
			}
			continue
		}

		t.agent.transport = transport

		// Re-register the tunnel
		if err := t.register(ctx); err != nil {
			t.agent.config.Logger.Error("re-registration failed", "error", err)

			// Exponential backoff
			delay = time.Duration(float64(delay) * config.ReconnectMultiplier)
			if delay > config.ReconnectMaxDelay {
				delay = config.ReconnectMaxDelay
			}
			continue
		}

		// Reset reconnect count on success
		t.reconnectCount = 0
		t.agent.config.Logger.Info("reconnected successfully", "url", t.url)
		return true
	}
}

// handleControlMessages handles messages on the control stream.
func (t *Tunnel) handleControlMessages(ctx context.Context) {
	for {
		select {
		case <-ctx.Done():
			return
		default:
		}

		msg, err := t.codec.DecodeMessage(t.controlStream)
		if err != nil {
			if err == io.EOF || t.closed.Load() {
				return
			}
			t.agent.config.Logger.Error("failed to decode control message", "error", err)
			return
		}

		switch m := msg.(type) {
		case *PingMessage:
			// Server sends Ping, client responds with Pong
			t.agent.config.Logger.Debug("received Ping", "timestamp", m.Timestamp)
			pong := &PongMessage{Timestamp: m.Timestamp}
			data, err := t.codec.EncodeMessage(pong)
			if err != nil {
				t.agent.config.Logger.Error("failed to encode Pong", "error", err)
				continue
			}
			if _, err := t.controlStream.Write(data); err != nil {
				t.agent.config.Logger.Error("failed to send Pong", "error", err)
				return
			}
			t.agent.config.Logger.Debug("sent Pong", "timestamp", m.Timestamp)

		case *DisconnectMessage:
			t.agent.config.Logger.Info("received Disconnect", "reason", m.Reason)
			t.Close()
			return

		default:
			t.agent.config.Logger.Debug("received control message", "type", fmt.Sprintf("%T", msg))
		}
	}
}

// handleDataStream handles an incoming data stream.
func (t *Tunnel) handleDataStream(ctx context.Context, stream Stream) {
	defer stream.Close()

	// Read the first message to determine the stream type
	msg, err := t.codec.DecodeMessage(stream)
	if err != nil {
		t.agent.config.Logger.Error("failed to decode stream message", "error", err)
		return
	}

	switch m := msg.(type) {
	case *TcpConnectMessage:
		t.handleTCPStream(ctx, stream, m)
	case *HttpRequestMessage:
		t.handleHTTPRequest(ctx, stream, m)
	case *HttpStreamConnectMessage:
		t.handleHTTPStream(ctx, stream, m)
	case *TlsConnectMessage:
		t.handleTLSStream(ctx, stream, m)
	default:
		t.agent.config.Logger.Error("unexpected stream message", "type", fmt.Sprintf("%T", msg))
	}
}

// handleTCPStream handles a TCP data stream.
func (t *Tunnel) handleTCPStream(ctx context.Context, stream Stream, connect *TcpConnectMessage) {
	t.agent.config.Logger.Debug("handling TCP stream",
		"stream_id", connect.StreamID,
		"remote", fmt.Sprintf("%s:%d", connect.RemoteAddr, connect.RemotePort))

	// Connect to local service
	localAddr := net.JoinHostPort(t.config.LocalHost(), fmt.Sprintf("%d", t.config.LocalPort()))
	local, err := net.DialTimeout("tcp", localAddr, DefaultConnectTimeout)
	if err != nil {
		t.agent.config.Logger.Error("failed to connect to local", "addr", localAddr, "error", err)
		// Send close message
		closeMsg := &TcpCloseMessage{StreamID: connect.StreamID}
		if data, err := t.codec.EncodeMessage(closeMsg); err == nil {
			stream.Write(data)
		}
		return
	}
	defer local.Close()

	// Bidirectional copy
	var wg sync.WaitGroup
	wg.Add(2)

	// Stream -> Local
	go func() {
		defer wg.Done()
		t.copyWithCodec(local, stream, connect.StreamID, true)
	}()

	// Local -> Stream
	go func() {
		defer wg.Done()
		t.copyToStream(stream, local, connect.StreamID)
	}()

	wg.Wait()
}

// handleHTTPRequest handles an HTTP request message.
func (t *Tunnel) handleHTTPRequest(ctx context.Context, stream Stream, req *HttpRequestMessage) {
	t.agent.config.Logger.Debug("handling HTTP request",
		"stream_id", req.StreamID,
		"method", req.Method,
		"uri", req.URI)

	if t.forwarder == nil {
		t.sendHTTPError(stream, req.StreamID, http.StatusBadGateway, "no upstream configured")
		return
	}

	resp, err := t.forwarder.forward(ctx, req)
	if err != nil {
		t.agent.config.Logger.Error("failed to forward request", "error", err)
		t.sendHTTPError(stream, req.StreamID, http.StatusBadGateway, err.Error())
		return
	}

	// Send response
	data, err := t.codec.EncodeMessage(resp)
	if err != nil {
		t.agent.config.Logger.Error("failed to encode response", "error", err)
		return
	}

	if _, err := stream.Write(data); err != nil {
		t.agent.config.Logger.Error("failed to send response", "error", err)
	}
}

// handleHTTPStream handles an HTTP stream passthrough.
func (t *Tunnel) handleHTTPStream(ctx context.Context, stream Stream, connect *HttpStreamConnectMessage) {
	t.agent.config.Logger.Debug("handling HTTP stream",
		"stream_id", connect.StreamID,
		"host", connect.Host)

	// Connect to local service
	localAddr := net.JoinHostPort(t.config.LocalHost(), fmt.Sprintf("%d", t.config.LocalPort()))
	local, err := net.DialTimeout("tcp", localAddr, DefaultConnectTimeout)
	if err != nil {
		t.agent.config.Logger.Error("failed to connect to local", "addr", localAddr, "error", err)
		closeMsg := &HttpStreamCloseMessage{StreamID: connect.StreamID}
		if data, err := t.codec.EncodeMessage(closeMsg); err == nil {
			stream.Write(data)
		}
		return
	}
	defer local.Close()

	// Send initial data
	if len(connect.InitialData) > 0 {
		if _, err := local.Write(connect.InitialData); err != nil {
			t.agent.config.Logger.Error("failed to send initial data", "error", err)
			return
		}
	}

	// Bidirectional copy (similar to TCP)
	var wg sync.WaitGroup
	wg.Add(2)

	go func() {
		defer wg.Done()
		t.copyHttpStream(local, stream, connect.StreamID)
	}()

	go func() {
		defer wg.Done()
		t.copyHttpStreamToRemote(stream, local, connect.StreamID)
	}()

	wg.Wait()
}

// handleTLSStream handles a TLS/SNI stream.
func (t *Tunnel) handleTLSStream(ctx context.Context, stream Stream, connect *TlsConnectMessage) {
	t.agent.config.Logger.Debug("handling TLS stream",
		"stream_id", connect.StreamID,
		"sni", connect.SNI)

	// Connect to local service
	localAddr := net.JoinHostPort(t.config.LocalHost(), fmt.Sprintf("%d", t.config.LocalPort()))
	local, err := net.DialTimeout("tcp", localAddr, DefaultConnectTimeout)
	if err != nil {
		t.agent.config.Logger.Error("failed to connect to local", "addr", localAddr, "error", err)
		closeMsg := &TlsCloseMessage{StreamID: connect.StreamID}
		if data, err := t.codec.EncodeMessage(closeMsg); err == nil {
			stream.Write(data)
		}
		return
	}
	defer local.Close()

	// Send the ClientHello first
	if _, err := local.Write(connect.ClientHello); err != nil {
		t.agent.config.Logger.Error("failed to send ClientHello", "error", err)
		return
	}

	// Bidirectional copy
	var wg sync.WaitGroup
	wg.Add(2)

	go func() {
		defer wg.Done()
		t.copyTlsStream(local, stream, connect.StreamID)
	}()

	go func() {
		defer wg.Done()
		t.copyTlsStreamToRemote(stream, local, connect.StreamID)
	}()

	wg.Wait()
}

// Helper methods for stream copying

func (t *Tunnel) copyWithCodec(dst io.Writer, src Stream, streamID uint32, isTcp bool) {
	buf := make([]byte, 32*1024)
	for {
		msg, err := t.codec.DecodeMessage(src)
		if err != nil {
			return
		}

		switch m := msg.(type) {
		case *TcpDataMessage:
			if _, err := dst.Write(m.Data); err != nil {
				return
			}
			t.bytesIn.Add(uint64(len(m.Data)))
		case *TcpCloseMessage:
			return
		default:
			continue
		}
		_ = buf // Silence unused warning
	}
}

func (t *Tunnel) copyToStream(dst Stream, src io.Reader, streamID uint32) {
	buf := make([]byte, 32*1024)
	for {
		n, err := src.Read(buf)
		if n > 0 {
			msg := &TcpDataMessage{StreamID: streamID, Data: buf[:n]}
			data, err := t.codec.EncodeMessage(msg)
			if err != nil {
				return
			}
			if _, err := dst.Write(data); err != nil {
				return
			}
			t.bytesOut.Add(uint64(n))
		}
		if err != nil {
			// Send close message
			closeMsg := &TcpCloseMessage{StreamID: streamID}
			if data, err := t.codec.EncodeMessage(closeMsg); err == nil {
				dst.Write(data)
			}
			return
		}
	}
}

func (t *Tunnel) copyHttpStream(dst io.Writer, src Stream, streamID uint32) {
	for {
		msg, err := t.codec.DecodeMessage(src)
		if err != nil {
			return
		}

		switch m := msg.(type) {
		case *HttpStreamDataMessage:
			if _, err := dst.Write(m.Data); err != nil {
				return
			}
			t.bytesIn.Add(uint64(len(m.Data)))
		case *HttpStreamCloseMessage:
			return
		}
	}
}

func (t *Tunnel) copyHttpStreamToRemote(dst Stream, src io.Reader, streamID uint32) {
	buf := make([]byte, 32*1024)
	for {
		n, err := src.Read(buf)
		if n > 0 {
			msg := &HttpStreamDataMessage{StreamID: streamID, Data: buf[:n]}
			data, err := t.codec.EncodeMessage(msg)
			if err != nil {
				return
			}
			if _, err := dst.Write(data); err != nil {
				return
			}
			t.bytesOut.Add(uint64(n))
		}
		if err != nil {
			closeMsg := &HttpStreamCloseMessage{StreamID: streamID}
			if data, err := t.codec.EncodeMessage(closeMsg); err == nil {
				dst.Write(data)
			}
			return
		}
	}
}

func (t *Tunnel) copyTlsStream(dst io.Writer, src Stream, streamID uint32) {
	for {
		msg, err := t.codec.DecodeMessage(src)
		if err != nil {
			return
		}

		switch m := msg.(type) {
		case *TlsDataMessage:
			if _, err := dst.Write(m.Data); err != nil {
				return
			}
			t.bytesIn.Add(uint64(len(m.Data)))
		case *TlsCloseMessage:
			return
		}
	}
}

func (t *Tunnel) copyTlsStreamToRemote(dst Stream, src io.Reader, streamID uint32) {
	buf := make([]byte, 32*1024)
	for {
		n, err := src.Read(buf)
		if n > 0 {
			msg := &TlsDataMessage{StreamID: streamID, Data: buf[:n]}
			data, err := t.codec.EncodeMessage(msg)
			if err != nil {
				return
			}
			if _, err := dst.Write(data); err != nil {
				return
			}
			t.bytesOut.Add(uint64(n))
		}
		if err != nil {
			closeMsg := &TlsCloseMessage{StreamID: streamID}
			if data, err := t.codec.EncodeMessage(closeMsg); err == nil {
				dst.Write(data)
			}
			return
		}
	}
}

func (t *Tunnel) sendHTTPError(stream Stream, streamID uint32, status int, message string) {
	resp := &HttpResponseMessage{
		StreamID: streamID,
		Status:   uint16(status),
		Headers:  map[string]string{"Content-Type": "text/plain"},
		Body:     []byte(message),
	}
	data, err := t.codec.EncodeMessage(resp)
	if err != nil {
		return
	}
	stream.Write(data)
}

// buildProtocols builds the protocol specifications for the Connect message.
func (t *Tunnel) buildProtocols() []ProtocolSpec {
	var protocols []ProtocolSpec

	switch t.config.Protocol {
	case ProtocolTCP:
		protocols = append(protocols, ProtocolSpec{
			Type: "tcp",
			Port: t.config.Port,
		})
	case ProtocolTLS:
		protocols = append(protocols, ProtocolSpec{
			Type: "tls",
			Port: t.config.Port,
		})
	case ProtocolHTTP:
		var subdomain *string
		if t.config.Subdomain != "" {
			subdomain = &t.config.Subdomain
		}
		protocols = append(protocols, ProtocolSpec{
			Type:      "http",
			Subdomain: subdomain,
		})
	case ProtocolHTTPS:
		var subdomain *string
		if t.config.Subdomain != "" {
			subdomain = &t.config.Subdomain
		}
		protocols = append(protocols, ProtocolSpec{
			Type:      "https",
			Subdomain: subdomain,
		})
	}

	return protocols
}

// buildTunnelConfig builds the tunnel configuration for the Connect message.
func (t *Tunnel) buildTunnelConfig() TunnelConfigMsg {
	var localPort *uint16
	if p := t.config.LocalPort(); p > 0 {
		localPort = &p
	}

	return TunnelConfigMsg{
		LocalHost:          t.config.LocalHost(),
		LocalPort:          localPort,
		LocalHTTPS:         t.config.LocalHTTPS,
		ExitNode:           ExitNodeConfig{Type: "auto"},
		Failover:           false,
		IPAllowlist:        nil,
		EnableCompression:  false,
		EnableMultiplexing: true,
	}
}

// generateTunnelID generates a unique tunnel ID.
func generateTunnelID() string {
	return fmt.Sprintf("tunnel-%d", time.Now().UnixNano())
}

// httpForwarder handles forwarding HTTP requests to the local service.
type httpForwarder struct {
	client    *http.Client
	upstream  *url.URL
	useHTTPS  bool
}

func newHTTPForwarder(config *TunnelConfig) *httpForwarder {
	upstream := config.Upstream
	if !strings.Contains(upstream, "://") {
		if config.LocalHTTPS {
			upstream = "https://" + upstream
		} else {
			upstream = "http://" + upstream
		}
	}

	u, _ := url.Parse(upstream)

	return &httpForwarder{
		client: &http.Client{
			Timeout: 30 * time.Second,
		},
		upstream: u,
		useHTTPS: config.LocalHTTPS,
	}
}

func (f *httpForwarder) forward(ctx context.Context, req *HttpRequestMessage) (*HttpResponseMessage, error) {
	// Build the request URL
	reqURL := *f.upstream
	reqURL.Path = req.URI

	// Create HTTP request
	httpReq, err := http.NewRequestWithContext(ctx, req.Method, reqURL.String(), nil)
	if err != nil {
		return nil, err
	}

	// Copy headers
	for k, v := range req.Headers {
		httpReq.Header.Set(k, v)
	}

	// Set body if present
	if len(req.Body) > 0 {
		httpReq.Body = io.NopCloser(strings.NewReader(string(req.Body)))
		httpReq.ContentLength = int64(len(req.Body))
	}

	// Send request
	resp, err := f.client.Do(httpReq)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	// Read response body
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	// Build response headers
	headers := make(map[string]string)
	for k, v := range resp.Header {
		if len(v) > 0 {
			headers[k] = v[0]
		}
	}

	return &HttpResponseMessage{
		StreamID: req.StreamID,
		Status:   uint16(resp.StatusCode),
		Headers:  headers,
		Body:     body,
	}, nil
}
