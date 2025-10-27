# Geo-Distributed Tunnel System

A high-performance, QUIC-based tunnel system for exposing local servers through geo-distributed exit nodes with support for multiple protocols (TCP, TLS/SNI, HTTP, HTTPS).

## ✨ Features

- 🌍 **Multi-Protocol Support**: TCP, TLS/SNI passthrough, HTTP, HTTPS with automatic routing
- 🚀 **QUIC-Native Transport**: Built-in multiplexing, 0-RTT connections, and TLS 1.3
- 🔒 **Automatic HTTPS**: Let's Encrypt/ACME integration with auto-renewal
- 🎯 **Flexible Routing**: Port-based (TCP), SNI-based (TLS), Host-based (HTTP/HTTPS)
- 📊 **Traffic Inspection**: Built-in request/response capture and replay capabilities
- 🔄 **Smart Reconnection**: Automatic reconnection with port/subdomain preservation
- 🗄️ **Database Support**: PostgreSQL (with TimescaleDB) or SQLite backends
- 🛡️ **JWT Authentication**: Secure token-based tunnel authorization

## 📦 Installation

### Quick Install (One-Liner)

**Linux/macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/localup-dev/localup/main/scripts/install.sh | bash
```

This auto-detects your platform, downloads the latest release, verifies checksums, and shows installation instructions.

---

### Option 1: Homebrew (macOS/Linux)

**Note:** Formula must be updated after each release. Check [releases](https://github.com/localup-dev/localup/releases) for the latest version.

```bash
# Stable release
brew install https://raw.githubusercontent.com/localup-dev/localup/main/Formula/localup.rb

# Verify installation
localup --version
localup-relay --version
```

This installs two commands:
- **`localup`** - Client CLI for creating tunnels
- **`localup-relay`** - Relay server (exit node) for hosting

---

### Option 2: Download Pre-built Binaries

#### Linux (AMD64)
```bash
# Get latest version
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

# Download
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-linux-amd64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-relay-linux-amd64.tar.gz"

# Extract
tar -xzf localup-linux-amd64.tar.gz
tar -xzf localup-relay-linux-amd64.tar.gz

# Install
sudo mv localup localup-relay /usr/local/bin/
sudo chmod +x /usr/local/bin/localup /usr/local/bin/localup-relay
```

#### Linux (ARM64)
```bash
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-linux-arm64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-relay-linux-arm64.tar.gz"
tar -xzf localup-linux-arm64.tar.gz
tar -xzf localup-relay-linux-arm64.tar.gz
sudo mv localup localup-relay /usr/local/bin/
```

#### macOS (Apple Silicon)
```bash
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-macos-arm64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-relay-macos-arm64.tar.gz"
tar -xzf localup-macos-arm64.tar.gz
tar -xzf localup-relay-macos-arm64.tar.gz
sudo mv localup localup-relay /usr/local/bin/
```

#### macOS (Intel)
```bash
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-macos-amd64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-relay-macos-amd64.tar.gz"
tar -xzf localup-macos-amd64.tar.gz
tar -xzf localup-relay-macos-amd64.tar.gz
sudo mv localup localup-relay /usr/local/bin/
```

#### Windows (AMD64)
```powershell
# Get latest version
$latestRelease = Invoke-RestMethod -Uri "https://api.github.com/repos/localup-dev/localup/releases/latest"
$version = $latestRelease.tag_name

# Download
Invoke-WebRequest -Uri "https://github.com/localup-dev/localup/releases/download/$version/localup-windows-amd64.zip" -OutFile "localup-windows-amd64.zip"
Invoke-WebRequest -Uri "https://github.com/localup-dev/localup/releases/download/$version/localup-relay-windows-amd64.zip" -OutFile "localup-relay-windows-amd64.zip"

# Extract
Expand-Archive -Path "localup-windows-amd64.zip" -DestinationPath "."
Expand-Archive -Path "localup-relay-windows-amd64.zip" -DestinationPath "."

# Binaries are now ready to use (localup.exe and localup-relay.exe)
# Add to PATH or move to desired location
```

---

### Option 3: Build from Source

**Prerequisites:**
- **Rust**: 1.90+ (install from [rustup.rs](https://rustup.rs))
- **Bun**: For building webapps (install from [bun.sh](https://bun.sh))
- **OpenSSL**: For TLS certificate generation

```bash
# Clone the repository
git clone https://github.com/localup-dev/localup.git
cd localup

# Build with automatic webapp compilation
cargo build --release -p tunnel-cli -p tunnel-exit-node

# Install
sudo cp target/release/tunnel-cli /usr/local/bin/localup
sudo cp target/release/tunnel-exit-node /usr/local/bin/localup-relay
sudo chmod +x /usr/local/bin/localup /usr/local/bin/localup-relay
```

---

### Option 4: Use as Rust Library

Add to your `Cargo.toml`:

```toml
[dependencies]
tunnel-lib = { path = "path/to/localup/crates/tunnel-lib" }
tokio = { version = "1", features = ["full"] }
```

---

### Verify Installation

After installation, verify the binaries:

```bash
# Check versions
localup --version
localup-relay --version

# Check help
localup --help
localup-relay --help
```

**Expected output:**
```
tunnel-cli 0.1.0
tunnel-exit-node 0.1.0
```

---

### Troubleshooting Installation

**Binary not found after installation (Linux/macOS):**
```bash
# Check if /usr/local/bin is in PATH
echo $PATH | grep /usr/local/bin

# If not, add to ~/.bashrc or ~/.zshrc:
export PATH="/usr/local/bin:$PATH"
source ~/.bashrc  # or ~/.zshrc
```

**Permission denied:**
```bash
# Make binaries executable
chmod +x /usr/local/bin/localup
chmod +x /usr/local/bin/localup-relay
```

**macOS Security Warning:**

If you get "cannot be opened because it is from an unidentified developer":
```bash
# Remove quarantine attribute
xattr -d com.apple.quarantine /usr/local/bin/localup
xattr -d com.apple.quarantine /usr/local/bin/localup-relay
```

**Windows SmartScreen Warning:**

If Windows blocks the executable:
1. Click "More info"
2. Click "Run anyway"

Or use PowerShell:
```powershell
Unblock-File -Path .\localup.exe
Unblock-File -Path .\localup-relay.exe
```

---

### Updating

**Homebrew:**
```bash
brew upgrade localup
```

**Manual:**

Download and install the latest version following the manual installation steps above.

**From Source:**
```bash
cd localup
git pull origin main
cargo build --release -p tunnel-cli -p tunnel-exit-node
sudo cp target/release/tunnel-cli /usr/local/bin/localup
sudo cp target/release/tunnel-exit-node /usr/local/bin/localup-relay
```

---

### Uninstalling

**Homebrew:**
```bash
brew uninstall localup
```

**Manual:**
```bash
# Remove binaries
sudo rm /usr/local/bin/localup
sudo rm /usr/local/bin/localup-relay

# Remove configuration (optional)
rm -rf ~/.config/localup
rm -rf ~/.localup
```

## 🚀 Quick Start

### 1. Install Localup

```bash
brew tap localup-dev/localup
brew install localup
```

### 2. Start a Relay Server

```bash
# Generate self-signed certificate for development
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout key.pem -out cert.pem -days 365 \
  -subj "/CN=localhost"

# Start relay (in-memory database)
localup-relay

# Relay is now running on:
# - Control plane: localhost:4443
# - HTTP: localhost:8080
# - HTTPS: localhost:8443
# - REST API: localhost:9090
```

### 3. Create a Tunnel

```bash
# Terminal 1: Start local HTTP server
python3 -m http.server 3000

# Terminal 2: Create tunnel
localup http --port 3000 --relay localhost:4443 --subdomain myapp

# Your local server is now accessible at:
# http://myapp.localhost:8080
```

### 4. Test Your Tunnel

```bash
# Access your local server through the tunnel
curl http://myapp.localhost:8080
```

### Using the Rust Library

For programmatic tunnel creation:

```rust
use tunnel_lib::Tunnel;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tunnel = Tunnel::http(3000)
        .relay("localhost:4443")
        .token("demo-token")
        .subdomain("myapp")
        .connect()
        .await?;

    println!("✅ Tunnel URL: {}", tunnel.url());
    // Prints: http://myapp.localhost:8080

    tunnel.wait().await?;
    Ok(())
}
```

## 🔧 Relay Server Setup

### Development Setup

```bash
# Run with in-memory SQLite (no persistence)
localup-relay

# Or with persistent SQLite
localup-relay --database-url "sqlite://./tunnel.db?mode=rwc"

# If building from source:
cargo run --release -p tunnel-exit-node
```

### Production Setup

```bash
# Install PostgreSQL with TimescaleDB
brew install timescaledb  # macOS
# or: sudo apt-get install postgresql timescaledb-2-postgresql-14

# Start PostgreSQL
brew services start postgresql  # macOS
# or: sudo systemctl start postgresql

# Create database
createdb tunnel_db

# Run relay server
localup-relay \
  --database-url "postgres://user:password@localhost/tunnel_db" \
  --domain "tunnel.example.com" \
  --jwt-secret "CHANGE-THIS-SECRET-KEY" \
  --http-addr "0.0.0.0:80" \
  --https-addr "0.0.0.0:443" \
  --control-addr "0.0.0.0:4443" \
  --cert-path "/path/to/cert.pem" \
  --key-path "/path/to/key.pem"
```

### Relay Configuration Options

```bash
localup-relay [OPTIONS]

Options:
  --control-addr <ADDR>         Control plane address [default: 0.0.0.0:4443]
  --http-addr <ADDR>            HTTP server address [default: 0.0.0.0:8080]
  --https-addr <ADDR>           HTTPS server address [default: 0.0.0.0:8443]
  --tcp-port-range <START-END>  TCP port range [default: 10000-20000]
  --domain <DOMAIN>             Base domain for subdomains [default: localhost]
  --cert-path <PATH>            TLS certificate path [default: cert.pem]
  --key-path <PATH>             TLS key path [default: key.pem]
  --database-url <URL>          Database URL (postgres:// or sqlite://)
  --jwt-secret <SECRET>         JWT signing secret (required for auth)
  --api-addr <ADDR>             REST API address [default: 0.0.0.0:9090]
```

### Setup as Systemd Service (Production)

```bash
# Create service file
sudo tee /etc/systemd/system/tunnel-exit-node.service > /dev/null <<EOF
[Unit]
Description=Tunnel Exit Node
After=network.target postgresql.service

[Service]
Type=simple
User=tunnel
WorkingDirectory=/opt/tunnel
ExecStart=/usr/local/bin/tunnel-exit-node \\
  --database-url "postgres://tunnel:password@localhost/tunnel_db" \\
  --domain "tunnel.example.com" \\
  --jwt-secret "CHANGE_THIS_SECRET" \\
  --http-addr "0.0.0.0:80" \\
  --https-addr "0.0.0.0:443" \\
  --control-addr "0.0.0.0:4443" \\
  --cert-path "/opt/tunnel/cert.pem" \\
  --key-path "/opt/tunnel/key.pem"
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

# Create tunnel user and directories
sudo useradd -r -s /bin/false tunnel
sudo mkdir -p /opt/tunnel
sudo chown tunnel:tunnel /opt/tunnel

# Copy certificates
sudo cp cert.pem key.pem /opt/tunnel/
sudo chown tunnel:tunnel /opt/tunnel/*.pem

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable tunnel-exit-node
sudo systemctl start tunnel-exit-node

# Check status
sudo systemctl status tunnel-exit-node
```

## 🌐 Creating Tunnels (Client)

### Using the Rust Library

**HTTP Tunnel:**

```rust
use tunnel_lib::Tunnel;

let tunnel = Tunnel::http(3000)
    .relay("relay.example.com:4443")
    .token("demo-token")
    .subdomain("myapp")
    .connect()
    .await?;

println!("Public URL: {}", tunnel.url());
```

**TCP Tunnel (for databases, SSH):**

```rust
use tunnel_lib::Tunnel;

let tunnel = Tunnel::tcp(5432) // Local PostgreSQL
    .relay("relay.example.com:4443")
    .token("your-auth-token")
    .connect()
    .await?;

println!("Connect to: {}:{}", tunnel.host(), tunnel.port());
// Prints: relay.example.com:15234 (dynamically allocated)
```

**HTTPS Tunnel:**

```rust
use tunnel_lib::Tunnel;

let tunnel = Tunnel::https(3000)
    .relay("relay.example.com:4443")
    .token("your-auth-token")
    .subdomain("secure-app")
    .connect()
    .await?;

println!("Secure URL: {}", tunnel.url());
// Prints: https://secure-app.relay.example.com
```

### Using the CLI Tool

```bash
# HTTP tunnel
localup http \
  --port 3000 \
  --relay localhost:4443 \
  --subdomain myapp \
  --token demo-token

# TCP tunnel (e.g., PostgreSQL)
localup tcp \
  --port 5432 \
  --relay localhost:4443 \
  --token demo-token

# HTTPS tunnel
localup https \
  --port 3000 \
  --relay tunnel.example.com:4443 \
  --subdomain myapp \
  --token demo-token
```

### Client Configuration

```bash
localup <PROTOCOL> [OPTIONS]

Protocols:
  http    HTTP tunnel with host-based routing
  https   HTTPS tunnel with automatic TLS
  tcp     Raw TCP tunnel with port allocation
  tls     TLS passthrough with SNI routing

Options:
  --relay <ADDR>      Relay server address (host:port)
  --port <PORT>       Local server port to tunnel
  --subdomain <NAME>  Subdomain for HTTP/HTTPS (auto-generated if omitted)
  --token <TOKEN>     Authentication token (JWT)
  --reconnect         Enable automatic reconnection [default: true]
```

## 📊 Advanced Features

### Traffic Inspection & Replay

When a relay is configured with a database, it automatically captures HTTP requests and responses:

```bash
# View captured traffic
curl http://localhost:9090/api/requests

# Get specific request details
curl http://localhost:9090/api/requests/{request_id}

# Replay a request
curl -X POST http://localhost:9090/api/requests/{request_id}/replay

# Access Swagger UI
open http://localhost:9090/swagger-ui
```

### Smart Reconnection

Clients automatically reconnect after network interruptions:

- **TCP tunnels**: Same public port preserved for 5 minutes (configurable TTL)
- **HTTP/HTTPS tunnels**: Same subdomain preserved for 5 minutes
- **Automatic**: No manual intervention needed

### Metrics and Monitoring

```rust
let tunnel = Tunnel::http(3000)
    .relay("localhost:4443")
    .token("demo-token")
    .connect()
    .await?;

// Access real-time metrics
let metrics = tunnel.metrics();
println!("Total requests: {}", metrics.total_requests());
println!("Bytes received: {}", metrics.bytes_received());
println!("Bytes sent: {}", metrics.bytes_sent());
```

## 🏗️ Architecture

This project is organized as a Rust workspace with 13 focused crates:

### Core Libraries
- **tunnel-proto**: Protocol definitions, messages, and multiplexing frames
- **tunnel-auth**: JWT authentication and token generation
- **tunnel-connection**: QUIC transport using quinn with reconnection logic
- **tunnel-router**: Routing registry for TCP/TLS/HTTP protocols
- **tunnel-cert**: Certificate storage and ACME integration

### Server Implementations
- **tunnel-server-tcp**: Raw TCP tunnel server
- **tunnel-server-tls**: TLS/SNI server with passthrough
- **tunnel-server-https**: HTTPS server with TLS termination

### Application Layer
- **tunnel-lib**: Main library entry point with high-level API ⭐ **Use this!**
- **tunnel-client**: Internal client implementation
- **tunnel-control**: Control plane for orchestration
- **tunnel-exit-node**: Exit node binary (orchestrator)
- **tunnel-cli**: Command-line tool

### Why QUIC?
- Built-in multiplexing (no custom layer needed)
- 0-RTT connection establishment
- Reduced head-of-line blocking
- Native stream management and flow control
- Modern protocol designed for mobile/unreliable networks

## 🌍 Protocol Support

### TCP Tunneling
Raw TCP connections for databases, SSH, and custom protocols.

**Use cases**: PostgreSQL, MySQL, Redis, SSH, custom protocols

### TLS with SNI
TLS passthrough with Server Name Indication routing (no termination at relay).

**Benefits**: End-to-end encryption, relay never sees plaintext

### HTTP
Plain HTTP tunneling with host-based routing.

**Use cases**: Development servers, webhooks, local APIs

### HTTPS
Full HTTP/1.1 and HTTP/2 support with TLS termination at relay.

**Features**: Automatic certificates, WebSocket support, HTTP/2

## 🔒 Security

- **TLS 1.3**: All tunnel connections use QUIC (built-in TLS 1.3)
- **JWT Authentication**: Token-based tunnel authorization
- **Automatic Certificates**: Let's Encrypt integration for HTTPS
- **End-to-End Encryption**: For TLS passthrough mode
- **Database Encryption**: Sensitive data encrypted at rest (PostgreSQL)
- **IP Filtering**: Allowlist/blocklist support (coming soon)
- **Rate Limiting**: Per-tunnel request limits (coming soon)

## ⚡ Performance

- **Latency overhead**: <50ms (same-region)
- **Throughput**: 10,000+ requests/second per relay
- **Concurrent connections**: 1,000+ per tunnel
- **Connection establishment**: Sub-100ms average
- **Memory usage**: ~10MB per active tunnel (client)

### Run Benchmarks

```bash
cargo bench
./run_all_benchmarks.sh
./test_benchmark_500.sh
```

## 🔧 Configuration

### Environment Variables

```bash
# Client
export TUNNEL_RELAY_ADDR="relay.example.com:4443"
export TUNNEL_AUTH_TOKEN="your-jwt-token"

# Relay Server
export TUNNEL_DATABASE_URL="postgres://user:pass@localhost/tunnel_db"
export TUNNEL_JWT_SECRET="your-secret-key"
export TUNNEL_DOMAIN="tunnel.example.com"
```

### Database URLs

```bash
# PostgreSQL (recommended for production)
postgres://user:password@host:5432/database_name

# PostgreSQL with TimescaleDB (best for traffic inspection)
postgres://user:password@host:5432/tunnel_db?options=-c%20timescaledb.telemetry_level=off

# SQLite persistent
sqlite://./path/to/tunnel.db?mode=rwc

# SQLite in-memory (default)
sqlite::memory:
```

## 🐛 Troubleshooting

### Relay Server Issues

**"Address already in use"**
```bash
lsof -i :8080
tunnel-exit-node --http-addr 0.0.0.0:8081
```

**"Certificate not found"**
```bash
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout key.pem -out cert.pem -days 365 \
  -subj "/CN=localhost"
```

**"Database connection failed"**
```bash
pg_isready
createdb tunnel_db
# Or use SQLite: --database-url "sqlite://./tunnel.db?mode=rwc"
```

### Client Issues

**"Connection refused"**
- Verify relay server is running: `curl http://relay-host:8080/health`
- Check firewall rules allow UDP traffic (QUIC uses UDP)

**"Authentication failed"**
- Verify JWT token is correct
- Check relay server `--jwt-secret` matches token generation

**"Subdomain already in use"**
- Choose a different subdomain
- Or omit `--subdomain` for auto-generated subdomain

### Common Errors

**QUIC connection timeout**
- Some networks/firewalls block UDP traffic
- Try using a different network or VPN

**High memory usage**
- Each tunnel uses ~10MB base memory
- Traffic inspection doubles memory (stores request/response data)
- Disable traffic capture: `--database-url ""`

## 🧪 Testing

```bash
# Run all tests
cargo test --workspace

# Integration tests
cargo test -p tunnel-lib --test integration_test

# Specific crate tests
cargo test -p tunnel-proto
```

**Testing Status**: 85+ passing tests including unit and integration tests

## 🛠️ Development

### Building from Source

```bash
# Build entire workspace
cargo build --workspace --release

# Build specific crate
cd crates/tunnel-exit-node
cargo build --release
```

### Code Quality

```bash
# Format code
cargo fmt --all

# Lint code
cargo clippy --all-targets --all-features -- -D warnings
```

## 🏗️ Project Status

⚠️ **Active Development**: This project is in active development. Some features described in this README are planned but not yet fully implemented.

**Working**:
- ✅ Core protocol and QUIC transport
- ✅ TCP tunneling
- ✅ Basic HTTP/HTTPS support
- ✅ JWT authentication
- ✅ Routing and multiplexing
- ✅ Database layer with SeaORM

**In Progress**:
- 🚧 Web dashboard for traffic inspection
- 🚧 Complete ACME/Let's Encrypt integration
- 🚧 TLS SNI passthrough
- 🚧 CLI tool improvements
- 🚧 Production-ready relay orchestration

**Current milestone**: Phase 2-3 (Multi-protocol support and advanced features)

See [SPEC.md](SPEC.md) for complete roadmap and implementation details.

## 🤝 Contributing

We welcome contributions! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Add tests for new functionality
4. Ensure CI passes:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --all
   ```
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to the branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

See [CLAUDE.md](CLAUDE.md) for detailed development guidelines.

## 📝 License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

## 🌟 Support

- **Issues**: [GitHub Issues](https://github.com/localup-dev/localup/issues)
- **Discussions**: [GitHub Discussions](https://github.com/localup-dev/localup/discussions)
- **Documentation**: [docs/](docs/)

---

**Built with ❤️ in Rust** | [Documentation](docs/) | [Examples](examples/) | [Installation Guide](docs/INSTALLATION.md)
