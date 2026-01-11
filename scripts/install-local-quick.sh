#!/bin/bash
# Quick install of locally built binaries (no prompts)
# Usage: ./scripts/install-local-quick.sh

set -e

# Check if running from project root
if [ ! -f "Cargo.toml" ]; then
    echo "Error: Must run from project root directory"
    exit 1
fi

# Build if needed
LOCALUP_BIN="target/release/localup"
RELAY_BIN="target/release/localup-relay"

if [ ! -f "$LOCALUP_BIN" ] || [ ! -f "$RELAY_BIN" ]; then
    echo "Building release binaries..."
    cargo build --release -p tunnel-cli -p tunnel-exit-node
fi

# Install
echo "Installing to /usr/local/bin..."
sudo cp "$LOCALUP_BIN" /usr/local/bin/localup
sudo cp "$RELAY_BIN" /usr/local/bin/localup-relay
sudo chmod +x /usr/local/bin/localup /usr/local/bin/localup-relay

echo ""
echo "✅ Installed successfully!"
echo ""
localup --version
localup-relay --version
