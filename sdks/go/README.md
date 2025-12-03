# LocalUp Go SDK

Go SDK for creating tunnels to expose local services through the LocalUp relay infrastructure.

## Installation

```bash
go get github.com/localup/localup-go
```

## Quick Start

```go
package main

import (
    "context"
    "fmt"
    "log"
    "os"

    "github.com/localup/localup-go"
)

func main() {
    // Create an agent with your auth token
    agent, err := localup.NewAgent(
        localup.WithAuthtoken(os.Getenv("LOCALUP_AUTHTOKEN")),
        localup.WithRelayAddr("relay.localup.io:4443"),
    )
    if err != nil {
        log.Fatal(err)
    }
    defer agent.Close()

    // Create an HTTP tunnel forwarding to localhost:8080
    ln, err := agent.Forward(context.Background(),
        localup.WithUpstream("http://localhost:8080"),
        localup.WithSubdomain("myapp"),
    )
    if err != nil {
        log.Fatal(err)
    }

    fmt.Println("Tunnel online:", ln.URL())

    // Keep running until tunnel closes
    <-ln.Done()
}
```

## API Reference

### Agent

The `Agent` manages connections to the LocalUp relay and creates tunnels.

```go
agent, err := localup.NewAgent(
    localup.WithAuthtoken("your-token"),
    localup.WithRelayAddr("relay.localup.io:4443"),
    localup.WithLogger(localup.NewStdLogger(localup.LogLevelInfo)),
)
```

**Options:**
- `WithAuthtoken(token string)` - JWT authentication token (required)
- `WithRelayAddr(addr string)` - Relay server address (default: `relay.localup.io:4443`)
- `WithTLSConfig(config *tls.Config)` - Custom TLS configuration
- `WithLogger(logger Logger)` - Custom logger
- `WithMetadata(map[string]string)` - Agent metadata

### Creating Tunnels

#### Forward Mode

Creates a tunnel that automatically forwards traffic to a local service:

```go
ln, err := agent.Forward(ctx,
    localup.WithUpstream("http://localhost:8080"),
    localup.WithProtocol(localup.ProtocolHTTP),
    localup.WithSubdomain("myapp"),
)
```

#### Listen Mode

Creates a tunnel where you manually handle connections:

```go
ln, err := agent.Listen(ctx,
    localup.WithProtocol(localup.ProtocolTCP),
    localup.WithPort(0), // auto-assign
)
```

### Tunnel Options

- `WithUpstream(addr string)` - Local address to forward traffic to
- `WithProtocol(protocol Protocol)` - Tunnel protocol (`ProtocolTCP`, `ProtocolTLS`, `ProtocolHTTP`, `ProtocolHTTPS`)
- `WithPort(port uint16)` - Specific port to request (TCP/TLS only, 0 = auto)
- `WithSubdomain(subdomain string)` - Subdomain to request (HTTP/HTTPS only)
- `WithURL(url string)` - Full URL to request (e.g., `https://myapp.localup.io`)
- `WithLocalHTTPS(enabled bool)` - Whether local service uses HTTPS
- `WithTunnelMetadata(map[string]string)` - Tunnel-specific metadata

### Tunnel Methods

```go
// Get the public URL
url := ln.URL()

// Get all endpoints
endpoints := ln.Endpoints()

// Get the tunnel ID
id := ln.ID()

// Get metrics
bytesIn := ln.BytesIn()
bytesOut := ln.BytesOut()

// Wait for tunnel to close
<-ln.Done()

// Close the tunnel
ln.Close()
```

## Protocols

| Protocol | Description | Options |
|----------|-------------|---------|
| `ProtocolTCP` | Raw TCP tunnel with port-based routing | `WithPort(port)` |
| `ProtocolTLS` | TLS tunnel with SNI-based routing (passthrough) | `WithPort(port)` |
| `ProtocolHTTP` | HTTP tunnel with host-based routing | `WithSubdomain(sub)` |
| `ProtocolHTTPS` | HTTPS tunnel with TLS termination | `WithSubdomain(sub)` |

## Examples

### HTTP Tunnel

```go
ln, err := agent.Forward(ctx,
    localup.WithUpstream("http://localhost:3000"),
    localup.WithProtocol(localup.ProtocolHTTP),
    localup.WithSubdomain("api"),
)
// Access at: http://api.localup.io
```

### HTTPS Tunnel

```go
ln, err := agent.Forward(ctx,
    localup.WithUpstream("http://localhost:3000"),
    localup.WithProtocol(localup.ProtocolHTTPS),
    localup.WithSubdomain("secure-api"),
)
// Access at: https://secure-api.localup.io
```

### TCP Tunnel

```go
ln, err := agent.Forward(ctx,
    localup.WithUpstream("localhost:5432"),
    localup.WithProtocol(localup.ProtocolTCP),
    localup.WithPort(0), // auto-assign
)
// Access at: tcp://relay.localup.io:<assigned-port>
```

### TLS Passthrough

```go
ln, err := agent.Forward(ctx,
    localup.WithUpstream("localhost:443"),
    localup.WithProtocol(localup.ProtocolTLS),
    localup.WithLocalHTTPS(true),
)
// TLS is passed through without termination
```

## Logging

The SDK supports pluggable logging:

```go
// Use built-in standard logger
agent, _ := localup.NewAgent(
    localup.WithAuthtoken(token),
    localup.WithLogger(localup.NewStdLogger(localup.LogLevelDebug)),
)

// Or implement your own Logger interface
type Logger interface {
    Debug(msg string, keysAndValues ...interface{})
    Info(msg string, keysAndValues ...interface{})
    Warn(msg string, keysAndValues ...interface{})
    Error(msg string, keysAndValues ...interface{})
}
```

## Error Handling

The SDK uses standard Go error handling:

```go
agent, err := localup.NewAgent(localup.WithAuthtoken(token))
if err != nil {
    // Handle error (e.g., missing token)
}

ln, err := agent.Forward(ctx, localup.WithUpstream("localhost:8080"))
if err != nil {
    // Handle error (e.g., connection failed, auth rejected)
}
```

Common errors:
- `authtoken is required` - Missing authentication token
- `failed to connect to relay` - Network/connection issues
- `registration rejected: <reason>` - Relay rejected the tunnel (e.g., subdomain in use)

## Requirements

- Go 1.22 or later
- Network access to the LocalUp relay (UDP port 4443 for QUIC)

## License

MIT License - See LICENSE file for details.
