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

## 🚀 Quick Start (30 seconds)

See what localup can do in under a minute:

```bash
# 1. Generate self-signed certificate (first time only, using v3/v3 SAN for localhost)
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout key.pem -out cert.pem -days 365 \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost"

# 2. Start relay server (Terminal 1)
# ✅ Relay is now listening on:
#    - localhost:4443/UDP  (QUIC control plane - where clients connect)
#    - localhost:8080/TCP  (HTTP - access tunneled services)
#    - localhost:8443/TCP  (HTTPS - access tunneled services)
#    - localhost:9090/TCP  (REST API)
localup relay --http-addr=0.0.0.0:18080 --https-addr=0.0.0.0:18443 \
  --tls-cert=cert.pem --tls-key=key.pem \
  --jwt-secret="my-jwt-secret" --localup-addr="0.0.0.0:14443"

# 3. Start a local HTTP server (Terminal 2)
python3 -m http.server 13000

export TOKEN=$(localup generate-token --secret "my-jwt-secret" --sub "myapp" --token-only)

# 4. Create a tunnel (Terminal 3)
localup --port 13000 --relay localhost:14443 --subdomain myapp --token=$TOKEN

# 5. Access your local server via HTTPS
curl -k https://myapp.localhost:18443
# 6. Access your local server via HTTP
curl http://myapp.localhost:18080
```

**That's it!** Your local server is now publicly accessible.

### More Examples

**TCP Tunnel (for databases, SSH):**
```bash
# Terminal 1: Start relay with TCP port range enabled
# Note: All three addresses (localup-addr, http-addr, https-addr) are required
# for TCP port allocation to work properly
# Terminal 1: Start relay with TCP port range enabled
localup relay \
  --localup-addr "0.0.0.0:14443" \
  --http-addr "0.0.0.0:18080" \
  --https-addr "0.0.0.0:18443" \
  --tcp-port-range "10000-20000" \
  --tls-cert=cert.pem --tls-key=key.pem \
  --jwt-secret "my-jwt-secret"
# Terminal 2: Generate token
export TOKEN=$(localup generate-token --secret "my-jwt-secret" --sub "myapp" --token-only)

# Terminal 3: Expose local PostgreSQL on a dynamic port
localup --port 5432 --protocol tcp --relay localhost:14443 --token="$TOKEN"
# Accessible at: localhost:PORT_ALLOCATED (check output for assigned port)
```

**TLS/SNI Tunnel (end-to-end encryption with SNI routing):**
```bash
# Terminal 1: Start relay with TLS/SNI server on port 443
localup relay --tls-addr 0.0.0.0:443 \
  --jwt-secret "my-jwt-secret"

# Terminal 2: Expose your TLS service via SNI
localup --port 3443 --protocol tls --relay localhost:4443 \
  --subdomain api.example.com --token="my-jwt-secret"

# Terminal 3: Test it (the relay will route based on SNI hostname)
openssl s_client -connect localhost:443 -servername api.example.com
```

**Generate JWT Token for Production:**
```bash
# Generate a token with custom subject ID
localup generate-token --secret "your-secret-key" --sub "myapp" --token-only

# Without --sub, generates random UUID automatically
localup generate-token --secret "your-secret-key" --token-only
```

---

## 📦 Installation

### Installation Guide by Platform

Select your operating system to see the recommended installation method:

| Platform | Recommended Method | Time | Level |
|----------|-------------------|------|-------|
| **macOS** | [Homebrew](#option-1-homebrew-macoslinux) or [Binary](#option-2-download-pre-built-binaries) | < 1 min | ⭐ Easiest |
| **Linux** | [Homebrew](#option-1-homebrew-macoslinux) or [Binary](#option-2-download-pre-built-binaries) | < 1 min | ⭐ Easiest |
| **Windows** | [Docker](#option-4-docker) or [Binary (PowerShell)](#windows-amd64) | < 2 min | ⭐⭐ Easy |
| **Docker** | [Docker / Docker Compose](#option-4-docker) | 5-15 min | ⭐⭐ Easy |
| **Any OS** | [Build from Source](#option-3-build-from-source) | 10-20 min | ⭐⭐⭐ Advanced |

---

### Quick Install (One-Liner)

**macOS / Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/localup-dev/localup/main/scripts/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/localup-dev/localup/main/scripts/install.ps1 | iex
```

These scripts auto-detect your architecture, download the latest release, verify checksums, and guide you through installation.

---

### Option 1: Homebrew (macOS/Linux)

**Note:** Formula must be updated after each release. Check [releases](https://github.com/localup-dev/localup/releases) for the latest version.

```bash
# Stable release
brew tap localup-dev/tap
brew install localup

# Verify installation
localup --version
localup --help
```

This installs a single binary with multiple subcommands:
- **`localup --port 3000 --relay ...`** - Client: Create tunnels to your relay
- **`localup relay`** - Run as relay/exit node server
- **`localup agent`** - Run as reverse tunnel agent
- **`localup agent-server`** - Combine relay and agent functionality (useful for VPN scenarios)
- **`localup generate-token`** - Generate JWT authentication tokens

---

### Option 2: Download Pre-built Binaries

This installs a single `localup` binary with all subcommands built-in.

#### Linux (AMD64)
```bash
# Get latest version
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

# Download single binary
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-linux-amd64.tar.gz"

# Extract and install
tar -xzf localup-linux-amd64.tar.gz
sudo mv localup /usr/local/bin/
sudo chmod +x /usr/local/bin/localup

# Verify
localup --version
localup --help
```

#### Linux (ARM64)
```bash
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-linux-arm64.tar.gz"
tar -xzf localup-linux-arm64.tar.gz
sudo mv localup /usr/local/bin/
sudo chmod +x /usr/local/bin/localup
```

#### macOS (Apple Silicon)
```bash
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-macos-arm64.tar.gz"
tar -xzf localup-macos-arm64.tar.gz
sudo mv localup /usr/local/bin/
xattr -d com.apple.quarantine /usr/local/bin/localup
```

#### macOS (Intel)
```bash
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-macos-amd64.tar.gz"
tar -xzf localup-macos-amd64.tar.gz
sudo mv localup /usr/local/bin/
xattr -d com.apple.quarantine /usr/local/bin/localup
```

#### Windows (AMD64)

**PowerShell (Recommended):**
```powershell
# Create directory for binary
mkdir "$env:LocalAppData\localup" -ErrorAction SilentlyContinue
cd "$env:LocalAppData\localup"

# Get latest version
$latestRelease = Invoke-RestMethod -Uri "https://api.github.com/repos/localup-dev/localup/releases/latest"
$version = $latestRelease.tag_name

# Download single binary
Invoke-WebRequest -Uri "https://github.com/localup-dev/localup/releases/download/$version/localup-windows-amd64.zip" -OutFile "localup-windows-amd64.zip"

# Extract
Expand-Archive -Path "localup-windows-amd64.zip" -DestinationPath "."
Remove-Item "localup-windows-amd64.zip"

# Add to PATH (permanently)
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notcontains "$env:LocalAppData\localup") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$env:LocalAppData\localup", "User")
    Write-Host "✅ Added to PATH. Please restart PowerShell for changes to take effect."
} else {
    Write-Host "✅ Already in PATH."
}

# Unblock executable
Unblock-File -Path "$env:LocalAppData\localup\localup.exe"

Write-Host "✅ Installation complete! Restart PowerShell and verify:"
Write-Host "   localup --version"
Write-Host "   localup --help"
```


---

### Option 3: Build from Source

**Prerequisites:**
- **Rust**: 1.90+ (install from [rustup.rs](https://rustup.rs))
- **Bun**: For building webapps (install from [bun.sh](https://bun.sh))
- **OpenSSL**: For TLS certificate generation
- **Git**: For cloning the repository

**Steps:**

```bash
# Clone the repository
git clone https://github.com/localup-dev/localup.git
cd localup

# Option 1: Use interactive install script
./scripts/install-local-from-source.sh

# Option 2: Manual build and install
# Build the unified binary
cargo build --release

# Install to system (Linux/macOS)
sudo cp target/release/localup /usr/local/bin/
sudo chmod +x /usr/local/bin/localup

# On Windows, copy to your desired location:
# Copy target/release/localup.exe to a directory in your PATH
```

**Verify installation:**
```bash
localup --version
localup --help
```

---

### Option 4: Docker

**Prerequisites:**
- Docker installed ([docker.com](https://www.docker.com))
- 5-15 minutes for build time (first run)

**Quick Start:**

```bash
# Build Docker image from source (recommended)
docker build -f Dockerfile -t localup:latest .

# Verify the image
docker run --rm localup:latest --version
docker run --rm localup:latest --help
```

**Generate Authentication Token:**

```bash
# Generate a JWT token for client authentication (token only)
TOKEN=$(docker run --rm localup:latest generate-token \
  --secret "my-super-secret-key" \
  --sub "myapp" \
  --token-only)

echo "Your token: $TOKEN"

# Or for verbose output with details (auto-generates random UUID if --sub not provided):
docker run --rm localup:latest generate-token \
  --secret "my-super-secret-key" \
  --sub "myapp"
```

**Run Relay Server in Docker:**

```bash
# First, build the Docker image (if not already built)
docker build -f Dockerfile -t localup:latest .

# Terminal 1: Run relay server (with JWT secret and TLS certificates)
docker run -d \
  --name localup-relay \
  -p 4443:4443/udp \
  -p 18080:18080 \
  -p 18443:18443 \
  -e RUST_LOG=info \
  -v "$(pwd)/relay-cert.pem:/app/relay-cert.pem:ro" \
  -v "$(pwd)/relay-key.pem:/app/relay-key.pem:ro" \
  localup:latest \
  relay \
    --localup-addr 0.0.0.0:4443 \
    --http-addr 0.0.0.0:18080 \
    --https-addr 0.0.0.0:18443 \
    --tls-cert /app/relay-cert.pem \
    --tls-key /app/relay-key.pem \
    --jwt-secret "my-super-secret-key"

# Check logs
docker logs -f localup-relay

# Stop when done
docker stop localup-relay
docker rm localup-relay
```

**Create a Tunnel in Docker:**

```bash
# Terminal 2: Create HTTP tunnel (standalone mode)
# Generate a token first (using --token-only for clean output):
export TOKEN=$(./target/release/localup generate-token --secret "my-super-secret-key" --sub "myapp" --token-only)

# For macOS/Windows Docker Desktop (use host.docker.internal):
docker run --rm \
  --network=host \
  --env TOKEN=$TOKEN \
  localup:latest \
    --address "host.docker.internal:5700" \
    --protocol http \
    --relay localhost:4443 \
    --subdomain myapp \
    --token "$TOKEN"

# Access at: http://localhost:18080/myapp

# For Linux (use the Docker bridge gateway):
docker run --rm \
  localup:latest \
    --port 3000 \
    --protocol http \
    --relay 172.17.0.1:4443 \
    --subdomain myapp \
    --token "YOUR_JWT_TOKEN_FROM_GENERATE_TOKEN"

# Or use Docker Compose network (see below for better approach)
```

**Using Docker Compose (Complete Setup):**

Docker Compose will automatically build the image when you run it. Create `docker-compose.yml`:
```yaml
version: '3.8'

services:
  relay:
    build:
      dockerfile: Dockerfile
    ports:
      - "4443:4443/udp"
      - "18080:18080"
      - "18443:18443"
    volumes:
      - ./relay-cert.pem:/app/relay-cert.pem:ro
      - ./relay-key.pem:/app/relay-key.pem:ro
    environment:
      RUST_LOG: info
    networks:
      - localup-net
    entrypoint: ["localup", "relay"]
    command:
      - "--localup-addr"
      - "0.0.0.0:4443"
      - "--http-addr"
      - "0.0.0.0:18080"
      - "--https-addr"
      - "0.0.0.0:18443"
      - "--tls-cert"
      - "/app/relay-cert.pem"
      - "--tls-key"
      - "/app/relay-key.pem"
      - "--jwt-secret"
      - "my-super-secret-key"
    healthcheck:
      test: ["CMD", "localup", "--help"]
      interval: 30s
      timeout: 10s
      retries: 3

  # Optional: Web server to expose via tunnel (internal only, no host port needed)
  web:
    image: python:3.11-slim
    networks:
      - localup-net
    command: python3 -m http.server 127.0.0.1 3000

  # Optional: Agent that creates a tunnel to the web server
  agent:
    build:
      dockerfile: Dockerfile
    networks:
      - localup-net
    depends_on:
      relay:
        condition: service_healthy
    environment:
      RUST_LOG: info
    entrypoint: ["localup"]
    command:
      - "--address"
      - "web:3000"
      - "--relay"
      - "relay:4443"
      - "--protocol"
      - "http"
      - "--subdomain"
      - "myapp"
      - "--token"
      - "my-super-secret-key"

networks:
  localup-net:
    driver: bridge
```

Deploy and monitor:
```bash
# Start all services
docker-compose up -d

# Check relay logs
docker-compose logs -f relay

# Generate a token interactively (verbose output)
docker-compose exec relay localup generate-token \
  --secret "my-super-secret-key" \
  --sub "myapp"

# Or for scripting (token only):
TOKEN=$(docker-compose exec relay localup generate-token \
  --secret "my-super-secret-key" \
  --sub "myapp" \
  --token-only)

# Access the tunneled web server
# Note: The web service on port 3000 is internal to Docker network
# Traffic flows: Host -> Relay (8080) -> Agent -> Web Service (3000)
curl http://localhost:8080/myapp

# Check agent connection status
docker-compose logs -f agent

# Shutdown
docker-compose down
```

**How it works:**
- **Web service** (port 3000): Runs internally in Docker network, not exposed to host
- **Agent**: Connects to relay and tunnels traffic from `web:3000` (internal hostname)
- **Relay**: Exposes tunneled service on `http://localhost:18080/myapp` with subdomain routing
- **Access**: Use relay's HTTP port (18080) + subdomain, NOT the internal port 3000

**Run HTTPS Relay with Certificates:**

For HTTPS support, you need TLS certificates. Use the pre-generated certificates in the repository:

```bash
# First, verify certificates exist in project root:
ls -l relay-cert.pem relay-key.pem

# Terminal 1: Run relay server with HTTPS (using volume mount for certificates)
docker run -d \
  --name localup-relay-https \
  -p 4443:4443/udp \
  -p 18080:18080 \
  -p 18443:18443 \
  -e RUST_LOG=info \
  -v "$(pwd)/relay-cert.pem:/app/relay-cert.pem:ro" \
  -v "$(pwd)/relay-key.pem:/app/relay-key.pem:ro" \
  localup:latest \
  relay \
    --localup-addr 0.0.0.0:4443 \
    --http-addr 0.0.0.0:18080 \
    --https-addr 0.0.0.0:18443 \
    --tls-cert /app/relay-cert.pem \
    --tls-key /app/relay-key.pem \
    --jwt-secret "my-super-secret-key"

# Check logs
docker logs -f localup-relay-https

# Terminal 2: Test HTTPS tunnel (with insecure SSL verification for self-signed cert)
docker run --rm \
  localup:latest \
    --port 3000 \
    --protocol https \
    --relay host.docker.internal:4443 \
    --subdomain secure-app \
    --token "YOUR_JWT_TOKEN_FROM_GENERATE_TOKEN"

# Access your service via HTTPS (ignore self-signed cert warning)
curl -k https://localhost:18443/secure-app
```

**Docker Compose with HTTPS:**

```yaml
version: '3.8'

services:
  relay:
    build:
      dockerfile: Dockerfile
    ports:
      - "4443:4443/udp"
      - "18080:18080"
      - "18443:18443"
    volumes:
      - ./relay-cert.pem:/app/relay-cert.pem:ro
      - ./relay-key.pem:/app/relay-key.pem:ro
    environment:
      RUST_LOG: info
    networks:
      - localup-net
    entrypoint: ["localup", "relay"]
    command:
      - "--localup-addr"
      - "0.0.0.0:4443"
      - "--http-addr"
      - "0.0.0.0:18080"
      - "--https-addr"
      - "0.0.0.0:18443"
      - "--tls-cert"
      - "/app/relay-cert.pem"
      - "--tls-key"
      - "/app/relay-key.pem"
      - "--jwt-secret"
      - "my-super-secret-key"
    healthcheck:
      test: ["CMD", "localup", "--help"]
      interval: 30s
      timeout: 10s
      retries: 3

  web:
    image: python:3.11-slim
    networks:
      - localup-net
    command: python3 -m http.server 127.0.0.1 3000

  agent:
    build:
      dockerfile: Dockerfile
    networks:
      - localup-net
    depends_on:
      relay:
        condition: service_healthy
    environment:
      RUST_LOG: info
    entrypoint: ["localup"]
    command:
      - "--address"
      - "web:3000"
      - "--relay"
      - "relay:4443"
      - "--protocol"
      - "https"
      - "--subdomain"
      - "myapp-secure"
      - "--token"
      - "my-super-secret-key"

networks:
  localup-net:
    driver: bridge
```

Test the HTTPS setup:
```bash
# Start all services
docker-compose up -d

# Generate a token (verbose output with details)
docker-compose exec relay localup generate-token \
  --secret "my-super-secret-key" \
  --sub "myapp"

# Or for scripting (token only):
TOKEN=$(docker-compose exec relay localup generate-token \
  --secret "my-super-secret-key" \
  --sub "myapp" \
  --token-only)

# Access the HTTPS service (ignore self-signed cert warning)
curl -k https://localhost:18443/myapp-secure

# Or use HTTP
curl http://localhost:18080/myapp-secure

# Cleanup
docker-compose down
```

**Generating Custom Certificates:**

If you need to generate new certificates for different hostnames:

```bash
# Generate for localhost (default)
openssl req -x509 -newkey rsa:2048 -keyout relay-key.pem -out relay-cert.pem \
  -days 365 -nodes -subj "/CN=localhost"

# Generate for specific domain
openssl req -x509 -newkey rsa:2048 -keyout relay-key.pem -out relay-cert.pem \
  -days 365 -nodes -subj "/CN=relay.example.com"

# Generate with multiple SANs (Subject Alternative Names)
openssl req -x509 -newkey rsa:2048 -keyout relay-key.pem -out relay-cert.pem \
  -days 365 -nodes -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,DNS:127.0.0.1,DNS:host.docker.internal"
```

**Available Dockerfiles:**

- **`Dockerfile`** (Recommended): Multi-stage build from Rust source, guaranteed correct Linux binary
- **`Dockerfile.prebuilt`** (Alternative): Fast build using pre-compiled Linux binary

**Docker Options:**

```bash
# Use specific version tag
docker build -t localup:v0.1.0 .

# Build specific Dockerfile
docker build -f Dockerfile.prebuilt -t localup:latest .

# Multi-stage build with caching
docker build --build-arg BUILDKIT_INLINE_CACHE=1 -t localup:latest .

# Pull from GitHub Container Registry (when published)
docker pull ghcr.io/davidviejo/localup:latest
docker run --rm ghcr.io/davidviejo/localup:latest --help
```

**For Complete Docker Documentation:**

See [DOCKER.md](DOCKER.md) for:
- Multi-stage build details
- Production deployment patterns
- Docker Compose examples
- Size optimization tips
- Troubleshooting guide

---

### Option 5: Use as Rust Library

Add to your `Cargo.toml`:

```toml
[dependencies]
localup-lib = { path = "path/to/localup/crates/localup-lib" }
tokio = { version = "1", features = ["full"] }
```

---

### Verify Installation

**For binary/Homebrew installation:**

```bash
# Check version
localup --version

# Check help
localup --help

# List available subcommands
localup relay --help
localup generate-token --help
```

**For Docker installation:**

```bash
# Verify image
docker images | grep localup

# Check version
docker run --rm localup:latest --version

# Check help
docker run --rm localup:latest --help
```

**Expected output:**
```
localup 0.1.0
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
# Make binary executable
chmod +x /usr/local/bin/localup
```

**macOS Security Warning:**

If you get "cannot be opened because it is from an unidentified developer":
```bash
# Remove quarantine attribute
xattr -d com.apple.quarantine /usr/local/bin/localup
```

**Windows SmartScreen Warning:**

If Windows blocks the executable:
1. Click "More info"
2. Click "Run anyway"

Or use PowerShell:
```powershell
Unblock-File -Path .\localup.exe
```

**Docker Installation Issues:**

*"Docker daemon not running"*:
```bash
# Start Docker daemon
docker run hello-world

# Or start Docker Desktop (macOS/Windows)
# Linux: sudo systemctl start docker
```

*"Cannot locate image" when building*:
```bash
# Ensure Dockerfile is in the repository root
docker build -f Dockerfile -t localup:latest .

# Check Docker build context
docker build -f Dockerfile -t localup:latest . --progress=plain
```

*"Exec format error" in container*:
The image was built with a Linux binary but container runtime is different. Solution:
```bash
# Rebuild the image (this compiles Linux binary inside Docker)
docker build --no-cache -f Dockerfile -t localup:latest .
```

*"Address already in use" when running container*:
```bash
# List running containers
docker ps

# Find and stop the container using the port
docker stop <container-id>

# Or use a different port
docker run -p 8081:8080 localup:latest relay --http-port 8080
```

**Docker Networking Issues:**

*"Connection refused" when accessing relay from another container*:
**Problem**: Container trying to connect to `localhost:4443` but relay is in a different container.

**Solutions**:
```bash
# Option 1: Use Docker Compose with shared network (recommended)
# See "Using Docker Compose" section above

# Option 2: For standalone containers, use the Docker bridge gateway
# macOS/Windows Docker Desktop:
docker run --rm localup:latest connect \
  --relay host.docker.internal:4443 \
  --port 3000 \
  --protocol http

# Linux (using Docker bridge gateway 172.17.0.1):
docker run --rm localup:latest connect \
  --relay 172.17.0.1:4443 \
  --port 3000 \
  --protocol http

# Option 3: Run on host network (shares host's network stack)
docker run --rm --network host localup:latest connect \
  --relay localhost:4443 \
  --port 3000 \
  --protocol http
```

*"Token authentication failed"*:
```bash
# Ensure relay and client use the same JWT secret
# Generate token with same secret:
TOKEN=$(docker run --rm localup:latest generate-token \
  --secret "same-secret-as-relay" \
  --sub "myapp" \
  --token-only)

# Use the generated token in the client:
docker run --rm localup:latest connect \
  --relay localhost:4443 \
  --token "$TOKEN" \
  --port 3000 \
  --protocol http
```

*"Port mapping not working"*:
```bash
# Verify port is exposed correctly:
docker ps
# Should show: 0.0.0.0:8080->8080/tcp

# Check if port is in use on host:
lsof -i :8080  # macOS/Linux
netstat -ano | findstr :8080  # Windows

# Use a different host port:
docker run -p 8081:8080 localup:latest relay --http-port 8080
```

---

### Updating

**Homebrew:**
```bash
brew upgrade localup
```

**Docker:**
```bash
# Rebuild from latest source (pulls latest dependencies)
docker build --no-cache -f Dockerfile -t localup:latest .

# Or pull latest image from GitHub Container Registry
docker pull ghcr.io/davidviejo/localup:latest
```

**Manual:**

Download and install the latest version following the manual installation steps above.

**From Source:**
```bash
cd localup
git pull origin main
cargo build --release
sudo cp target/release/localup /usr/local/bin/
sudo chmod +x /usr/local/bin/localup
```

---

### Uninstalling

**Homebrew:**
```bash
brew uninstall localup
```

**Docker:**
```bash
# Stop running container
docker stop localup-relay
docker rm localup-relay

# Remove image
docker rmi localup:latest

# Or remove from GitHub Container Registry
docker rmi ghcr.io/davidviejo/localup:latest
```

**Manual:**
```bash
# Remove binary
sudo rm /usr/local/bin/localup

# Remove configuration (optional)
rm -rf ~/.config/localup
rm -rf ~/.localup
```

## 🚀 Quick Start

### 1. Install Localup

```bash
brew tap localup-dev/tap
brew install localup
```

### 2. Start a Relay Server

```bash
# Generate self-signed certificate for development
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout key.pem -out cert.pem -days 365 \
  -subj "/CN=localhost"

# Start relay (in-memory database)
localup relay

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

# Terminal 2: Create tunnel (using standalone mode with global flags)
localup --port 3000 --protocol http --relay localhost:4443 --subdomain myapp

# Your local server is now accessible at:
# http://myapp.localhost:8080
```

### 4. Test Your Tunnel

```bash
# Access your local server through the tunnel
curl http://myapp.localhost:8080
```

### 5. Advanced: TLS/SNI Tunnel (Optional)

For exposing TLS services with Server Name Indication routing:

```bash
# Terminal 1: Start relay with TLS server on port 443
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout key.pem -out cert.pem -days 365 \
  -subj "/CN=localhost"

localup relay --tls-addr 0.0.0.0:443 --jwt-secret "demo-secret"

# Terminal 2: Start local TLS service
# (e.g., nginx with TLS, or any TLS server on port 3443)
python3 -m http.server --bind 127.0.0.1 3443

# Terminal 3: Create TLS tunnel with SNI routing (using standalone mode)
localup --port 3443 --protocol tls --relay localhost:4443 \
  --subdomain api.example.com --token "demo-secret"

# Terminal 4: Test the tunnel
openssl s_client -connect localhost:443 -servername api.example.com
```

### Using the Rust Library

For programmatic tunnel creation:

```rust
use localup_lib::Tunnel;

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

## 🏗️ Self-Hosting Localup

Localup can be self-hosted on your own infrastructure (VPS, on-premises, Kubernetes, Docker) to create private tunnels for your organization. This section covers common deployment scenarios.

### Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│        Your Infrastructure (Self-Hosted)             │
├─────────────────────────────────────────────────────┤
│                                                       │
│  ┌──────────────────────────────────────────────┐   │
│  │  Localup Relay Server (Public Endpoint)      │   │
│  │  - Runs on public IP/domain                  │   │
│  │  - Handles QUIC connections                 │   │
│  │  - Manages TCP/HTTP/HTTPS routing            │   │
│  └──────────────────────────────────────────────┘   │
│              ↓                                       │
│  ┌──────────────────────────────────────────────┐   │
│  │  Database (PostgreSQL/SQLite)                │   │
│  │  - Traffic inspection logs                   │   │
│  │  - Tunnel metadata                           │   │
│  │  - Metrics and analytics                     │   │
│  └──────────────────────────────────────────────┘   │
│              ↓                                       │
│  ┌──────────────────────────────────────────────┐   │
│  │  Your Internal Services                      │   │
│  │  - Web apps on localhost:3000                │   │
│  │  - PostgreSQL on localhost:5432              │   │
│  │  - APIs behind NAT/firewall                  │   │
│  └──────────────────────────────────────────────┘   │
│                                                       │
└─────────────────────────────────────────────────────┘
```

> **📝 Note on Port Configuration**: When using HTTPS in any scenario, both `--http-addr` (port 80 or 8080) and `--https-addr` (port 443 or 8443) must be configured. The HTTP port is essential for ACME certificate validation and HTTP → HTTPS redirects. The HTTPS port handles encrypted TLS traffic.

### Scenario 1: Development Setup (Single Machine)

**Use case**: Local development, testing tunnel functionality

```bash
# Terminal 1: Start relay with in-memory database
localup relay

# Terminal 2: Start a local HTTP server
python3 -m http.server 3000

# Terminal 3: Create a tunnel
localup --port 3000 --protocol http --relay localhost:4443 --subdomain myapp

# Access at: http://myapp.localhost:8080
```

**Files created**: None (in-memory database)

**Cleanup**: Press Ctrl+C on all terminals

---

### Scenario 2: Small Team with Persistent Storage (SQLite)

**Use case**: Small teams, internal staging environments, limited traffic

**Requirements**:
- Single machine or VPS
- ~1-2GB disk for SQLite database
- Less than 50 concurrent tunnels

**Setup**:

```bash
# 1. Create data directory
mkdir -p ~/.localup
cd ~/.localup

# 2. Start relay with persistent SQLite
localup relay \
  --database-url "sqlite://./tunnel.db?mode=rwc" \
  --http-addr "0.0.0.0:8080" \
  --https-addr "0.0.0.0:8443" \
  --tcp-port-range "10000-20000" \
  --control-addr "0.0.0.0:4443" \
  --domain "relay.yourcompany.local" \
  --jwt-secret "your-secret-key-change-this"

# 3. In another terminal, create tunnels
localup \
  --port 3000 \
  --protocol http \
  --relay "relay.yourcompany.local:4443" \
  --subdomain "staging-app" \
  --token "your-secret-key-change-this"
```

**Data persistence**: All tunnel data stored in `~/.localup/tunnel.db`

**Backup strategy**:
```bash
# Daily backup
cp ~/.localup/tunnel.db ~/.localup/tunnel.db.backup.$(date +%Y-%m-%d)

# Weekly retention
find ~/.localup -name "*.backup.*" -mtime +7 -delete
```

---

### Scenario 3: Production Setup (PostgreSQL + Multiple Machines)

**Use case**: Production deployments, high availability, 100+ concurrent tunnels

**Requirements**:
- PostgreSQL 13+
- Public domain name
- Valid TLS certificates (Let's Encrypt or custom)
- Multiple machines (optional, for HA)
- 4GB+ RAM, 50GB+ disk

**Step 1: Setup PostgreSQL**

```bash
# macOS
brew install postgresql
brew services start postgresql

# Linux (Ubuntu/Debian)
sudo apt-get install postgresql postgresql-contrib
sudo systemctl start postgresql
sudo systemctl enable postgresql

# Windows (using WSL2 or Docker)
# Option A: Docker
docker run --name postgres -e POSTGRES_PASSWORD=secret -p 5432:5432 -d postgres

# Option B: Windows native installer (https://www.postgresql.org/download/windows/)
```

**Step 2: Create Database**

```bash
# Create database and user
psql -U postgres << EOF
CREATE DATABASE localup_db;
CREATE USER localup WITH PASSWORD 'strong-password-change-this';
ALTER ROLE localup WITH CREATEDB;
GRANT ALL PRIVILEGES ON DATABASE localup_db TO localup;
EOF

# Test connection
psql -U localup -d localup_db -h localhost
```

**Step 3: Generate TLS Certificates**

```bash
# Option A: Let's Encrypt (recommended for production)
# Follow: https://letsencrypt.org/getting-started/
certbot certonly --standalone -d relay.yourcompany.com

# Then use:
# --cert-path "/etc/letsencrypt/live/relay.yourcompany.com/fullchain.pem"
# --key-path "/etc/letsencrypt/live/relay.yourcompany.com/privkey.pem"

# Option B: Self-signed (for internal/staging)
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout /opt/localup/key.pem \
  -out /opt/localup/cert.pem \
  -days 365 \
  -subj "/CN=relay.yourcompany.com"
```

> **⚠️ IMPORTANT: HTTPS Requires Both HTTP and HTTPS Ports**
>
> When running the relay with HTTPS support, you **must configure both ports**:
> - **Port 80 (HTTP)**: Required for ACME/Let's Encrypt certificate validation and HTTP → HTTPS redirects
> - **Port 443 (HTTPS)**: Required for encrypted TLS traffic to clients and tunnels
>
> Both `--http-addr` and `--https-addr` must be specified. If either is missing, HTTPS clients will fail to connect and certificate renewal will be blocked.

**Step 4: Start Relay Server**

```bash
localup relay \
  --control-addr "0.0.0.0:4443" \
  --http-addr "0.0.0.0:80" \
  --https-addr "0.0.0.0:443" \
  --tcp-port-range "10000-20000" \
  --domain "relay.yourcompany.com" \
  --database-url "postgres://localup:strong-password-change-this@localhost:5432/localup_db" \
  --jwt-secret "$(openssl rand -base64 32)" \
  --cert-path "/etc/letsencrypt/live/relay.yourcompany.com/fullchain.pem" \
  --key-path "/etc/letsencrypt/live/relay.yourcompany.com/privkey.pem"
```

**Step 5: Configure as Systemd Service (Linux)**

```bash
# Create service file
sudo tee /etc/systemd/system/localup-relay.service > /dev/null <<'EOF'
[Unit]
Description=Localup Relay Server
After=network.target postgresql.service
Wants=postgresql.service

[Service]
Type=simple
User=localup
WorkingDirectory=/opt/localup
Environment="RUST_LOG=info"

ExecStart=/usr/local/bin/localup relay \
  --control-addr "0.0.0.0:4443" \
  --http-addr "0.0.0.0:80" \
  --https-addr "0.0.0.0:443" \
  --tcp-port-range "10000-20000" \
  --domain "relay.yourcompany.com" \
  --database-url "postgres://localup:strong-password@localhost:5432/localup_db" \
  --jwt-secret "CHANGE-THIS-SECRET-KEY" \
  --cert-path "/etc/letsencrypt/live/relay.yourcompany.com/fullchain.pem" \
  --key-path "/etc/letsencrypt/live/relay.yourcompany.com/privkey.pem"

Restart=on-failure
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

# Create localup user
sudo useradd -r -s /bin/false localup

# Set permissions
sudo chown localup:localup /opt/localup
sudo chmod 755 /opt/localup

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable localup
sudo systemctl start localup

# Check status
sudo systemctl status localup
sudo journalctl -u localup -f
```

**Step 6: Firewall Configuration**

```bash
# Linux (UFW)
sudo ufw allow 4443/tcp  # QUIC control plane
sudo ufw allow 80/tcp    # HTTP
sudo ufw allow 443/tcp   # HTTPS
sudo ufw allow 10000:20000/tcp  # TCP tunnel ports

# macOS (if using pf)
# Add to /etc/pf.conf:
# pass in proto tcp from any to any port {4443, 80, 443, 10000:20000}
```

---

### Scenario 4: Docker Deployment

**Use case**: Container-based deployment, easier scaling and updates

**Files needed**:

Create `Dockerfile`:
```dockerfile
FROM rust:latest as builder

WORKDIR /workspace
COPY . .
RUN cargo build --release -p localup-exit-node

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /workspace/target/release/localup-exit-node /usr/local/bin/localup-relay

EXPOSE 4443/udp 80/tcp 443/tcp 8080/tcp 10000-20000/tcp

ENTRYPOINT ["/usr/local/bin/localup-relay"]
CMD ["--control-addr", "0.0.0.0:4443", \
     "--http-addr", "0.0.0.0:8080", \
     "--https-addr", "0.0.0.0:8443", \
     "--tcp-port-range", "10000-20000", \
     "--domain", "relay.local"]
```

Create `docker-compose.yml`:
```yaml
version: '3.8'

services:
  postgres:
    image: postgres:15-alpine
    environment:
      POSTGRES_DB: localup_db
      POSTGRES_USER: localup
      POSTGRES_PASSWORD: secure-password
    volumes:
      - postgres_data:/var/lib/postgresql/data
    ports:
      - "5432:5432"

  relay:
    build: .
    ports:
      - "4443:4443/udp"
      - "80:80/tcp"
      - "443:443/tcp"
      - "8080:8080/tcp"
      - "10000-20000:10000-20000/tcp"
    environment:
      RUST_LOG: info
    command: >
      localup-relay
      --control-addr 0.0.0.0:4443
      --http-addr 0.0.0.0:8080
      --https-addr 0.0.0.0:8443
      --tcp-port-range 10000-20000
      --domain relay.local
      --database-url postgres://localup:secure-password@postgres:5432/localup_db
      --jwt-secret change-this-secret
    depends_on:
      - postgres

volumes:
  postgres_data:
```

**Deploy**:
```bash
docker-compose up -d
docker-compose logs -f relay
```

---

### Scenario 5: Kubernetes Deployment

**Use case**: Enterprise deployments, automatic scaling, high availability

Create `k8s-relay.yaml`:
```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: localup

---
apiVersion: v1
kind: ConfigMap
metadata:
  name: localup-config
  namespace: localup
data:
  RUST_LOG: "info"

---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: localup-relay
  namespace: localup
spec:
  replicas: 2
  selector:
    matchLabels:
      app: localup-relay
  template:
    metadata:
      labels:
        app: localup-relay
    spec:
      containers:
      - name: relay
        image: localup-relay:latest
        imagePullPolicy: Always
        ports:
        - name: quic
          containerPort: 4443
          protocol: UDP
        - name: http
          containerPort: 8080
        - name: https
          containerPort: 8443
        args:
        - "--control-addr=0.0.0.0:4443"
        - "--http-addr=0.0.0.0:8080"
        - "--https-addr=0.0.0.0:8443"
        - "--tcp-port-range=10000-20000"
        - "--domain=relay.example.com"
        - "--database-url=postgres://localup:password@postgres:5432/localup_db"
        - "--jwt-secret=change-this-secret"
        envFrom:
        - configMapRef:
            name: localup-config
        livenessProbe:
          tcpSocket:
            port: 4443
          initialDelaySeconds: 10
          periodSeconds: 10
        readinessProbe:
          tcpSocket:
            port: 4443
          initialDelaySeconds: 5
          periodSeconds: 5

---
apiVersion: v1
kind: Service
metadata:
  name: localup-relay
  namespace: localup
spec:
  type: LoadBalancer
  selector:
    app: localup-relay
  ports:
  - name: quic
    port: 4443
    targetPort: 4443
    protocol: UDP
  - name: http
    port: 80
    targetPort: 8080
  - name: https
    port: 443
    targetPort: 8443
```

**Deploy**:
```bash
kubectl apply -f k8s-relay.yaml
kubectl get pods -n localup
kubectl logs -f -n localup deployment/localup-relay
```

---

### Scenario 6: Expose Internal Services (Common Pattern)

**Real-world example**: Expose internal PostgreSQL + staging web app

```bash
# Terminal 1: Start relay on your server (publicly accessible)
localup-relay \
  --control-addr "0.0.0.0:4443" \
  --http-addr "0.0.0.0:8080" \
  --tcp-port-range "10000-20000" \
  --domain "relay.mycompany.com" \
  --database-url "sqlite://./tunnel.db?mode=rwc"

# Terminal 2 (On internal machine behind NAT): Expose PostgreSQL
localup \
  --port 5432 \
  --protocol tcp \
  --relay "relay.mycompany.com:4443" \
  --token "your-secret-token"

# Terminal 3 (On internal machine): Expose web app
localup \
  --port 3000 \
  --protocol http \
  --relay "relay.mycompany.com:4443" \
  --subdomain "staging-app" \
  --token "your-secret-token"

# Terminal 4 (From any machine): Access services
# Connect to PostgreSQL
psql -h relay.mycompany.com -p 15234 -U postgres

# Access web app
curl http://staging-app.relay.mycompany.com:8080
```

---

### Scenario 7: Multi-Region Setup (Advanced)

**Use case**: Global tunnel network, geographic redundancy

```bash
# Primary relay (us-east-1)
localup-relay \
  --control-addr "0.0.0.0:4443" \
  --domain "relay-us.mycompany.com" \
  --database-url "postgres://localup:pass@postgres-primary:5432/localup_db"

# Secondary relay (eu-west-1)
localup-relay \
  --control-addr "0.0.0.0:4443" \
  --domain "relay-eu.mycompany.com" \
  --database-url "postgres://localup:pass@postgres-secondary:5432/localup_db"

# Client: Connect to nearest relay
localup \
  --port 3000 \
  --protocol http \
  --relay "relay-us.mycompany.com:4443" \
  --subdomain "myapp"
```

---

## 🔧 Relay Server Setup

### Development Setup

```bash
# Run with in-memory SQLite (no persistence)
localup relay

# Or with persistent SQLite
localup relay --database-url "sqlite://./tunnel.db?mode=rwc"

# If building from source:
cargo run --release
```

### Production Setup

See **Self-Hosting Scenarios** section above for complete production configurations, including:
- PostgreSQL setup
- TLS certificates
- Systemd service
- Firewall rules
- Docker deployment
- Kubernetes deployment

### Relay Configuration Options

```bash
localup relay [OPTIONS]

Options:
  --control-addr <ADDR>         Control plane address [default: 0.0.0.0:4443]
  --http-addr <ADDR>            HTTP server address [default: 0.0.0.0:8080]
  --https-addr <ADDR>           HTTPS server address [default: 0.0.0.0:8443]
  --tls-addr <ADDR>             TLS/SNI server address (e.g., 0.0.0.0:443)
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
sudo tee /etc/systemd/system/localup.service > /dev/null <<EOF
[Unit]
Description=Localup Relay Server
After=network.target postgresql.service

[Service]
Type=simple
User=tunnel
WorkingDirectory=/opt/tunnel
ExecStart=/usr/local/bin/localup relay \\
  --database-url "postgres://tunnel:password@localhost/localup_db" \\
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
sudo systemctl enable localup
sudo systemctl start localup

# Check status
sudo systemctl status localup
```

### SNI Server Setup

SNI (Server Name Indication) allows multiple TLS services to run on the same port (443) with routing based on hostname. This is a **passthrough mode** - the relay doesn't decrypt traffic or manage certificates.

#### How SNI Passthrough Works

1. **Client sends ClientHello** with SNI extension (hostname)
2. **Relay extracts SNI hostname** from ClientHello bytes (binary parsing, no decryption)
3. **Relay routes to appropriate tunnel** based on SNI hostname
4. **Connection forwarded directly** to local service (end-to-end encryption maintained)
5. **Relay never sees plaintext** (unlike HTTPS termination mode)

#### Setup SNI Relay

```bash
# Start relay with SNI server on port 443
# No certificates required - relay only reads SNI hostname from ClientHello
localup relay \
  --control-addr "0.0.0.0:4443" \
  --tls-addr "0.0.0.0:443" \
  --jwt-secret "your-secret-key"
```

#### Multi-Tenant SNI Example

Host multiple TLS services on the same relay with different hostnames:

```bash
# Terminal 1: Start relay with SNI on port 443
localup relay \
  --control-addr "0.0.0.0:4443" \
  --tls-addr "0.0.0.0:443" \
  --jwt-secret "demo-token"

# Terminal 2: Expose first TLS service (api.example.com)
localup --protocol tls \
  --port 3443 \
  --relay localhost:4443 \
  --subdomain "api.example.com" \
  --token "demo-token"

# Terminal 3: Expose second TLS service (db.example.com)
localup --protocol tls \
  --port 4443 \
  --relay localhost:4443 \
  --subdomain "db.example.com" \
  --token "demo-token"

# Terminal 4: Test routing
# Clients connecting with SNI "api.example.com" → routed to localhost:3443
openssl s_client -connect localhost:443 -servername api.example.com

# Clients connecting with SNI "db.example.com" → routed to localhost:4443
openssl s_client -connect localhost:443 -servername db.example.com
```

#### Best Practices for SNI

1. **No Certificate Management at Relay**:
   - SNI extraction happens at ClientHello (before TLS handshake)
   - Relay doesn't need certificates for SNI routing
   - Local services keep their own certificates
   - **Security advantage**: Relay cannot decrypt traffic

2. **Hostname Convention**:
   - Use descriptive, DNS-resolvable hostnames
   - Examples: `api-v1.company.com`, `db-replica.company.com`
   - Avoid reusing hostnames across different relays

3. **Security Model** (SNI vs HTTPS):
   - **SNI (passthrough)**: End-to-end encrypted, relay is blind, no cert needed
   - **HTTPS (termination)**: Relay decrypts, inspects, re-encrypts, manages certs
   - Choose SNI for maximum privacy/security
   - Choose HTTPS for traffic inspection

4. **Multiple Protocol Support** (Production Setup):
   ```bash
   # Single relay can handle all protocols simultaneously
   localup-relay \
     --control-addr "0.0.0.0:4443" \
     --http-addr "0.0.0.0:8080" \
     --https-addr "0.0.0.0:8443" \
     --tls-addr "0.0.0.0:443" \
     --tcp-port-range "10000-20000" \
     --database-url "postgres://localup:pass@localhost/localup_db" \
     --jwt-secret "secret-key"
   ```

## 🌐 Creating Tunnels (Client)

### Using the Rust Library

**HTTP Tunnel:**

```rust
use localup_lib::Tunnel;

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
use localup_lib::Tunnel;

let tunnel = Tunnel::tcp(5432) // Local PostgreSQL
    .relay("relay.example.com:4443")
    .token("your-auth-token")
    .connect()
    .await?;

println!("Connect to: {}:{}", tunnel.host(), tunnel.port());
// Prints: relay.example.com:15234 (dynamically allocated)
```

**TLS Tunnel (SNI-based passthrough):**

```rust
use localup_lib::Tunnel;

// Expose local TLS service with SNI hostname
let tunnel = Tunnel::tls(3443) // Local TLS service on port 3443
    .relay("relay.example.com:4443")
    .token("your-auth-token")
    .sni_hostname("api.example.com")  // SNI hostname for routing
    .connect()
    .await?;

println!("Public TLS endpoint: {}:{}", tunnel.host(), tunnel.port());
// Prints: relay.example.com:443

// Clients connect with:
// openssl s_client -connect relay.example.com:443 -servername api.example.com
```

**Multi-Tenant TLS Setup (Multiple Services):**

```rust
use localup_lib::Tunnel;
use tokio::task;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Expose three different TLS services on the same relay:443

    // Service 1: API server
    let api_tunnel = Tunnel::tls(3443)
        .relay("relay.example.com:4443")
        .token("demo-token")
        .sni_hostname("api.example.com")
        .connect()
        .await?;

    // Service 2: Database server
    let db_tunnel = Tunnel::tls(5443)
        .relay("relay.example.com:4443")
        .token("demo-token")
        .sni_hostname("db.example.com")
        .connect()
        .await?;

    // Service 3: Cache server
    let cache_tunnel = Tunnel::tls(6443)
        .relay("relay.example.com:4443")
        .token("demo-token")
        .sni_hostname("cache.example.com")
        .connect()
        .await?;

    println!("✅ All services exposed on relay.example.com:443");
    println!("   - api.example.com → localhost:3443");
    println!("   - db.example.com → localhost:5443");
    println!("   - cache.example.com → localhost:6443");

    // Keep tunnels alive
    tokio::select! {
        _ = api_tunnel.wait() => println!("API tunnel closed"),
        _ = db_tunnel.wait() => println!("DB tunnel closed"),
        _ = cache_tunnel.wait() => println!("Cache tunnel closed"),
    }

    Ok(())
}
```

**HTTPS Tunnel:**

```rust
use localup_lib::Tunnel;

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
localup \
  --port 3000 \
  --protocol http \
  --relay localhost:4443 \
  --subdomain myapp \
  --token demo-token

# TCP tunnel (e.g., PostgreSQL)
localup \
  --port 5432 \
  --protocol tcp \
  --relay localhost:4443 \
  --token demo-token

# TLS tunnel (SNI-based passthrough)
localup \
  --port 3443 \
  --protocol tls \
  --relay tunnel.example.com:4443 \
  --subdomain api.example.com \
  --token demo-token

# HTTPS tunnel
localup \
  --port 3000 \
  --protocol https \
  --relay tunnel.example.com:4443 \
  --subdomain myapp \
  --token demo-token

# TLS tunnel with SNI (passthrough, no decryption at relay)
localup \
  --port 3443 \
  --protocol tls \
  --relay tunnel.example.com:4443 \
  --subdomain api.example.com \
  --token demo-token

# Multiple TLS services on same relay (run in separate terminals)
# Terminal 1:
localup --protocol tls \
  --port 3443 \
  --relay tunnel.example.com:4443 \
  --subdomain api.example.com \
  --token demo-token

# Terminal 2:
localup --protocol tls \
  --port 4443 \
  --relay tunnel.example.com:4443 \
  --subdomain db.example.com \
  --token demo-token

# Clients connect with:
# openssl s_client -connect tunnel.example.com:443 -servername api.example.com
# openssl s_client -connect tunnel.example.com:443 -servername db.example.com
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
- **localup-proto**: Protocol definitions, messages, and multiplexing frames
- **localup-auth**: JWT authentication and token generation
- **localup-connection**: QUIC transport using quinn with reconnection logic
- **localup-router**: Routing registry for TCP/TLS/HTTP protocols
- **localup-cert**: Certificate storage and ACME integration

### Server Implementations
- **localup-server-tcp**: Raw TCP tunnel server
- **localup-server-tls**: TLS/SNI server with passthrough
- **localup-server-https**: HTTPS server with TLS termination

### Application Layer
- **localup-lib**: Main library entry point with high-level API ⭐ **Use this!**
- **localup-client**: Internal client implementation
- **localup-control**: Control plane for orchestration
- **localup-exit-node**: Exit node binary (orchestrator)
- **localup-cli**: Command-line tool

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
TLS passthrough with Server Name Indication (SNI) routing - no TLS termination at relay.

**How it works:**
1. Client sends TLS ClientHello with SNI extension specifying the target hostname
2. Relay extracts the hostname from the ClientHello (before full TLS handshake)
3. Relay routes the connection to the appropriate tunnel based on SNI hostname
4. All TLS encryption remains end-to-end between client and local service

**Benefits**:
- End-to-end encryption (relay never sees plaintext)
- Run multiple TLS services on the same port (443)
- No certificate management at relay (certs on local services)
- Support for wildcard certificates

**Use cases**: Multiple TLS APIs, SSL-based databases, custom protocols

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
export TUNNEL_DATABASE_URL="postgres://user:pass@localhost/localup_db"
export TUNNEL_JWT_SECRET="your-secret-key"
export TUNNEL_DOMAIN="tunnel.example.com"
```

### Database URLs

```bash
# PostgreSQL (recommended for production)
postgres://user:password@host:5432/database_name

# PostgreSQL with TimescaleDB (best for traffic inspection)
postgres://user:password@host:5432/localup_db?options=-c%20timescaledb.telemetry_level=off

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
localup relay --http-addr 0.0.0.0:8081
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
createdb localup_db
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
cargo test -p localup-lib --test integration_test

# Specific crate tests
cargo test -p localup-proto
```

**Testing Status**: 85+ passing tests including unit and integration tests

## 🛠️ Development

### Building from Source

```bash
# Build entire workspace
cargo build --workspace --release

# Build specific crate
cd crates/localup-exit-node
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
- ✅ TLS/SNI passthrough with hostname-based routing
- ✅ JWT authentication
- ✅ Routing and multiplexing
- ✅ Database layer with SeaORM

**In Progress**:
- 🚧 Web dashboard for traffic inspection
- 🚧 Complete ACME/Let's Encrypt integration
- 🚧 CLI tool improvements
- 🚧 Production-ready relay orchestration
- 🚧 Wildcard SNI hostname matching

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

## 📚 Documentation

For more detailed guides and examples, see the documentation folder:

| Document | Purpose |
|----------|---------|
| [**Examples**](docs/examples.md) | Common usage patterns and real-world examples |
| [**Daemon Mode**](docs/daemon.md) | Running multiple tunnels concurrently |
| [**Relay Selection**](docs/relay-selection.md) | Choosing and configuring exit nodes |
| [**Custom Relay Config**](docs/custom-relay-config.md) | Building custom relay configurations |
| [**Releasing**](docs/RELEASING.md) | Release process and versioning |

## 🌟 Support

- **Issues**: [GitHub Issues](https://github.com/localup-dev/localup/issues)
- **Discussions**: [GitHub Discussions](https://github.com/localup-dev/localup/discussions)
- **Development**: See [CLAUDE.md](CLAUDE.md) for architectural guidelines and development standards

---

**Built with ❤️ in Rust** | [Full Documentation](docs/) | [Report an Issue](https://github.com/localup-dev/localup/issues)
