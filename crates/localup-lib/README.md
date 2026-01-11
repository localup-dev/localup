# Tunnel Library - Simple Rust API for Geo-Distributed Tunnels

A high-level Rust library for creating secure tunnels to expose local services to the internet.

## Features

- ✅ **Super Simple API** - 3-5 lines of code to create a tunnel
- ✅ **Multiple Protocols** - HTTP, HTTPS, TCP, TLS/SNI
- ✅ **Automatic Port Allocation** - TCP tunnels get dynamic ports
- ✅ **Built-in Relay Server** - Run your own tunnel infrastructure
- ✅ **Address Flexibility** - Forward to localhost or remote addresses

## Quick Start

### Install

Add to your `Cargo.toml`:

```toml
[dependencies]
localup-lib = { path = "../localup-lib" }
tokio = { version = "1", features = ["full"] }
```

### Create a Tunnel (Client)

```rust
use localup_lib::Tunnel;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Expose local HTTP server on port 3000
    let tunnel = Tunnel::http(3000)
        .relay("localhost:4443")
        .token("your-auth-token")
        .subdomain("myapp")
        .connect()
        .await?;

    println!("Tunnel URL: {}", tunnel.url());

    tunnel.wait().await?;
    Ok(())
}
```

### Run a Relay Server

```rust
use localup_lib::Relay;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let relay = Relay::builder()
        .http("0.0.0.0:8080")
        .https("0.0.0.0:8443", "cert.pem", "key.pem")
        .tcp_port_range(10000, 20000)
        .localup_port("0.0.0.0:4443")
        .domain("tunnel.example.com")
        .jwt_secret("your-secret-key")
        .start()
        .await?;

    relay.wait_for_shutdown().await?;
    Ok(())
}
```

## Examples

### HTTP Tunnel

```rust
// Expose local web app
let tunnel = Tunnel::http(3000)
    .relay("localhost:4443")
    .token("my-token")
    .subdomain("webapp")
    .connect()
    .await?;

println!("Access at: {}", tunnel.url());
// Output: Access at: http://webapp.localhost
```

### TCP Tunnel (PostgreSQL, SSH, etc.)

```rust
// Expose PostgreSQL database
let tunnel = Tunnel::tcp(5432)
    .relay("localhost:4443")
    .token("my-token")
    .connect()
    .await?;

println!("Database at: {}", tunnel.url());
// Output: Database at: tcp://localhost:12481
```

### Forward to Remote Address

```rust
// Tunnel to a service on another machine in your network
let tunnel = Tunnel::http_to("192.168.1.100:3000")
    .relay("localhost:4443")
    .token("my-token")
    .connect()
    .await?;

// Or TCP to remote address
let tunnel = Tunnel::tcp_to("192.168.1.50:5432")
    .relay("localhost:4443")
    .token("my-token")
    .connect()
    .await?;
```

### HTTPS Tunnel

```rust
let tunnel = Tunnel::https(3000)
    .relay("localhost:4443")
    .token("my-token")
    .subdomain("secure")
    .connect()
    .await?;

println!("HTTPS URL: {}", tunnel.url());
// Output: HTTPS URL: https://secure.example.com
```

## Running the Examples

```bash
# Terminal 1: Start relay server
cargo run --example simple_relay

# Terminal 2: Start local HTTP server
python3 -m http.server 3000

# Terminal 3: Create tunnel
cargo run --example simple_tunnel

# Terminal 4: Test it
curl http://myapp.localhost:8080
```

## API Documentation

### Tunnel Builder

```rust
Tunnel::http(port: u16) -> TunnelBuilder
Tunnel::https(port: u16) -> TunnelBuilder
Tunnel::tcp(port: u16) -> TunnelBuilder
Tunnel::tls(port: u16) -> TunnelBuilder

// Forward to custom addresses
Tunnel::http_to(addr: &str) -> TunnelBuilder
Tunnel::tcp_to(addr: &str) -> TunnelBuilder
```

### Tunnel Builder Methods

```rust
.relay(addr: &str)          // Required: Relay server address
.token(token: &str)         // Required: Auth token
.subdomain(name: &str)      // Optional: For HTTP/HTTPS/TLS
.remote_port(port: u16)     // Optional: For TCP/TLS
.local_host(host: &str)     // Optional: Default "localhost"
.connect()                  // Connect and return Tunnel
```

### Relay Builder

```rust
Relay::builder()
    .http(addr: &str)
    .https(addr: &str, cert: &str, key: &str)
    .localup_port(addr: &str)
    .tcp_port_range(start: u16, end: u16)
    .domain(domain: &str)
    .jwt_secret(secret: &str)
    .start()
```

## Architecture

```
┌─────────────────┐         ┌─────────────────┐         ┌─────────────────┐
│  Your Service   │  ←───→  │  Tunnel Client  │  ←───→  │  Relay Server   │
│  localhost:3000 │         │  (this library) │         │  Public facing  │
└─────────────────┘         └─────────────────┘         └─────────────────┘
                                                                 ↑
                                                                 │
                                                         ┌───────┴───────┐
                                                    Internet Users
```

1. **Your Service**: Runs locally (HTTP server, database, etc.)
2. **Tunnel Client**: Connects to relay and forwards traffic
3. **Relay Server**: Public-facing server that routes traffic through tunnels

## Use Cases

- 🌐 **Web Development**: Share localhost with teammates/clients
- 🗄️ **Database Access**: Expose PostgreSQL, MySQL, MongoDB, Redis
- 🔐 **SSH Tunneling**: Secure access to remote machines
- 🎮 **Game Servers**: Host multiplayer games from home
- 🔧 **IoT Devices**: Connect devices behind NAT/firewall
- 📡 **Webhooks**: Receive webhooks during local development

## Security

- ✅ JWT-based authentication
- ✅ TLS 1.3 encryption for all connections
- ✅ Automatic HTTPS certificates
- ✅ Rate limiting (planned)
- ✅ IP allowlisting (planned)

## License

MIT OR Apache-2.0
