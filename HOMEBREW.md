# Homebrew Tap for Localup

This repository contains a Homebrew tap for installing Localup tunnel system.

## Installation

### Option 1: Install from Tap (Recommended)

```bash
# Add the tap
brew tap localup-dev/localup

# Install the latest stable release
brew install localup

# Or install from HEAD (latest source)
brew install localup-head
```

### Option 2: Direct Install (without adding tap)

```bash
brew install localup-dev/localup/localup
```

## What Gets Installed

The formula installs two binaries:

- **`localup`** - Client CLI for creating tunnels
- **`localup-relay`** - Relay server (exit node) for hosting

## Quick Start

After installation:

```bash
# Start a relay server (development)
localup-relay

# In another terminal, create a tunnel
localup http --port 3000 --relay localhost:4443
```

## Updating

```bash
# Update tap
brew update

# Upgrade to latest version
brew upgrade localup
```

## Uninstalling

```bash
# Remove the package
brew uninstall localup

# Remove the tap
brew untap localup-dev/localup
```

## For Maintainers

### Creating a New Release

1. Update the version in `Formula/localup.rb`
2. Build and create release binaries for all platforms
3. Upload binaries to GitHub Releases
4. Calculate SHA256 checksums:

```bash
shasum -a 256 localup-darwin-arm64.tar.gz
shasum -a 256 localup-darwin-amd64.tar.gz
shasum -a 256 localup-linux-arm64.tar.gz
shasum -a 256 localup-linux-amd64.tar.gz
```

5. Update SHA256 hashes in `Formula/localup.rb`
6. Commit and push changes

### Testing the Formula

```bash
# Audit the formula
brew audit --strict localup

# Test installation
brew install --build-from-source localup

# Test the installed binaries
brew test localup

# Uninstall
brew uninstall localup
```

### Building Release Binaries

Use the provided script or GitHub Actions workflow:

```bash
# Build for current platform
cargo build --release -p tunnel-cli
cargo build --release -p tunnel-exit-node

# Create tarball
tar -czf localup-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m).tar.gz \
  -C target/release tunnel-cli tunnel-exit-node
```

## Supported Platforms

- **macOS**: ARM64 (Apple Silicon), AMD64 (Intel)
- **Linux**: ARM64, AMD64

## License

MIT OR Apache-2.0
