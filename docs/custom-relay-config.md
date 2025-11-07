# Custom Relay Configuration

This guide explains how to build LocalUp with a custom relay configuration embedded in the binary.

## Overview

LocalUp embeds the relay server configuration at **compile time** using the `relays.yaml` file. You can specify a custom configuration file using the `LOCALUP_RELAYS_CONFIG` environment variable when building.

## Quick Start

### 1. Create Your Custom Relay Configuration

```bash
# Copy the example
cp relays.example.yaml my-custom-relays.yaml

# Edit with your relay servers
vim my-custom-relays.yaml
```

### 2. Build with Custom Configuration

```bash
# Build with your custom relay config
LOCALUP_RELAYS_CONFIG=my-custom-relays.yaml cargo build --release -p localup-cli

# The binary will be at: target/release/localup
```

### 3. Verify the Configuration

The build will show which configuration file was used:

```
warning: localup-client@0.1.0: 📡 Using relay configuration from: /path/to/my-custom-relays.yaml
```

## Configuration File Format

Your custom relay configuration must follow the YAML schema:

```yaml
version: 1

config:
  default_protocol: https
  connection_timeout: 30
  health_check_interval: 60

relays:
  - id: unique-relay-id
    name: Human-Readable Name
    region: region-code
    location:
      city: City Name
      state: State/Province
      country: Country Code
      continent: Continent Name
    endpoints:
      - protocol: https
        address: relay.yourdomain.com:443
        capacity: 1000
        priority: 1
      - protocol: tcp
        address: relay.yourdomain.com:8080
        capacity: 1000
        priority: 1
    status: active
    tags:
      - production

region_groups:
  - name: Region Group Name
    regions:
      - region-code
    fallback_order:
      - region-code

selection_policies:
  auto:
    prefer_same_region: true
    fallback_to_nearest: false
    consider_capacity: true
    only_active: true
    include_tags:
      - production
```

## Use Cases

### Private Deployment

Build a binary with only your private relay servers:

```yaml
# private-relays.yaml
relays:
  - id: my-private-relay
    name: My Private Relay
    region: eu-west
    endpoints:
      - protocol: https
        address: tunnel.mycompany.com:4443
      - protocol: tcp
        address: tunnel.mycompany.com:5443
    status: active
    tags: [production]
```

Build:
```bash
LOCALUP_RELAYS_CONFIG=private-relays.yaml cargo build --release -p localup-cli
```

### Multi-Region Deployment

Configure multiple relay servers across regions:

```yaml
relays:
  - id: us-east-1
    region: us-east
    endpoints:
      - protocol: https
        address: us-east.relay.example.com:443
      - protocol: tcp
        address: us-east.relay.example.com:8080
    status: active
    tags: [production]

  - id: eu-west-1
    region: eu-west
    endpoints:
      - protocol: https
        address: eu-west.relay.example.com:443
      - protocol: tcp
        address: eu-west.relay.example.com:8080
    status: active
    tags: [production]
```

### Staging vs Production Builds

**Production:**
```bash
LOCALUP_RELAYS_CONFIG=relays-production.yaml cargo build --release -p localup-cli
mv target/release/localup localup-production
```

**Staging:**
```bash
LOCALUP_RELAYS_CONFIG=relays-staging.yaml cargo build --release -p localup-cli
mv target/release/localup localup-staging
```

## Build Scripts

### Automated Build Script

Create `build-custom.sh`:

```bash
#!/bin/bash
set -e

RELAY_CONFIG="${1:-relays.yaml}"
VERSION="0.0.1-beta8"

if [ ! -f "$RELAY_CONFIG" ]; then
    echo "❌ Relay configuration not found: $RELAY_CONFIG"
    exit 1
fi

echo "🔨 Building LocalUp with relay config: $RELAY_CONFIG"

# Build with custom relay config
LOCALUP_RELAYS_CONFIG="$RELAY_CONFIG" cargo build --release -p localup-cli

# Get config name for output
CONFIG_NAME=$(basename "$RELAY_CONFIG" .yaml)

# Create distribution
mkdir -p dist
cp target/release/localup "dist/localup-${CONFIG_NAME}"

# Generate checksum
cd dist
shasum -a 256 "localup-${CONFIG_NAME}" > "localup-${CONFIG_NAME}.sha256"

echo "✅ Built: dist/localup-${CONFIG_NAME}"
echo "📋 Checksum: dist/localup-${CONFIG_NAME}.sha256"
```

Usage:
```bash
chmod +x build-custom.sh
./build-custom.sh my-custom-relays.yaml
```

## Validation

### Verify Embedded Configuration

The relay configuration is embedded at compile time, so it cannot be inspected from the binary. However, you can verify it works:

```bash
# Build a test binary
LOCALUP_RELAYS_CONFIG=my-relays.yaml cargo build -p localup-cli

# Run tests to verify config is valid
cargo test -p localup-client --lib relay_discovery

# Test the CLI (won't connect without running relay, but shows it's embedded)
./target/debug/localup --help
```

### Configuration Validation

The build script automatically validates that:
1. The configuration file exists
2. The file path is accessible
3. Cargo will fail if the YAML is malformed

For additional validation, you can test the YAML syntax:

```bash
# Using Python
python3 -c "import yaml; yaml.safe_load(open('my-relays.yaml'))"

# Using Ruby
ruby -ryaml -e "YAML.load_file('my-relays.yaml')"
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `LOCALUP_RELAYS_CONFIG` | Path to custom relay configuration file | `<workspace>/relays.yaml` |

## Troubleshooting

### Error: Relay configuration file not found

```
❌ ERROR: Relay configuration file not found at: /path/to/config.yaml
Set LOCALUP_RELAYS_CONFIG environment variable to specify a custom path.
```

**Solution:** Ensure the file exists and the path is correct:
```bash
ls -l my-relays.yaml
LOCALUP_RELAYS_CONFIG="$(pwd)/my-relays.yaml" cargo build --release -p localup-cli
```

### Error: Failed to parse relay configuration

This means your YAML is invalid. Run validation:
```bash
python3 -c "import yaml; yaml.safe_load(open('my-relays.yaml'))"
```

### Rebuild After Configuration Changes

The build system automatically detects changes to the relay configuration:

```bash
# First build
LOCALUP_RELAYS_CONFIG=my-relays.yaml cargo build --release -p localup-cli

# Edit the config
vim my-relays.yaml

# Rebuild (automatically detects changes)
LOCALUP_RELAYS_CONFIG=my-relays.yaml cargo build --release -p localup-cli
```

## Security Considerations

1. **Private Relay Servers:** Keep your custom relay configuration files private. Don't commit them to public repositories.

2. **Binary Distribution:** Since the configuration is embedded at compile time:
   - Users cannot modify relay servers without recompiling
   - Your relay server addresses are baked into the binary
   - This is ideal for private/enterprise deployments

3. **Version Control:**
   ```gitignore
   # .gitignore
   relays-production.yaml
   relays-staging.yaml
   my-relays.yaml
   *-relays.yaml
   ```

## Examples

### Minimal Configuration (Single Relay)

```yaml
version: 1
config:
  default_protocol: https
  connection_timeout: 30
  health_check_interval: 60

relays:
  - id: main
    name: Main Relay
    region: global
    location:
      city: Cloud
      state: Cloud
      country: Global
      continent: Global
    endpoints:
      - protocol: https
        address: tunnel.example.com:443
        capacity: 1000
        priority: 1
      - protocol: tcp
        address: tunnel.example.com:8080
        capacity: 1000
        priority: 1
    status: active
    tags: [production]

region_groups:
  - name: Global
    regions: [global]
    fallback_order: [global]

selection_policies:
  auto:
    prefer_same_region: true
    fallback_to_nearest: false
    consider_capacity: true
    only_active: true
    include_tags: [production]
```

### Development Configuration (Local Testing)

```yaml
version: 1
config:
  default_protocol: https
  connection_timeout: 10
  health_check_interval: 30

relays:
  - id: dev-local
    name: Development (Local)
    region: local
    location:
      city: Local
      state: Local
      country: Local
      continent: Local
    endpoints:
      - protocol: https
        address: localhost:8443
        capacity: 10
        priority: 1
      - protocol: tcp
        address: localhost:8080
        capacity: 10
        priority: 1
    status: active
    tags: [development]

region_groups:
  - name: Local
    regions: [local]
    fallback_order: [local]

selection_policies:
  auto:
    prefer_same_region: false
    fallback_to_nearest: false
    consider_capacity: false
    only_active: true
    include_tags: [development]
```

Build development version:
```bash
LOCALUP_RELAYS_CONFIG=relays-dev.yaml cargo build -p localup-cli
```

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Build Custom LocalUp

on:
  push:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Create Custom Relay Config
        env:
          RELAY_SERVER: ${{ secrets.RELAY_SERVER }}
        run: |
          cat > custom-relays.yaml <<EOF
          version: 1
          config:
            default_protocol: https
            connection_timeout: 30
            health_check_interval: 60
          relays:
            - id: production
              name: Production Relay
              region: global
              location: {city: Cloud, state: Cloud, country: Global, continent: Global}
              endpoints:
                - {protocol: https, address: "${RELAY_SERVER}:443", capacity: 1000, priority: 1}
                - {protocol: tcp, address: "${RELAY_SERVER}:8080", capacity: 1000, priority: 1}
              status: active
              tags: [production]
          region_groups:
            - {name: Global, regions: [global], fallback_order: [global]}
          selection_policies:
            auto: {prefer_same_region: true, fallback_to_nearest: false, consider_capacity: true, only_active: true, include_tags: [production]}
          EOF

      - name: Build with Custom Config
        env:
          LOCALUP_RELAYS_CONFIG: custom-relays.yaml
        run: cargo build --release -p localup-cli

      - name: Upload Binary
        uses: actions/upload-artifact@v3
        with:
          name: localup
          path: target/release/localup
```

---

**Note:** The default `relays.yaml` in the workspace root is used if `LOCALUP_RELAYS_CONFIG` is not set.
