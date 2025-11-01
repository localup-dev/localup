# Building LocalUp with Custom Relay Configuration

## Quick Start

### 1. Create Your Relay Configuration

```bash
# Copy the example
cp relays.example.yaml my-relays.yaml

# Edit with your relay servers
vim my-relays.yaml
```

Update the relay endpoints:
```yaml
relays:
  - id: my-relay
    name: My Relay Server
    region: eu-west
    location:
      city: Madrid
      state: Madrid
      country: ES
      continent: Europe
    endpoints:
      - protocol: https
        address: tunnel.yourdomain.com:4443
        capacity: 1000
        priority: 1
      - protocol: tcp
        address: tunnel.yourdomain.com:5443
        capacity: 1000
        priority: 1
    status: active
    tags: [production]
```

### 2. Build with Custom Configuration

```bash
# Build with absolute path
LOCALUP_RELAYS_CONFIG=/full/path/to/my-relays.yaml cargo build --release -p tunnel-cli

# Or from current directory
LOCALUP_RELAYS_CONFIG="$(pwd)/my-relays.yaml" cargo build --release -p tunnel-cli
```

### 3. Verify

The build output will show which configuration was used:

```
warning: tunnel-client@0.1.0: 📡 Using relay configuration from: /path/to/my-relays.yaml
```

### 4. Binary Location

```bash
ls -lh target/release/localup
```

The binary is **~20MB** and includes:
- ✅ Embedded relay configuration
- ✅ Web dashboard assets
- ✅ All dependencies

## Default Behavior

If `LOCALUP_RELAYS_CONFIG` is **not set**, the build uses:
```
/Users/davidviejo/projects/kfs/localup-dev/relays.yaml
```

This is the default configuration with `tunnel.kfs.es` endpoints.

## Build Script

Create `build.sh`:

```bash
#!/bin/bash
set -e

RELAY_CONFIG="${1:-relays.yaml}"

if [ ! -f "$RELAY_CONFIG" ]; then
    echo "❌ Configuration not found: $RELAY_CONFIG"
    echo "Usage: ./build.sh [path-to-relays.yaml]"
    exit 1
fi

# Get absolute path
RELAY_CONFIG_ABS="$(cd "$(dirname "$RELAY_CONFIG")" && pwd)/$(basename "$RELAY_CONFIG")"

echo "🔨 Building LocalUp with: $RELAY_CONFIG_ABS"

# Build
LOCALUP_RELAYS_CONFIG="$RELAY_CONFIG_ABS" cargo build --release -p tunnel-cli

# Done
echo "✅ Binary: target/release/localup"
ls -lh target/release/localup
```

Usage:
```bash
chmod +x build.sh

# Use default relays.yaml
./build.sh

# Use custom config
./build.sh my-custom-relays.yaml
```

## Distribution

```bash
# Create tarball
tar -czf localup-custom-$(date +%Y%m%d).tar.gz -C target/release localup

# Generate checksum
shasum -a 256 target/release/localup > localup.sha256

# Sign (macOS)
codesign -s "Developer ID Application: Your Name" target/release/localup
```

## Security

**Keep your custom relay configurations private!**

Custom relay configs are in `.gitignore`:
- `relays-*.yaml`
- `*-relays.yaml`
- `my-relays.yaml`
- `custom-relays.yaml`

Only distribute the compiled binary, not the YAML configuration source.

## Examples

### Single Production Relay

```yaml
version: 1
config:
  default_protocol: https
  connection_timeout: 30
  health_check_interval: 60

relays:
  - id: prod-1
    name: Production Relay
    region: global
    location: {city: Cloud, state: Cloud, country: Global, continent: Global}
    endpoints:
      - {protocol: https, address: tunnel.example.com:443, capacity: 1000, priority: 1}
      - {protocol: tcp, address: tunnel.example.com:8080, capacity: 1000, priority: 1}
    status: active
    tags: [production]

region_groups:
  - {name: Global, regions: [global], fallback_order: [global]}

selection_policies:
  auto:
    prefer_same_region: true
    fallback_to_nearest: false
    consider_capacity: true
    only_active: true
    include_tags: [production]
```

Build:
```bash
LOCALUP_RELAYS_CONFIG=prod-relays.yaml cargo build --release -p tunnel-cli
```

## Troubleshooting

### Error: File not found

```
❌ ERROR: Relay configuration file not found at: my-relays.yaml
```

**Solution:** Use absolute path:
```bash
LOCALUP_RELAYS_CONFIG="$(pwd)/my-relays.yaml" cargo build --release -p tunnel-cli
```

### Rebuild After Config Changes

The build system automatically detects changes:

```bash
# Edit config
vim my-relays.yaml

# Rebuild (will detect changes)
LOCALUP_RELAYS_CONFIG="$(pwd)/my-relays.yaml" cargo build --release -p tunnel-cli
```

## Cross-Platform Builds

### Linux

```bash
rustup target add x86_64-unknown-linux-gnu
LOCALUP_RELAYS_CONFIG="$(pwd)/my-relays.yaml" \
  cargo build --release --target x86_64-unknown-linux-gnu -p tunnel-cli
```

### macOS (Intel)

```bash
rustup target add x86_64-apple-darwin
LOCALUP_RELAYS_CONFIG="$(pwd)/my-relays.yaml" \
  cargo build --release --target x86_64-apple-darwin -p tunnel-cli
```

### macOS (Apple Silicon)

```bash
rustup target add aarch64-apple-darwin
LOCALUP_RELAYS_CONFIG="$(pwd)/my-relays.yaml" \
  cargo build --release --target aarch64-apple-darwin -p tunnel-cli
```

---

📖 **Full Documentation:** See [docs/custom-relay-config.md](docs/custom-relay-config.md)
