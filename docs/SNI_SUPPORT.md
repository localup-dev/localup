# SNI (Server Name Indication) Support

## Overview

Server Name Indication (SNI) allows multiple TLS-encrypted services to be exposed through a single network port. When a client connects, it specifies which hostname it's trying to reach using the TLS SNI extension, and the exit node routes the connection accordingly.

This is useful for:
- Running multiple services on port 443 without exposing them on different ports
- Dynamic routing based on the requested hostname
- Zero-configuration certificate management (when using HTTPS with SNI)

## Architecture

The SNI implementation consists of several layers:

```
┌─────────────────────────────────────────────────────────┐
│                   Client Application                     │
│              (localup-client library)                    │
└────────────────────┬────────────────────────────────────┘
                     │ TLS connections with SNI
                     ▼
┌─────────────────────────────────────────────────────────┐
│              Exit Node (localup-relay)                   │
├─────────────────────────────────────────────────────────┤
│                                                           │
│  TLS Server (localup-server-tls)                         │
│  - Listens on port 443 (configurable)                    │
│  - Accepts incoming TLS connections                      │
│  - Extracts SNI from ClientHello                         │
│                    │                                      │
│                    ▼                                      │
│  SNI Router (localup-router::SniRouter)                  │
│  - Routes by SNI hostname                               │
│  - Maintains mapping: hostname → tunnel ID              │
│                    │                                      │
│                    ▼                                      │
│  Route Registry (localup-router::RouteRegistry)          │
│  - Stores all active routes                             │
│  - Handles concurrent access                            │
│                                                           │
└─────────────────────────────────────────────────────────┘
                     │
                     ├─→ QUIC Tunnel 1 (api.example.com)
                     ├─→ QUIC Tunnel 2 (web.example.com)
                     └─→ QUIC Tunnel 3 (db.example.com)
                              │
                              ▼
                    ┌──────────────────────┐
                    │ Local Services       │
                    │ - API Server 3000    │
                    │ - Web Server 3001    │
                    │ - Database 5432      │
                    └──────────────────────┘
```

## How SNI Routing Works

### 1. TLS ClientHello Parsing

When a client connects to the TLS server, it sends a ClientHello message as part of the TLS handshake. This message includes:

```
TLS Record
├── Type: Handshake (0x16)
├── Version: TLS 1.2 or later
└── Payload
    └── Handshake Message
        ├── Type: ClientHello (0x01)
        └── Extensions
            └── server_name (0x0000)
                └── Host name: "api.example.com"
```

The `SniRouter::extract_sni()` function parses this binary structure to extract the hostname:

```rust
pub fn extract_sni(client_hello: &[u8]) -> Result<String, SniRouterError>
```

It handles:
- TLS record header parsing
- ClientHello version and random data
- Session ID and cipher suites
- Extension list parsing
- SNI extension extraction (type 0x0000)
- Host name decoding

### 2. Route Lookup and Registration

Once extracted, the SNI hostname is used to look up the appropriate tunnel:

```rust
// During tunnel connection registration
let sni_route = SniRoute {
    sni_hostname: "api.example.com".to_string(),
    localup_id: "tunnel-api".to_string(),
    target_addr: "127.0.0.1:3000".to_string(),
};

sni_router.register_route(sni_route)?;

// During incoming TLS connection
let target = sni_router.lookup("api.example.com")?;
// Now we know this connection should go to tunnel-api
```

### 3. Connection Routing

Once the target tunnel is identified:

1. The TLS server receives the ClientHello bytes
2. SNI is extracted from ClientHello
3. Route is looked up in the registry
4. A `TlsConnect` message is sent to the tunnel with:
   - `stream_id`: Unique stream identifier
   - `sni`: The extracted hostname
   - `client_hello`: Raw ClientHello bytes

The tunnel client then:
1. Accepts the TlsConnect message
2. Establishes connection to local TLS service
3. Forwards the ClientHello bytes
4. Proxies all subsequent data bidirectionally

## Client Usage

### Configure SNI in Tunnel

```rust
use localup_client::{ProtocolConfig, TunnelConfig};

let config = TunnelConfig {
    local_host: "127.0.0.1".to_string(),
    protocols: vec![
        ProtocolConfig::Tls {
            local_port: 3443,  // Local TLS service port
            sni_hostname: Some("api.example.com".to_string()),
            remote_port: Some(443),
        },
    ],
    auth_token: "your-token".to_string(),
    exit_node: ExitNodeConfig::Auto,
    failover: true,
    connection_timeout: Duration::from_secs(30),
};

let client = TunnelClient::connect(config).await?;
```

### Using CLI

```bash
# Expose local TLS service on port 443 via SNI
localup-relay --tls-addr 0.0.0.0:443

# Client connects and registers SNI route
localup --port 3443 --protocol tls --subdomain api.example.com
```

## Exit Node Configuration

### Start Exit Node with SNI Server

```bash
localup-relay \
  --tls-addr 0.0.0.0:443 \
  --domain example.com
```

This makes the TLS server listen on port 443 and routes based on:
- Exact hostname match: `api.example.com` → tunnel 1
- Wildcard support planned: `*.api.example.com` → tunnel 1

### Command Line Options

```
--tls-addr <ADDRESS>
  Enable TLS/SNI routing server on specified address
  Format: "0.0.0.0:443" or "localhost:8443"
  Optional: if not specified, TLS server is disabled
```

## Protocol Integration

### TunnelMessage Types

The protocol supports TLS tunneling through these message types:

```rust
pub enum TunnelMessage {
    TlsConnect {
        stream_id: u32,
        sni: String,
        client_hello: Vec<u8>,
    },
    TlsData {
        stream_id: u32,
        data: Vec<u8>,
    },
    TlsClose {
        stream_id: u32,
    },
}
```

### Protocol Flow

```
1. Client connects to relay on port 443
2. Client sends ClientHello (includes SNI)
3. TlsServer extracts SNI from ClientHello
4. TlsServer looks up route: "api.example.com" → tunnel-api
5. TlsServer sends TlsConnect{sni, client_hello} to tunnel
6. Client receives TlsConnect and connects to local service
7. Client sends ClientHello bytes to local TLS service
8. Bidirectional TLS data forwarding via TlsData messages
9. Either side initiates TlsClose to end session
```

## Implementation Details

### SNI Extraction Algorithm

The SNI extraction follows the TLS 1.3 specification (RFC 8446):

1. Parse record header (type, version, length)
2. Skip handshake header
3. Skip ClientHello fixed fields (version, random, session_id)
4. Skip cipher suites list
5. Skip compression methods list
6. Parse extensions:
   - Find extension with type 0x0000 (server_name)
   - Extract server_name_list
   - Get first entry (host_name type 0x00)
   - Decode hostname as UTF-8

**Buffer Safety:** All parsing includes bounds checking to prevent buffer overruns.

### Router Implementation

The `RouteRegistry` is thread-safe using `DashMap`:

```rust
pub struct RouteRegistry {
    routes: DashMap<RouteKey, RouteTarget>,
}
```

This allows:
- Concurrent read access (many tunnels can be looked up simultaneously)
- Safe concurrent mutations (new tunnels can register while others are routing)
- No locks needed for lookups

### Error Handling

The SNI system gracefully handles errors:

- **Malformed ClientHello**: Returns `SniExtractionFailed`
- **No SNI extension**: Uses fallback SNI if provided
- **Route not found**: Returns `NoRoute` error with hostname
- **Invalid hostname**: Returns `InvalidSni` error

## Testing

The SNI implementation includes comprehensive tests:

### Unit Tests

```bash
cargo test -p localup-router sni::tests
```

Tests cover:
- SNI extraction from valid ClientHello
- SNI extraction from ClientHello without SNI extension
- Malformed ClientHello handling
- SNI route registration and lookup
- Wildcard SNI patterns (exact match for now)

### Integration Tests

TLS server tests verify:
- Server creation with SNI router
- Route registration before connections
- SNI-based routing in connection handling

## Performance Considerations

### SNI Extraction

- **Single pass:** One linear scan through ClientHello
- **No allocations:** Uses stack-based parsing
- **Early termination:** Stops after finding SNI extension
- **Typical time:** < 100 microseconds

### Route Lookup

- **O(1) expected:** Hash map lookup
- **Thread-safe:** Lock-free reads via DashMap
- **No cloning:** Direct reference to route target
- **Typical time:** < 1 microsecond

### Overall

- Negligible overhead compared to TLS handshake time
- No impact on data forwarding performance
- Scales linearly with number of routes

## Security Considerations

### SNI Leaks Hostname

SNI is sent in cleartext as part of the TLS handshake. The hostname is visible to:
- Network observers
- ISPs
- Any intermediate proxies

This is inherent to SNI and not a limitation of this implementation.

### Certificate Validation

When using SNI with automatic HTTPS (ACME), ensure:
1. DNS points all SNI hostnames to the relay server
2. ACME validation succeeds for each hostname
3. Certificates are automatically renewed before expiration

### Access Control

Routes are registered when tunnels connect with valid JWT tokens. The relay ensures:
- Only authenticated clients can register routes
- Each client gets a unique tunnel ID
- Routes cannot be accessed without active tunnel connection

## Future Enhancements

### Wildcard Hostname Matching

Currently supports exact matches. Future versions could add:
- `*.example.com` matches all subdomains
- `api.*.example.com` matches variable parts

### Custom SNI Validation

Allow custom validation logic:
- Whitelist specific hostnames
- Reject based on patterns
- Rate limiting per hostname

### SNI-based Rate Limiting

Different rate limits per hostname:
- Public APIs: high limits
- Internal services: low limits
- Blocked: zero limits

### TLS Version Support

Extend to support earlier TLS versions:
- TLS 1.2: Already works (ClientHello format compatible)
- TLS 1.1: Possible with format differences
- SSL 3.0: Deprecated, not planned

## Troubleshooting

### "No route found for SNI: api.example.com"

**Cause:** The tunnel hasn't registered this SNI hostname yet.

**Solution:**
1. Start tunnel client with correct `sni_hostname`
2. Check tunnel client logs for connection errors
3. Verify relay and client can reach each other

### "SNI extraction failed"

**Cause:** ClientHello doesn't include SNI extension, or is malformed.

**Solution:**
1. Verify TLS client supports SNI (most modern clients do)
2. Check network path isn't corrupting data
3. Try with a different TLS client for debugging

### "Certificate verification failed"

**Cause:** Certificate hostname doesn't match SNI hostname.

**Solution:**
1. Use certificates that cover all SNI hostnames
2. Use wildcard certificates for subdomains
3. Or disable certificate verification (dev only)

## References

- [RFC 8446: TLS 1.3 Specification](https://tools.ietf.org/html/rfc8446)
- [Server Name Indication](https://en.wikipedia.org/wiki/Server_Name_Indication)
- [TLS ClientHello Structure](https://tools.ietf.org/html/rfc5246#section-7.4.1.2)
