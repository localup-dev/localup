# Transport Protocols

localup supports three transport protocols for the connection between client and relay. All transports provide encryption and multiplexing, but differ in performance, compatibility, and firewall traversal.

---

## Overview

| | QUIC | WebSocket | HTTP/2 |
|---|---|---|---|
| **Protocol** | UDP | TCP (wss://) | TCP (h2) |
| **Default Port** | 4443/UDP | 443/TCP | 443/TCP |
| **Encryption** | TLS 1.3 (built-in) | TLS 1.2/1.3 | TLS 1.2/1.3 |
| **Multiplexing** | Native | Custom framing | Native (HTTP/2 streams) |
| **Firewall** | May be blocked | Passes all firewalls | Passes all firewalls |
| **Performance** | Best | Good | Good |
| **Head-of-line blocking** | No (per-stream) | Yes (TCP) | Yes (TCP) |
| **0-RTT reconnection** | Yes | No | No |
| **Best for** | Default / mobile | Corporate networks | Mixed HTTP traffic |

---

## QUIC (Default)

QUIC is the default and recommended transport. It provides the best performance with native multiplexing, 0-RTT reconnection, and per-stream flow control.

### When to Use

- Default choice for most deployments
- Mobile or unreliable networks (handles network switching)
- Low-latency requirements
- High-throughput scenarios

### When to Avoid

- Corporate firewalls blocking UDP
- Networks that only allow TCP port 443
- Strict proxy environments

### Usage

```bash
# Explicit (same as default)
localup -p 3000 --transport quic -r relay.example.com:4443

# Relay-side
localup relay http --localup-addr 0.0.0.0:4443 --transport quic
```

---

## WebSocket

WebSocket runs over TCP port 443, making it compatible with virtually all networks and firewalls. It uses a custom framing protocol for stream multiplexing on top of a single WebSocket connection.

### When to Use

- Corporate networks with restrictive firewalls
- Environments that block UDP
- Behind HTTP proxies
- When maximum compatibility is needed

### When to Avoid

- When lowest latency is critical (TCP overhead)
- When you need per-stream flow control

### Usage

```bash
# Client
localup -p 3000 --transport websocket -r relay.example.com:443

# Relay-side
localup relay http \
  --localup-addr 0.0.0.0:4443 \
  --transport websocket \
  --websocket-path /localup    # Default endpoint path
```

The WebSocket endpoint is served at the path configured with `--websocket-path` (default: `/localup`).

---

## HTTP/2 (H2)

HTTP/2 uses native HTTP/2 streams for multiplexing, running over TCP port 443. It's a standard protocol universally supported by CDNs, proxies, and load balancers.

### When to Use

- Environments with HTTP/2-aware proxies or CDNs
- When mixing tunnel traffic with HTTP services
- Standard protocol compliance requirements
- Behind load balancers that speak HTTP/2

### When to Avoid

- When lowest latency is critical (TCP head-of-line blocking)
- Simple deployments where QUIC works fine

### Usage

```bash
# Client
localup -p 3000 --transport h2 -r relay.example.com:443

# Relay-side
localup relay http --localup-addr 0.0.0.0:4443 --transport h2
```

---

## Auto-Discovery

When no `--transport` flag is provided, the client can auto-discover available transports by querying the relay's well-known endpoint:

```
GET /.well-known/localup-protocols
```

The response lists available transports with their configuration:

```json
{
  "quic": { "port": 4443 },
  "websocket": { "port": 443, "path": "/localup" },
  "h2": { "port": 443 }
}
```

The client selects the best available transport automatically, preferring QUIC when available.

---

## Configuration

### In `.localup.yml`

Set a default transport or override per-tunnel:

```yaml
defaults:
  transport: quic            # Default for all tunnels

tunnels:
  - name: web
    port: 3000
    transport: websocket     # Override for this tunnel (behind firewall)

  - name: api
    port: 8080               # Uses default (quic)
```

### Via Environment

The transport can also be influenced by the relay address. If the relay URL includes a scheme:

- `wss://relay.example.com` implies WebSocket
- `https://relay.example.com` implies H2
- `relay.example.com:4443` (no scheme) implies QUIC

---

## Troubleshooting

### "Connection refused" or "Connection timed out"

Your network may be blocking the transport's port/protocol.

```bash
# Try WebSocket (most compatible)
localup -p 3000 --transport websocket -r relay.example.com:443

# Enable debug logging to see transport negotiation
localup -p 3000 --log-level debug -r relay.example.com:4443
```

### Slow performance

If tunnels feel slow:

1. Check if you're using QUIC (best performance)
2. For TCP-based transports (WebSocket, H2), latency is higher due to TCP head-of-line blocking
3. Try switching transports to see if performance improves

### Transport mismatch

The client and relay must support the same transport. If the relay only has QUIC configured but the client requests WebSocket, the connection will fail.

Check available transports:

```bash
curl https://relay.example.com/.well-known/localup-protocols
```
