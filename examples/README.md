# Localup Library Examples

This directory contains practical examples demonstrating how to use the `localup-lib` library for creating tunnel relays and clients.

## Getting Started

### Prerequisites

Make sure you have:
- Rust 1.90+ ([install](https://rustup.rs/))
- `openssl` and `curl` for testing (optional but recommended)

### Quick Start

1. **Navigate to the project root**:
   ```bash
   cd /path/to/localup
   ```

2. **List available examples**:
   ```bash
   cargo run --example 2>&1 | grep "example\|Compiling"
   ```

3. **Run an example** (in one terminal):
   ```bash
   # Start the relay
   cargo run --example https_relay
   # or
   cargo run --example tcp_relay
   # or
   cargo run --example tls_relay
   ```

4. **Test the relay** (in another terminal):
   ```bash
   # Follow instructions printed by the running example
   ```

---

## Examples

### 1. HTTPS Relay (`https_relay.rs`)

A complete example showing how to:
- Generate self-signed TLS certificates
- Create an HTTPS relay server
- Register tunnel routes with host-based routing
- Set up a tunnel client connection

**Features:**
- Auto-generated self-signed certificates (no manual setup)
- HTTPS termination at the relay
- Host-based routing (`Host` header)
- Local echo server for testing

**How to Run:**

Terminal 1 - Start the relay:
```bash
$ cargo run --example https_relay

🚀 HTTPS Relay Example
======================

📝 Step 1: Generating self-signed certificates...
✅ Certificates generated: cert.pem, key.pem

📝 Step 2: Starting HTTPS relay on localhost:8443...
✅ HTTPS relay started

📝 Step 3: Starting local HTTP server on localhost:3000...
✅ Local HTTP server started

📝 Step 4: Registering tunnel route...
✅ Route registered: localho.st:8443 → 127.0.0.1:3000

🧪 Testing the tunnel:
======================
In another terminal, test the tunnel with:
  curl -k https://localho.st:8443/myapp

Expected response:
  ✅ Hello from local server! (via HTTPS relay)

Note: localho.st is a convenient domain for local development that resolves to 127.0.0.1

Press Ctrl+C to stop the relay...
```

Terminal 2 - Test with curl:
```bash
$ curl -k https://localho.st:8443/myapp
✅ Hello from local server! (via HTTPS relay)
```

**What Happens:**
1. Relay generates self-signed certificates (`cert.pem`, `key.pem`)
2. Local HTTP server starts on port 3000 (simulating the user's app)
3. HTTPS relay starts on port 8443 (accepts public HTTPS connections)
4. Route is registered (mapping `localho.st:8443` Host header to local server)
5. When curl connects to `localho.st:8443`, the relay routes it to the local server
6. Client requests are HTTPS encrypted at relay, then forwarded as HTTP to local server
7. `localho.st` resolves to 127.0.0.1 automatically (no /etc/hosts needed)

**Architecture:**
```
Public Client (curl)
        ↓
HTTPS Relay (8443) ← Host Header: localho.st:8443
        ↓
Route Registry (matches localho.st → 127.0.0.1:3000)
        ↓
Local HTTP Server (3000) - Your app
```

**Key Points:**
- This demonstrates **relay server setup**: generating certs, configuring routes, accepting HTTPS connections
- Routes are registered directly via `RouteRegistry` (in production, registered by TunnelClient via control plane)
- The relay handles HTTPS termination and host-based routing based on Host header
- For a **complete system** with client support, see `crates/localup-exit-node` or run the exit node binary:
  ```bash
  cargo run -p localup-exit-node -- --domain localhost
  ```
- The exit node provides:
  - QUIC-based control plane for client registration
  - Route registration from tunnel clients
  - Multi-protocol support (HTTP, HTTPS, TLS/SNI, TCP)
  - Complete tunnel lifecycle management

**Use Case:**
- HTTPS services with automatic certificate management
- Multiple HTTPS services on different subdomains
- Internal staging environments
- Development/testing

---

### 2. TCP Relay (`tcp_relay.rs`)

A complete example showing how to:
- Create a raw TCP relay server
- Create a local TCP echo server
- Register tunnel routes for port-based routing
- Set up bidirectional TCP communication

**Features:**
- Raw TCP tunneling (no protocol-specific handling)
- Port-based routing
- TCP echo server for testing
- Bidirectional data forwarding

**How to Run:**

Terminal 1 - Start the relay:
```bash
$ cargo run --example tcp_relay

🚀 TCP Relay Example
====================

📝 Step 1: Starting local TCP echo server on localhost:5000...
✅ Echo server started

📝 Step 2: Starting TCP relay on localhost:10000-10010...
✅ TCP relay started

📝 Step 3: Registering tunnel route...
✅ Route would be registered: port 10000 → 127.0.0.1:5000

📝 Step 4: How the tunnel client would work...
In your Rust application:
...

🧪 Testing the tunnel:
======================
In another terminal, connect to the relay with:
  nc localhost 10000  # or: telnet localhost 10000

Type any message, and it will be echoed back:
  > Hello
  Hello

The tunnel routes the connection through the relay to your local echo server.

Press Ctrl+C to stop the relay...
```

Terminal 2 - Test with netcat:
```bash
$ nc localhost 10000
Hello
Hello
TCP Test Message
TCP Test Message
^C
```

Or with telnet:
```bash
$ telnet localhost 10000
Trying 127.0.0.1...
Connected to localhost.
Escape character is '^]'.
Hello
Hello
Test
Test
^]
quit
Connection closed.
```

Terminal 1 - Shows connection logs:
```
[Echo] Received: Hello
[Echo] Received: TCP Test Message
[Echo] Client 127.0.0.1:54321 disconnected
```

**What Happens:**
1. Local echo server starts on port 5000 (echoes back all input)
2. TCP relay starts on port 10000
3. Client connections on port 10000 are forwarded to the echo server
4. All data is bidirectionally forwarded through the relay
5. When client disconnects, connection is closed

**Use Case:**
- Database tunneling (PostgreSQL, MySQL, Redis, etc.)
- SSH port forwarding
- Custom TCP protocols
- Legacy services behind NAT
- Any raw TCP service

---

### 3. TLS/SNI Relay (`tls_relay.rs`)

A complete example showing how to:
- Generate self-signed certificates for multiple hostnames
- Create a TLS/SNI relay server with passthrough mode
- Register tunnel routes with SNI-based routing
- Route multiple TLS services on the same port (443)

**Features:**
- Auto-generated certificates for multiple hostnames
- SNI extraction from TLS ClientHello
- TLS passthrough (no decryption at relay)
- Multi-tenant support (multiple services on port 443)
- End-to-end encryption

**How to Run:**

Terminal 1 - Start the relay:
```bash
$ cargo run --example tls_relay

🚀 TLS/SNI Relay Example
========================

📝 Step 1: Generating self-signed certificates for SNI relay...
  → Generating relay certificate...
✅ Relay certificate generated:
   - relay_cert.pem, relay_key.pem

⚠️  Note: For SNI routing, clients use their own certificates.
    The relay certificate is only for accepting connections.

📝 Step 2: Setting up route registry...
✅ Route registry created

📝 Step 3: Starting local TLS services...
  → Starting API service on localhost:3443...
  → Starting DB service on localhost:4443...
✅ Local TLS services started

📝 Step 4: Starting TLS/SNI relay on localhost:443...
✅ TLS/SNI relay started

📝 Step 5: Registering tunnel routes with SNI hostnames...
✅ Route registered: api.example.com → 127.0.0.1:3443
✅ Route registered: db.example.com → 127.0.0.1:4443

🧪 Testing the TLS/SNI relay:
=============================
In another terminal, test SNI routing with:

  # Test API service (routes to localhost:3443)
  openssl s_client -connect localhost:443 -servername api.example.com </dev/null

  # Test DB service (routes to localhost:4443)
  openssl s_client -connect localhost:443 -servername db.example.com </dev/null

The relay extracts the SNI hostname from the TLS ClientHello
and routes to the appropriate backend service.

🔒 Security note:
  - The relay never decrypts TLS traffic (passthrough mode)
  - Each service keeps its own certificates
  - End-to-end encryption is maintained

Press Ctrl+C to stop the relay...
```

Terminal 2 - Test API service:
```bash
$ openssl s_client -connect localhost:443 -servername api.example.com </dev/null
...
[TLS connection details showing api.example.com certificate]
...
```

Terminal 3 - Test DB service:
```bash
$ openssl s_client -connect localhost:443 -servername db.example.com </dev/null
...
[TLS connection details showing db.example.com certificate]
...
```

Terminal 1 - Shows routing logs:
```
[API] Connection from 127.0.0.1:54321 (TLS handshake detected)
[DB] Connection from 127.0.0.1:54322 (TLS handshake detected)
```

**What Happens:**
1. Relay generates certificate for accepting connections
2. Two local TLS services start on ports 3443 and 4443
3. TLS/SNI relay starts on port 443
4. Routes registered:
   - `api.example.com` → 127.0.0.1:3443
   - `db.example.com` → 127.0.0.1:4444
5. Client connects with SNI hostname in ClientHello
6. Relay extracts SNI hostname and routes to appropriate service
7. No TLS decryption happens at relay (passthrough mode)

**Advanced: Multi-Tenant on Same Port**

With SNI routing, you can run unlimited services on the same port (443):

```
Client 1: api.example.com:443 → Relay → 127.0.0.1:3443
Client 2: db.example.com:443  → Relay → 127.0.0.1:4443
Client 3: cache.example.com:443 → Relay → 127.0.0.1:5443
...
```

Each client is routed based on the SNI hostname in their TLS ClientHello.

**Use Case:**
- Multiple TLS services on one relay
- End-to-end encrypted tunnels
- SNI-based routing (no decryption at relay)
- API gateways with TLS passthrough
- SSL/TLS-based databases (PostgreSQL, MySQL with SSL)
- Multi-tenant applications
- Microservices architectures

---

## Architecture Diagrams

### HTTPS Relay Flow
```
┌─────────────────┐
│  Public Client  │
│  (HTTPS Port    │
│   8443)         │
└────────┬────────┘
         │
         ▼
┌─────────────────────────────┐
│  HTTPS Relay Server         │
│  - TLS Termination          │
│  - Host-based routing       │
│  - Auto-generated certs     │
└────────┬────────────────────┘
         │
         ▼
┌─────────────────────────────┐
│  Local HTTP Server          │
│  (Port 3000)                │
│  (plaintext inside network) │
└─────────────────────────────┘
```

### TCP Relay Flow
```
┌─────────────────┐
│  Public Client  │
│  (Port 10000)   │
└────────┬────────┘
         │
         ▼
┌─────────────────────────────┐
│  TCP Relay Server           │
│  - Port-based routing       │
│  - Bidirectional forwarding │
└────────┬────────────────────┘
         │
         ▼
┌─────────────────────────────┐
│  Local TCP Service          │
│  (Port 5000)                │
│  (e.g., Database)           │
└─────────────────────────────┘
```

### TLS/SNI Relay Flow
```
┌──────────────────────┐      ┌──────────────────────┐
│  Client: api.*.com   │      │  Client: db.*.com    │
│  (Port 443, SNI)     │      │  (Port 443, SNI)     │
└──────────┬───────────┘      └──────────┬───────────┘
           │                             │
           └──────────────┬──────────────┘
                          │
                          ▼
         ┌────────────────────────────────┐
         │  TLS/SNI Relay (Port 443)      │
         │  - SNI extraction              │
         │  - TLS passthrough             │
         │  - Multi-tenant routing        │
         └────────┬───────────┬───────────┘
                  │           │
         ┌────────▼──┐  ┌─────▼─────────┐
         │ API Srv   │  │ DB Server     │
         │ Port 3443 │  │ Port 4443     │
         │ (TLS)     │  │ (TLS)         │
         └───────────┘  └───────────────┘
```

---

## Common Patterns

### Creating a Relay in Your Application

```rust
use localup_lib::{HttpsServer, HttpsServerConfig, RouteRegistry};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create route registry
    let route_registry = Arc::new(RouteRegistry::new());

    // Configure HTTPS server
    let config = HttpsServerConfig {
        bind_addr: "0.0.0.0:443".parse()?,
        cert_path: "cert.pem".to_string(),
        key_path: "key.pem".to_string(),
    };

    // Create server
    let server = HttpsServer::new(config, route_registry.clone());

    // Start server
    tokio::spawn(async move { server.start().await });

    // Register routes
    route_registry.register_http(
        "myapp",
        "tunnel-001".to_string(),
        "127.0.0.1:3000".parse()?,
    )?;

    // Keep running
    tokio::signal::ctrl_c().await?;
    Ok(())
}
```

### Creating a Tunnel Client

```rust
use localup_lib::{TunnelClient, TunnelConfig, ProtocolConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TunnelConfig {
        local_host: "127.0.0.1".to_string(),
        local_port: 3000,
        protocol: ProtocolConfig::Https {
            local_port: 3000,
            subdomain: Some("myapp".to_string()),
        },
        control_plane_addr: Some("relay.example.com:4443".to_string()),
        ..Default::default()
    };

    let client = TunnelClient::connect(config).await?;
    println!("Tunnel URL: {}", client.public_url().unwrap());

    client.wait().await?;
    Ok(())
}
```

---

## Running All Examples

List all available examples:
```bash
cargo run --example 2>&1 | grep example
```

Run a specific example:
```bash
cargo run --example https_relay
cargo run --example tcp_relay
cargo run --example tls_relay
```

---

## Troubleshooting

### Port Already in Use
If you get "Address already in use" errors:
```bash
# Find and kill the process using the port
lsof -i :443
lsof -i :10000
lsof -i :8443

# Or change the port in the example
```

### Permission Denied (Port 443)
On Linux/macOS, ports below 1024 require root:
```bash
# Use a higher port instead
sudo cargo run --example tls_relay

# Or modify the example to use port 8443
```

### Certificate Issues
The examples generate self-signed certificates automatically. When connecting:
- Use `curl -k` to skip certificate verification
- Use `openssl s_client -servername` to specify SNI

---

## Next Steps

1. **Modify Examples**: Adapt the examples for your use case
2. **Add Authentication**: Implement custom JWT validation
3. **Add Metrics**: Track tunnel usage and performance
4. **Add Database**: Store tunnel metadata in a database
5. **Deploy**: Host the relay on a public server

See [CLAUDE.md](../CLAUDE.md) for architectural guidelines and best practices.
