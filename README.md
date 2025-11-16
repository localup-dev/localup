# Geo-Distributed Tunnel System

A QUIC-based tunnel system for exposing local servers through geographically distributed exit nodes with support for multiple protocols (TCP, TLS/SNI, HTTP, HTTPS).

## ✨ Features

- 🌍 **Multi-Protocol Support**: TCP, TLS/SNI passthrough, HTTP, HTTPS
- 🚀 **QUIC-Native Transport**: Built-in multiplexing, 0-RTT connections, TLS 1.3
- 🔒 **Automatic HTTPS**: Let's Encrypt integration with auto-renewal
- 🎯 **Flexible Routing**: Port-based (TCP), SNI-based (TLS), Host-based (HTTP/HTTPS)
- 🔄 **Smart Reconnection**: Automatic reconnection with port/subdomain preservation
- 🛡️ **JWT Authentication**: Secure token-based tunnel authorization

## 🚀 Quick Start (2 minutes)

```bash
# 1. Generate self-signed certificate
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout key.pem -out cert.pem -days 365 \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost"

# 2. Start relay server (Terminal 1)
localup relay http \
  --localup-addr=0.0.0.0:14443 \
  --http-addr=0.0.0.0:18080 \
  --https-addr=0.0.0.0:18443 \
  --tls-cert=cert.pem --tls-key=key.pem \
  --jwt-secret="my-jwt-secret"

# 3. Start a local HTTP server (Terminal 2)
python3 -m http.server 13000

# 4. Create a tunnel (Terminal 3)
export TOKEN=$(localup generate-token --secret "my-jwt-secret" --sub "myapp" --token-only)
localup --port 13000 --relay localhost:14443 --subdomain myapp --token=$TOKEN

# 5. Access your service
curl -k https://myapp.localhost:18443
curl http://myapp.localhost:18080
```

---

## 📚 Three Essential Examples

### Example 1: HTTPS/HTTP Tunnel

Perfect for web applications, APIs, and webhooks.

```bash
# Generate self-signed v3 certificates (one-time)
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout key.pem -out cert.pem -days 365 \
  -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost"

# Terminal 1: Start relay server with HTTP/HTTPS support
localup relay http \
  --localup-addr "0.0.0.0:14443" \
  --http-addr "0.0.0.0:18080" \
  --https-addr "0.0.0.0:18443" \
  --tls-cert=cert.pem --tls-key=key.pem \
  --jwt-secret "my-jwt-secret"

# Terminal 2: Start a local web server
python3 -m http.server 3000

# Terminal 3: Generate token
export TOKEN=$(localup generate-token --secret "my-jwt-secret" --sub "myapp" --token-only)

# Terminal 4: Create tunnel
localup --port 3000 --protocol https --relay localhost:14443 --subdomain myapp --token "$TOKEN"

# Terminal 5: Access your service
curl -k https://myapp.localhost:18443
curl http://myapp.localhost:18080
```

### Example 2: TCP Tunnel

For databases, SSH, and custom TCP services.

```bash
# Terminal 1: Start relay with TCP port range
localup relay tcp \
  --localup-addr "0.0.0.0:14443" \
  --tcp-port-range "10000-20000" \
  --jwt-secret "my-jwt-secret"

# Terminal 2: Generate token
export TOKEN=$(localup generate-token --secret "my-jwt-secret" --sub "mydb" --token-only)

# Terminal 3: Expose local PostgreSQL (auto-allocate port)
localup --port 5432 --protocol tcp --relay localhost:14443 --token "$TOKEN" --remote-port=16432
# Wait for: ✅ TCP tunnel created: localhost:PORT

# OR request a specific port (must be within 10000-20000 range)
# localup --port 5432 --protocol tcp --relay localhost:14443 --remote-port 15432 --token "$TOKEN"

# Terminal 4: Connect from anywhere (use the port from step 3)
psql -h localhost -p 16432 -U postgres
```

### Example 3: TLS/SNI Tunnel

For end-to-end encrypted services with SNI-based routing (no certificates needed on relay).

```bash

# Terminal 1: Start relay with TLS/SNI server (no certificates needed)
localup relay tls \
  --localup-addr "0.0.0.0:14443" \
  --tls-addr "0.0.0.0:18443" \
  --jwt-secret "my-jwt-secret"

# Terminal 2: Start a local TLS service (using openssl s_server)
# Generate self-signed certificates for local TLS service (one-time)
rm tls-service-cert.pem tls-service-key.pem
openssl req -x509 -newkey rsa:2048 -keyout tls-service-key.pem -out tls-service-cert.pem \
  -days 365 -nodes -subj "/CN=localhost"

openssl s_server -cert tls-service-cert.pem -key tls-service-key.pem \
  -accept 3443 -www

# Terminal 3: Generate token
export TOKEN=$(localup generate-token --secret "my-jwt-secret" --sub "api" --token-only)

# Terminal 4: Expose your TLS service to the relay (SNI-based routing)
localup --port 3443 --protocol tls --relay localhost:14443 --subdomain api.example.com --token "$TOKEN"

# Terminal 5: Test the tunnel (relay routes based on SNI hostname)
openssl s_client -connect localhost:18443 -servername api.example.com
openssl s_client -connect localhost:3443 -servername api.example.com
```

---

## 📦 Installation

Choose one of two methods:

### Option 1: Homebrew (macOS/Linux)

```bash
brew tap localup-dev/tap
brew install localup

# Verify
localup --version
localup --help
```

### Option 2: Quick Install (One-Liner)

```bash
curl -fsSL https://raw.githubusercontent.com/localup-dev/localup/main/scripts/install.sh | bash
```

**For Docker**, see [DOCKER.md](DOCKER.md)

### Verify Installation

```bash
localup --version
localup relay --help        # Shows available relay subcommands
localup relay tcp --help    # Shows TCP relay options
localup relay tls --help    # Shows TLS/SNI relay options
localup relay http --help   # Shows HTTP/HTTPS relay options
localup relay all --help    # Shows all protocol options
localup generate-token --help
```

---

## 🔧 Basic Usage

### Relay Server Subcommands

The relay server uses subcommands to specify which protocols to enable:

```bash
localup relay <SUBCOMMAND> [OPTIONS]
```

**Subcommands:**
- `tcp` - TCP tunnel relay (port-based routing, port allocation)
- `tls` - TLS/SNI relay (SNI-based routing, no certificates needed)
- `http` - HTTP/HTTPS relay (host-based routing, TLS termination)
- `all` - All protocols (TCP, TLS, HTTP, HTTPS combined)

### Common Options (all subcommands)

```bash
--localup-addr <ADDR>         Control plane address [default: 0.0.0.0:4443]
--jwt-secret <SECRET>         JWT secret for authenticating clients
--domain <DOMAIN>             Public domain name [default: localhost]
--log-level <LEVEL>           Log level (trace, debug, info, warn, error)
--database-url <URL>          Database URL (postgres:// or sqlite://)
```

### TCP Relay Options

```bash
localup relay tcp [OPTIONS]

--tcp-port-range <START-END>  TCP port range [default: 10000-20000]
```

### TLS/SNI Relay Options

```bash
localup relay tls [OPTIONS]

--tls-addr <ADDR>             TLS/SNI server address [default: 0.0.0.0:4443]
```

### HTTP/HTTPS Relay Options

```bash
localup relay http [OPTIONS]

--http-addr <ADDR>            HTTP server address [default: 0.0.0.0:8080]
--https-addr <ADDR>           HTTPS server address (optional)
--tls-cert <PATH>             TLS certificate file (PEM format, required if --https-addr used)
--tls-key <PATH>              TLS private key file (PEM format, required if --https-addr used)
```

### All Protocols Relay Options

```bash
localup relay all [OPTIONS]

# Combines all options from tcp, tls, and http subcommands
--tcp-port-range <START-END>  TCP port range [default: 10000-20000]
--tls-addr <ADDR>             TLS/SNI server address [default: 0.0.0.0:4443]
--http-addr <ADDR>            HTTP server address [default: 0.0.0.0:8080]
--https-addr <ADDR>           HTTPS server address (optional)
--tls-cert <PATH>             TLS certificate file (optional)
--tls-key <PATH>              TLS private key file (optional)
```

### Client Options

```bash
localup [OPTIONS]

--port <PORT>                 Local port to expose
--address <HOST:PORT>         Local address to expose (alternative to --port)
--protocol <PROTOCOL>         Protocol: http, https, tcp, tls
--relay <ADDR>                Relay server address (host:port)
--subdomain <NAME>            Subdomain for HTTP/HTTPS
--remote-port <PORT>          Specific port for TCP tunnels (must be in relay's --tcp-port-range)
--token <TOKEN>               JWT authentication token
```

**TCP Port Allocation:**
- Without `--remote-port`: relay auto-allocates a port from the configured range
- With `--remote-port`: relay tries to allocate the specific port (must be within relay's `--tcp-port-range`)
- If requested port is unavailable: tunnel fails with error message
- **JWT tokens don't need special claims**: Any valid JWT token (with correct signature/expiration) works for TCP tunnels
- Requested port must be:
  - Within relay's `--tcp-port-range` (e.g., 10000-20000)
  - Not in use by OS (check with `lsof -i :PORT`)
  - Not already allocated to another tunnel

### Generate JWT Token

```bash
localup generate-token --secret "your-secret-key" --sub "myapp" --token-only
```

---

## 🐛 Troubleshooting

**"Address already in use" or "Failed to bind to"**
```bash
# Check what's using the port
lsof -i :19812

# If it's a lingering tunnel, kill it
kill -9 <PID>

# Or use a different port range
localup relay tcp --tcp-port-range "20000-30000" --jwt-secret "..."
```
*Note: TCP ports stay in TIME_WAIT for 60 seconds after closing. The relay automatically retries binding up to 3 times with 1-second delays.*

**"Certificate not found"**
```bash
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout key.pem -out cert.pem -days 365 \
  -subj "/CN=localhost"
```

**"Connection refused"**
- Verify relay is running: `lsof -i :14443`
- Check firewall allows UDP (QUIC uses port 4443/UDP)

**"Authentication failed"**
- Verify JWT token matches relay secret
- Generate new token: `localup generate-token --secret "your-secret" --sub "id"`

**Tunnel hangs on startup**
- Ensure relay server is running in Terminal 1
- Check relay is listening: `lsof -i :14443`
- Verify relay address matches client `--relay localhost:14443`

---

## 📖 Documentation

- [**CLAUDE.md**](CLAUDE.md) - Development guidelines and architecture
- [**DOCKER.md**](DOCKER.md) - Docker setup and deployment
- [**SPEC.md**](SPEC.md) - Complete technical specification

---

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Add tests for new functionality
4. Ensure CI passes:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --all
   ```
5. Commit and push
6. Open a Pull Request

---

## 📝 License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

---

## 🌟 Support

- **Issues**: [GitHub Issues](https://github.com/localup-dev/localup/issues)
- **Discussions**: [GitHub Discussions](https://github.com/localup-dev/localup/discussions)

**Built with ❤️ in Rust**
