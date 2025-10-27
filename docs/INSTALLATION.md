# Installation Guide

Multiple ways to install localup on Linux, macOS, and Windows.

---

## 📦 Quick Install (Linux/macOS)

### One-Liner Download Script

```bash
curl -fsSL https://raw.githubusercontent.com/localup-dev/localup/main/scripts/install.sh | bash
```

This will:
- Auto-detect your OS and architecture
- Download the latest release binaries
- Verify checksums
- Extract and make executable
- Show installation instructions

---

## 🍺 Homebrew (macOS/Linux)

### Prerequisites
- Homebrew installed: https://brew.sh

### Install

**Note:** Formula must be updated after each release. Check if the latest version is available.

```bash
# Stable release
brew install https://raw.githubusercontent.com/localup-dev/localup/main/Formula/localup.rb

# Beta/pre-release
brew install https://raw.githubusercontent.com/localup-dev/localup/main/Formula/localup-beta.rb
```

### Usage

```bash
# Client CLI
localup --version

# Relay server
localup-relay --version
```

---

## 📥 Manual Download (All Platforms)

### Linux

#### AMD64 (x86_64)
```bash
# Download
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/tunnel-linux-amd64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/tunnel-exit-node-linux-amd64.tar.gz"

# Extract
tar -xzf tunnel-linux-amd64.tar.gz
tar -xzf tunnel-exit-node-linux-amd64.tar.gz

# Install
sudo mv tunnel /usr/local/bin/localup
sudo mv tunnel-exit-node /usr/local/bin/localup-relay
sudo chmod +x /usr/local/bin/localup /usr/local/bin/localup-relay

# Verify
localup --version
localup-relay --version
```

#### ARM64 (aarch64)
```bash
# Download
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/tunnel-linux-arm64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/tunnel-exit-node-linux-arm64.tar.gz"

# Extract
tar -xzf tunnel-linux-arm64.tar.gz
tar -xzf tunnel-exit-node-linux-arm64.tar.gz

# Install
sudo mv tunnel /usr/local/bin/localup
sudo mv tunnel-exit-node /usr/local/bin/localup-relay
sudo chmod +x /usr/local/bin/localup /usr/local/bin/localup-relay
```

---

### macOS

#### Intel (AMD64)
```bash
# Download
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/tunnel-macos-amd64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/tunnel-exit-node-macos-amd64.tar.gz"

# Extract
tar -xzf tunnel-macos-amd64.tar.gz
tar -xzf tunnel-exit-node-macos-amd64.tar.gz

# Install
sudo mv tunnel /usr/local/bin/localup
sudo mv tunnel-exit-node /usr/local/bin/localup-relay
sudo chmod +x /usr/local/bin/localup /usr/local/bin/localup-relay
```

#### Apple Silicon (ARM64)
```bash
# Download
LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/tunnel-macos-arm64.tar.gz"
curl -L -O "https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}/tunnel-exit-node-macos-arm64.tar.gz"

# Extract
tar -xzf tunnel-macos-arm64.tar.gz
tar -xzf tunnel-exit-node-macos-arm64.tar.gz

# Install
sudo mv tunnel /usr/local/bin/localup
sudo mv tunnel-exit-node /usr/local/bin/localup-relay
sudo chmod +x /usr/local/bin/localup /usr/local/bin/localup-relay
```

---

### Windows

#### AMD64 (x86_64)

**PowerShell:**
```powershell
# Get latest version
$latestRelease = Invoke-RestMethod -Uri "https://api.github.com/repos/localup-dev/localup/releases/latest"
$version = $latestRelease.tag_name

# Download
Invoke-WebRequest -Uri "https://github.com/localup-dev/localup/releases/download/$version/tunnel-windows-amd64.zip" -OutFile "tunnel-windows-amd64.zip"
Invoke-WebRequest -Uri "https://github.com/localup-dev/localup/releases/download/$version/tunnel-exit-node-windows-amd64.zip" -OutFile "tunnel-exit-node-windows-amd64.zip"

# Extract
Expand-Archive -Path "tunnel-windows-amd64.zip" -DestinationPath "."
Expand-Archive -Path "tunnel-exit-node-windows-amd64.zip" -DestinationPath "."

# Rename
Rename-Item "tunnel.exe" "localup.exe"
Rename-Item "tunnel-exit-node.exe" "localup-relay.exe"

# Add to PATH or move to desired location
# Example: Move to C:\Program Files\Localup\
```

---

## 🔨 Build from Source

### Prerequisites
- Rust 1.90.0+ (`rustup`)
- Bun (for building webapps)
- Git

### Clone and Build

```bash
# Clone repository
git clone https://github.com/localup-dev/localup.git
cd localup

# Build release binaries
cargo build --release -p tunnel-cli -p tunnel-exit-node

# Install
sudo cp target/release/tunnel-cli /usr/local/bin/localup
sudo cp target/release/tunnel-exit-node /usr/local/bin/localup-relay
sudo chmod +x /usr/local/bin/localup /usr/local/bin/localup-relay
```

### Using the Install Script

```bash
# Build and install in one command
./scripts/install.sh
```

---

## 🔐 Verify Installation

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

## 🚀 Quick Start

### Start Relay Server

```bash
# Development (in-memory database)
localup-relay

# Production (with database)
localup-relay --database-url "sqlite://./tunnel.db?mode=rwc"
```

### Create Tunnel

```bash
# HTTP tunnel
localup http --port 3000

# TCP tunnel
localup tcp --port 5432

# With custom subdomain
localup http --port 8080 --subdomain myapp
```

---

## 📋 System Requirements

### Minimum Requirements
- **OS**: Linux (kernel 3.10+), macOS 10.15+, Windows 10+
- **CPU**: x86_64 or ARM64
- **RAM**: 128 MB
- **Disk**: 50 MB

### Recommended Requirements
- **RAM**: 512 MB+ (for relay server)
- **Network**: Stable internet connection
- **Ports**: 4443 (QUIC), 80/443 (HTTP/HTTPS) for relay server

---

## 🐛 Troubleshooting

### Binary not found after installation

**Linux/macOS:**
```bash
# Check if /usr/local/bin is in PATH
echo $PATH | grep /usr/local/bin

# If not, add to ~/.bashrc or ~/.zshrc:
export PATH="/usr/local/bin:$PATH"
source ~/.bashrc  # or ~/.zshrc
```

### Permission denied

```bash
# Make binaries executable
chmod +x /usr/local/bin/localup
chmod +x /usr/local/bin/localup-relay
```

### macOS Security Warning

If you get "cannot be opened because it is from an unidentified developer":

```bash
# Remove quarantine attribute
xattr -d com.apple.quarantine /usr/local/bin/localup
xattr -d com.apple.quarantine /usr/local/bin/localup-relay
```

### Windows SmartScreen Warning

If Windows blocks the executable:
1. Click "More info"
2. Click "Run anyway"

Or use PowerShell:
```powershell
Unblock-File -Path .\localup.exe
Unblock-File -Path .\localup-relay.exe
```

---

## 🔄 Updating

### Homebrew

```bash
brew upgrade localup
```

### Manual

Download and install the latest version following the manual installation steps above.

### From Source

```bash
cd localup
git pull origin main
cargo build --release -p tunnel-cli -p tunnel-exit-node
sudo cp target/release/tunnel-cli /usr/local/bin/localup
sudo cp target/release/tunnel-exit-node /usr/local/bin/localup-relay
```

---

## ❌ Uninstalling

### Homebrew

```bash
brew uninstall localup
```

### Manual

```bash
# Remove binaries
sudo rm /usr/local/bin/localup
sudo rm /usr/local/bin/localup-relay

# Remove configuration (optional)
rm -rf ~/.config/localup
rm -rf ~/.localup
```

---

## 📚 See Also

- [Quick Start Guide](../README.md#quick-start)
- [Release Guide](RELEASING.md) - For maintainers
- [Formula Documentation](../Formula/README.md) - Homebrew details
