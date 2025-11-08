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

### Installation Guide by Platform

Select your operating system to see the recommended installation method:

| Platform | Recommended Method | Time | Level |
|----------|-------------------|------|-------|
| **macOS** | [Homebrew](#option-1-homebrew-macoslinux) or [Binary](#option-2-download-pre-built-binaries) | < 1 min | ⭐ Easiest |
| **Linux** | [Homebrew](#option-1-homebrew-macoslinux) or [Binary](#option-2-download-pre-built-binaries) | < 1 min | ⭐ Easiest |
| **Windows** | [Binary (PowerShell)](#windows-amd64) | < 2 min | ⭐⭐ Easy |
| **Any OS** | [Build from Source](#option-3-build-from-source) | 5-10 min | ⭐⭐⭐ Advanced |

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
localup-relay --version
localup-agent-server --version
```

This installs three commands:
- **`localup`** - Client CLI for creating tunnels to your relay
- **`localup-relay`** - Relay/exit node server that handles public connections
- **`localup-agent-server`** - Agent that combines relay + agent functionality (useful for VPN scenarios)

---

### Option 2: Download Pre-built Binaries

#### Linux (AMD64)
```bash
# Get latest version
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

# Download all binaries
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-linux-amd64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-relay-linux-amd64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-agent-server-linux-amd64.tar.gz"

# Extract
tar -xzf localup-linux-amd64.tar.gz
tar -xzf localup-relay-linux-amd64.tar.gz
tar -xzf localup-agent-server-linux-amd64.tar.gz

# Install
sudo mv localup localup-relay localup-agent-server /usr/local/bin/
sudo chmod +x /usr/local/bin/localup /usr/local/bin/localup-relay /usr/local/bin/localup-agent-server

# Verify
localup --version
localup-relay --version
localup-agent-server --version
```

#### Linux (ARM64)
```bash
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-linux-arm64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-relay-linux-arm64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-agent-server-linux-arm64.tar.gz"
tar -xzf localup-linux-arm64.tar.gz
tar -xzf localup-relay-linux-arm64.tar.gz
tar -xzf localup-agent-server-linux-arm64.tar.gz
sudo mv localup localup-relay localup-agent-server /usr/local/bin/
```

#### macOS (Apple Silicon)
```bash
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-macos-arm64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-relay-macos-arm64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-agent-server-macos-arm64.tar.gz"
tar -xzf localup-macos-arm64.tar.gz
tar -xzf localup-relay-macos-arm64.tar.gz
tar -xzf localup-agent-server-macos-arm64.tar.gz
sudo mv localup localup-relay localup-agent-server /usr/local/bin/
xattr -d com.apple.quarantine /usr/local/bin/localup /usr/local/bin/localup-relay /usr/local/bin/localup-agent-server
```

#### macOS (Intel)
```bash
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-macos-amd64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-relay-macos-amd64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/localup-agent-server-macos-amd64.tar.gz"
tar -xzf localup-macos-amd64.tar.gz
tar -xzf localup-relay-macos-amd64.tar.gz
tar -xzf localup-agent-server-macos-amd64.tar.gz
sudo mv localup localup-relay localup-agent-server /usr/local/bin/
xattr -d com.apple.quarantine /usr/local/bin/localup /usr/local/bin/localup-relay /usr/local/bin/localup-agent-server
```

#### Windows (AMD64)

**PowerShell (Recommended):**
```powershell
# Create directory for binaries
mkdir "$env:LocalAppData\localup" -ErrorAction SilentlyContinue
cd "$env:LocalAppData\localup"

# Get latest version
$latestRelease = Invoke-RestMethod -Uri "https://api.github.com/repos/localup-dev/localup/releases/latest"
$version = $latestRelease.tag_name

# Download all binaries
Invoke-WebRequest -Uri "https://github.com/localup-dev/localup/releases/download/$version/localup-windows-amd64.zip" -OutFile "localup-windows-amd64.zip"
Invoke-WebRequest -Uri "https://github.com/localup-dev/localup/releases/download/$version/localup-relay-windows-amd64.zip" -OutFile "localup-relay-windows-amd64.zip"
Invoke-WebRequest -Uri "https://github.com/localup-dev/localup/releases/download/$version/localup-agent-server-windows-amd64.zip" -OutFile "localup-agent-server-windows-amd64.zip"

# Extract
Expand-Archive -Path "localup-windows-amd64.zip" -DestinationPath "."
Expand-Archive -Path "localup-relay-windows-amd64.zip" -DestinationPath "."
Expand-Archive -Path "localup-agent-server-windows-amd64.zip" -DestinationPath "."

# Remove archives
Remove-Item "*.zip"

# Add to PATH (permanently)
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notcontains "$env:LocalAppData\localup") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$env:LocalAppData\localup", "User")
    Write-Host "✅ Added to PATH. Please restart PowerShell for changes to take effect."
} else {
    Write-Host "✅ Already in PATH."
}

# Unblock executables
Unblock-File -Path "$env:LocalAppData\localup\localup.exe"
Unblock-File -Path "$env:LocalAppData\localup\localup-relay.exe"
Unblock-File -Path "$env:LocalAppData\localup\localup-agent-server.exe"

Write-Host "✅ Installation complete! Restart PowerShell and verify:"
Write-Host "   localup --version"
Write-Host "   localup-relay --version"
Write-Host "   localup-agent-server --version"
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
./scripts/install-local.sh

# Option 2: Quick install (no prompts)
./scripts/install-local-quick.sh

# Option 3: Manual build and install
# Build all three binaries
cargo build --release -p localup -p localup-exit-node -p localup-agent-server

# Install to system (Linux/macOS)
sudo cp target/release/localup target/release/localup-relay target/release/localup-agent-server /usr/local/bin/
sudo chmod +x /usr/local/bin/localup /usr/local/bin/localup-relay /usr/local/bin/localup-agent-server

# On Windows, copy to your desired location:
# Copy target/release/localup.exe, localup-relay.exe, and localup-agent-server.exe
# to a directory in your PATH
```

**Verify installation:**
```bash
localup --version
localup-relay --version
localup-agent-server --version
```

---

### Option 4: Use as Rust Library

Add to your `Cargo.toml`:

```toml
[dependencies]
localup-lib = { path = "path/to/localup/crates/localup-lib" }
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
localup-cli 0.1.0
localup-exit-node 0.1.0
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
cargo build --release -p localup-cli -p localup-exit-node
sudo cp target/release/localup-cli /usr/local/bin/localup
sudo cp target/release/localup-exit-node /usr/local/bin/localup-relay
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

### 5. Advanced: TLS/SNI Tunnel (Optional)

For exposing TLS services with Server Name Indication routing:

```bash
# Terminal 1: Start relay with TLS server on port 443
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout key.pem -out cert.pem -days 365 \
  -subj "/CN=localhost"

localup-relay --tls-addr 0.0.0.0:443

# Terminal 2: Start local TLS service
# (e.g., nginx with TLS, or any TLS server on port 3443)
python3 -m http.server --bind 127.0.0.1 3443

# Terminal 3: Create TLS tunnel with SNI routing
localup tls \
  --port 3443 \
  --relay localhost:4443 \
  --sni-hostname api.example.com \
  --token demo-token

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
localup-relay

# Terminal 2: Start a local HTTP server
python3 -m http.server 3000

# Terminal 3: Create a tunnel
localup http --port 3000 --relay localhost:4443 --subdomain myapp

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
localup-relay \
  --database-url "sqlite://./tunnel.db?mode=rwc" \
  --http-addr "0.0.0.0:8080" \
  --https-addr "0.0.0.0:8443" \
  --tcp-port-range "10000-20000" \
  --control-addr "0.0.0.0:4443" \
  --domain "relay.yourcompany.local" \
  --jwt-secret "your-secret-key-change-this"

# 3. In another terminal, create tunnels
localup http \
  --port 3000 \
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
localup-relay \
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

ExecStart=/usr/local/bin/localup-relay \
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
sudo systemctl enable localup-relay
sudo systemctl start localup-relay

# Check status
sudo systemctl status localup-relay
sudo journalctl -u localup-relay -f
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
localup tcp \
  --port 5432 \
  --relay "relay.mycompany.com:4443" \
  --token "your-secret-token"

# Terminal 3 (On internal machine): Expose web app
localup http \
  --port 3000 \
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
localup http \
  --port 3000 \
  --relay "relay-us.mycompany.com:4443" \
  --subdomain "myapp"
```

---

## 🔧 Relay Server Setup

### Development Setup

```bash
# Run with in-memory SQLite (no persistence)
localup-relay

# Or with persistent SQLite
localup-relay --database-url "sqlite://./tunnel.db?mode=rwc"

# If building from source:
cargo run --release -p localup-exit-node
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
localup-relay [OPTIONS]

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
sudo tee /etc/systemd/system/localup-exit-node.service > /dev/null <<EOF
[Unit]
Description=Tunnel Exit Node
After=network.target postgresql.service

[Service]
Type=simple
User=tunnel
WorkingDirectory=/opt/tunnel
ExecStart=/usr/local/bin/localup-exit-node \\
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
sudo systemctl enable localup-exit-node
sudo systemctl start localup-exit-node

# Check status
sudo systemctl status localup-exit-node
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
localup-relay \
  --control-addr "0.0.0.0:4443" \
  --tls-addr "0.0.0.0:443" \
  --jwt-secret "your-secret-key"
```

#### Multi-Tenant SNI Example

Host multiple TLS services on the same relay with different hostnames:

```bash
# Terminal 1: Start relay with SNI on port 443
localup-relay \
  --control-addr "0.0.0.0:4443" \
  --tls-addr "0.0.0.0:443" \
  --jwt-secret "demo-token"

# Terminal 2: Expose first TLS service (api.example.com)
localup tls \
  --port 3443 \
  --relay localhost:4443 \
  --sni-hostname "api.example.com" \
  --token "demo-token"

# Terminal 3: Expose second TLS service (db.example.com)
localup tls \
  --port 4443 \
  --relay localhost:4443 \
  --sni-hostname "db.example.com" \
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

# TLS tunnel (SNI-based passthrough)
localup tls \
  --port 3443 \
  --relay tunnel.example.com:4443 \
  --sni-hostname api.example.com \
  --token demo-token

# HTTPS tunnel
localup https \
  --port 3000 \
  --relay tunnel.example.com:4443 \
  --subdomain myapp \
  --token demo-token

# TLS tunnel with SNI (passthrough, no decryption at relay)
localup tls \
  --port 3443 \
  --relay tunnel.example.com:4443 \
  --sni-hostname api.example.com \
  --token demo-token

# Multiple TLS services on same relay (run in separate terminals)
# Terminal 1:
localup tls \
  --port 3443 \
  --relay tunnel.example.com:4443 \
  --sni-hostname api.example.com \
  --token demo-token

# Terminal 2:
localup tls \
  --port 4443 \
  --relay tunnel.example.com:4443 \
  --sni-hostname db.example.com \
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
localup-exit-node --http-addr 0.0.0.0:8081
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
