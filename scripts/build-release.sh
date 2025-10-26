#!/bin/bash
set -euo pipefail

# Build script for creating Homebrew release binaries
# Usage: ./scripts/build-release.sh [version]

VERSION="${1:-0.1.0}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${REPO_ROOT}/dist"

echo "🚀 Building Localup v${VERSION} release binaries..."

# Detect platform
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

# Normalize architecture names
case "$ARCH" in
  x86_64)
    ARCH="amd64"
    ;;
  aarch64|arm64)
    ARCH="arm64"
    ;;
esac

PLATFORM="${OS}-${ARCH}"
OUTPUT_NAME="localup-${PLATFORM}.tar.gz"

echo "📦 Platform: ${PLATFORM}"
echo "📁 Output: ${OUTPUT_NAME}"

# Create dist directory
mkdir -p "${DIST_DIR}"

# Build binaries
echo "🔨 Building binaries..."
cd "${REPO_ROOT}/crates/tunnel-cli"
cargo build --release

cd "${REPO_ROOT}/crates/tunnel-exit-node"
cargo build --release

# Create tarball
echo "📦 Creating tarball..."
cd "${REPO_ROOT}"
tar -czf "${DIST_DIR}/${OUTPUT_NAME}" \
  -C target/release \
  tunnel-cli \
  tunnel-exit-node

# Calculate checksum
echo "🔐 Calculating SHA256..."
cd "${DIST_DIR}"
shasum -a 256 "${OUTPUT_NAME}" > "${OUTPUT_NAME}.sha256"

echo "✅ Build complete!"
echo ""
echo "📦 Tarball: ${DIST_DIR}/${OUTPUT_NAME}"
echo "🔐 Checksum: ${DIST_DIR}/${OUTPUT_NAME}.sha256"
echo ""
echo "SHA256:"
cat "${OUTPUT_NAME}.sha256"
echo ""
echo "Next steps:"
echo "1. Upload ${OUTPUT_NAME} to GitHub Releases"
echo "2. Update SHA256 in Formula/localup.rb"
echo "3. Update version in Formula/localup.rb"
