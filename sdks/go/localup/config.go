package localup

import (
	"errors"
	"net/url"
	"strings"
)

// TunnelConfig holds the configuration for a tunnel.
type TunnelConfig struct {
	// Protocol specifies the tunnel protocol (tcp, tls, http, https).
	Protocol Protocol

	// Upstream is the local address to forward traffic to.
	// Format: "http://localhost:8080" or "localhost:8080"
	Upstream string

	// Port is the specific port to request (TCP/TLS only).
	// 0 means auto-assign.
	Port uint16

	// Subdomain is the subdomain to request (HTTP/HTTPS only).
	// Empty means auto-assign.
	Subdomain string

	// URL is the full URL to request (e.g., "https://myapp.localup.io").
	// Takes precedence over Subdomain if set.
	URL string

	// LocalHTTPS indicates if the local upstream uses HTTPS.
	LocalHTTPS bool

	// Metadata contains optional key-value pairs for this tunnel.
	Metadata map[string]string
}

// TunnelOption is a function that configures a TunnelConfig.
type TunnelOption func(*TunnelConfig)

// WithUpstream sets the upstream address to forward traffic to.
// Format: "http://localhost:8080" or just "localhost:8080"
func WithUpstream(addr string) TunnelOption {
	return func(c *TunnelConfig) {
		c.Upstream = addr

		// Detect if upstream is HTTPS
		if strings.HasPrefix(addr, "https://") {
			c.LocalHTTPS = true
		}
	}
}

// WithProtocol sets the tunnel protocol.
func WithProtocol(protocol Protocol) TunnelOption {
	return func(c *TunnelConfig) {
		c.Protocol = protocol
	}
}

// WithPort sets the specific port to request (TCP/TLS only).
func WithPort(port uint16) TunnelOption {
	return func(c *TunnelConfig) {
		c.Port = port
	}
}

// WithSubdomain sets the subdomain to request (HTTP/HTTPS only).
func WithSubdomain(subdomain string) TunnelOption {
	return func(c *TunnelConfig) {
		c.Subdomain = subdomain
	}
}

// WithURL sets the full URL to request.
// Example: "https://myapp.localup.io"
func WithURL(urlStr string) TunnelOption {
	return func(c *TunnelConfig) {
		c.URL = urlStr

		// Parse URL to extract subdomain and protocol
		if u, err := url.Parse(urlStr); err == nil {
			// Determine protocol from scheme
			switch u.Scheme {
			case "http":
				c.Protocol = ProtocolHTTP
			case "https":
				c.Protocol = ProtocolHTTPS
			case "tcp":
				c.Protocol = ProtocolTCP
			case "tls":
				c.Protocol = ProtocolTLS
			}

			// Extract subdomain from host
			parts := strings.Split(u.Hostname(), ".")
			if len(parts) > 2 {
				c.Subdomain = parts[0]
			}
		}
	}
}

// WithLocalHTTPS indicates that the local upstream uses HTTPS.
func WithLocalHTTPS(enabled bool) TunnelOption {
	return func(c *TunnelConfig) {
		c.LocalHTTPS = enabled
	}
}

// WithTunnelMetadata sets metadata for this specific tunnel.
func WithTunnelMetadata(metadata map[string]string) TunnelOption {
	return func(c *TunnelConfig) {
		c.Metadata = metadata
	}
}

// Validate checks if the tunnel configuration is valid.
func (c *TunnelConfig) Validate() error {
	switch c.Protocol {
	case ProtocolTCP, ProtocolTLS:
		// Port-based protocols - upstream is optional for Listen mode
	case ProtocolHTTP, ProtocolHTTPS:
		// HTTP-based protocols - upstream is required for Forward mode
		// but optional for Listen mode
	case "":
		return errors.New("protocol is required")
	default:
		return errors.New("unknown protocol: " + string(c.Protocol))
	}

	return nil
}

// LocalHost returns the host portion of the upstream address.
func (c *TunnelConfig) LocalHost() string {
	if c.Upstream == "" {
		return "localhost"
	}

	// Parse as URL
	upstream := c.Upstream
	if !strings.Contains(upstream, "://") {
		upstream = "http://" + upstream
	}

	u, err := url.Parse(upstream)
	if err != nil {
		return "localhost"
	}

	host := u.Hostname()
	if host == "" {
		return "localhost"
	}
	return host
}

// LocalPort returns the port portion of the upstream address.
func (c *TunnelConfig) LocalPort() uint16 {
	if c.Upstream == "" {
		return 0
	}

	// Parse as URL
	upstream := c.Upstream
	if !strings.Contains(upstream, "://") {
		upstream = "http://" + upstream
	}

	u, err := url.Parse(upstream)
	if err != nil {
		return 0
	}

	port := u.Port()
	if port == "" {
		// Default ports
		if u.Scheme == "https" {
			return 443
		}
		return 80
	}

	// Parse port
	var p uint16
	for _, ch := range port {
		p = p*10 + uint16(ch-'0')
	}
	return p
}
