#!/bin/bash
# Prepare daemon binary for Tauri bundling
# This script copies the daemon binary with the correct platform suffix

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
WORKSPACE_ROOT="$(cd "$PROJECT_DIR/../.." && pwd)"

# Build the daemon
echo "Building localup-daemon..."
cd "$WORKSPACE_ROOT"
cargo build --release -p localup-desktop --bin localup-daemon

# Determine target triple
case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)
        TARGET="aarch64-apple-darwin"
        ;;
    Darwin-x86_64)
        TARGET="x86_64-apple-darwin"
        ;;
    Linux-x86_64)
        TARGET="x86_64-unknown-linux-gnu"
        ;;
    Linux-aarch64)
        TARGET="aarch64-unknown-linux-gnu"
        ;;
    *)
        echo "Unsupported platform: $(uname -s)-$(uname -m)"
        exit 1
        ;;
esac

# Create binaries directory
mkdir -p "$PROJECT_DIR/src-tauri/binaries"

# Copy binary with platform suffix
DAEMON_SRC="$WORKSPACE_ROOT/target/release/localup-daemon"
DAEMON_DST="$PROJECT_DIR/src-tauri/binaries/localup-daemon-$TARGET"

echo "Copying $DAEMON_SRC to $DAEMON_DST"
cp "$DAEMON_SRC" "$DAEMON_DST"

echo "Done! Daemon binary ready for bundling."
