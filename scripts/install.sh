#!/bin/bash
# Download the latest localup release for your platform
# Usage: ./scripts/install.sh
# Or: curl -fsSL https://raw.githubusercontent.com/localup-dev/localup/main/scripts/install.sh | bash

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Localup - Download Latest Release${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo ""

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  linux*)
    PLATFORM="linux"
    EXT="tar.gz"
    ;;
  darwin*)
    PLATFORM="macos"
    EXT="tar.gz"
    ;;
  mingw* | msys* | cygwin*)
    PLATFORM="windows"
    EXT="zip"
    ;;
  *)
    echo -e "${RED}❌ Unsupported OS: $OS${NC}"
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64 | amd64)
    ARCH_NAME="amd64"
    ;;
  aarch64 | arm64)
    ARCH_NAME="arm64"
    ;;
  *)
    echo -e "${RED}❌ Unsupported architecture: $ARCH${NC}"
    exit 1
    ;;
esac

echo -e "${GREEN}📋 Detected platform:${NC}"
echo -e "  OS: $PLATFORM"
echo -e "  Architecture: $ARCH_NAME"
echo ""

# Get latest release version
echo -e "${YELLOW}🔍 Fetching latest release...${NC}"

if command -v gh &> /dev/null; then
  # Use GitHub CLI
  LATEST_VERSION=$(gh release list --repo localup-dev/localup --limit 1 | awk '{print $1}' | head -1)
else
  # Use curl and GitHub API
  LATEST_VERSION=$(curl -s https://api.github.com/repos/localup-dev/localup/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
fi

if [ -z "$LATEST_VERSION" ]; then
  echo -e "${RED}❌ Could not fetch latest release version${NC}"
  exit 1
fi

echo -e "  Latest version: ${GREEN}$LATEST_VERSION${NC}"
echo ""

# Construct download URLs
TUNNEL_FILE="localup-${PLATFORM}-${ARCH_NAME}.${EXT}"
RELAY_FILE="localup-relay-${PLATFORM}-${ARCH_NAME}.${EXT}"
CHECKSUMS_FILE="checksums-${PLATFORM}-${ARCH_NAME}.txt"

BASE_URL="https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}"

TUNNEL_URL="${BASE_URL}/${TUNNEL_FILE}"
RELAY_URL="${BASE_URL}/${RELAY_FILE}"
CHECKSUMS_URL="${BASE_URL}/${CHECKSUMS_FILE}"

# Create download directory
DOWNLOAD_DIR="localup-${LATEST_VERSION}"
mkdir -p "$DOWNLOAD_DIR"
cd "$DOWNLOAD_DIR"

echo -e "${YELLOW}📥 Downloading binaries...${NC}"

# Download files
echo -e "  Downloading tunnel CLI..."
if curl -L -f -o "$TUNNEL_FILE" "$TUNNEL_URL"; then
  echo -e "  ${GREEN}✓ Downloaded $TUNNEL_FILE${NC}"
else
  echo -e "  ${RED}✗ Failed to download $TUNNEL_FILE${NC}"
  exit 1
fi

echo -e "  Downloading relay server..."
if curl -L -f -o "$RELAY_FILE" "$RELAY_URL"; then
  echo -e "  ${GREEN}✓ Downloaded $RELAY_FILE${NC}"
else
  echo -e "  ${RED}✗ Failed to download $RELAY_FILE${NC}"
  exit 1
fi

echo -e "  Downloading checksums..."
if curl -L -f -o "$CHECKSUMS_FILE" "$CHECKSUMS_URL"; then
  echo -e "  ${GREEN}✓ Downloaded $CHECKSUMS_FILE${NC}"
else
  echo -e "  ${YELLOW}⚠️  Checksums file not available${NC}"
fi

echo ""

# Verify checksums if available
if [ -f "$CHECKSUMS_FILE" ]; then
  echo -e "${YELLOW}🔐 Verifying checksums...${NC}"

  if command -v sha256sum &> /dev/null; then
    if sha256sum -c "$CHECKSUMS_FILE" 2>/dev/null; then
      echo -e "  ${GREEN}✓ Checksums verified${NC}"
    else
      echo -e "  ${YELLOW}⚠️  Checksum verification failed${NC}"
      echo -e "  ${YELLOW}   Files downloaded but may be corrupted${NC}"
    fi
  elif command -v shasum &> /dev/null; then
    if shasum -a 256 -c "$CHECKSUMS_FILE" 2>/dev/null; then
      echo -e "  ${GREEN}✓ Checksums verified${NC}"
    else
      echo -e "  ${YELLOW}⚠️  Checksum verification failed${NC}"
    fi
  else
    echo -e "  ${YELLOW}⚠️  sha256sum not found, skipping verification${NC}"
  fi
  echo ""
fi

# Extract archives
echo -e "${YELLOW}📦 Extracting archives...${NC}"

if [ "$EXT" = "tar.gz" ]; then
  tar -xzf "$TUNNEL_FILE"
  tar -xzf "$RELAY_FILE"
  echo -e "  ${GREEN}✓ Extracted binaries${NC}"
elif [ "$EXT" = "zip" ]; then
  unzip -q "$TUNNEL_FILE"
  unzip -q "$RELAY_FILE"
  echo -e "  ${GREEN}✓ Extracted binaries${NC}"
fi

echo ""

# Make binaries executable on Unix
if [ "$PLATFORM" != "windows" ]; then
  chmod +x localup localup-relay
fi

# Show installation instructions
echo -e "${GREEN}═══════════════════════════════════════════════${NC}"
echo -e "${GREEN}✅ Download complete!${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════${NC}"
echo ""
echo -e "${YELLOW}📂 Files downloaded to:${NC} $(pwd)"
echo ""
echo -e "${YELLOW}📋 Next steps:${NC}"
echo ""

if [ "$PLATFORM" = "linux" ] || [ "$PLATFORM" = "macos" ]; then
  echo -e "  ${BLUE}# Install to system:${NC}"
  echo -e "  sudo mv localup localup-relay /usr/local/bin/"
  echo ""
  echo -e "  ${BLUE}# Or run from current directory:${NC}"
  echo -e "  ./localup --version"
  echo -e "  ./localup-relay --version"
else
  echo -e "  ${BLUE}# Run binaries:${NC}"
  echo -e "  .\\localup.exe --version"
  echo -e "  .\\localup-relay.exe --version"
  echo ""
  echo -e "  ${BLUE}# Add to PATH or move to desired location${NC}"
fi

echo ""
echo -e "${YELLOW}🚀 Quick start:${NC}"
echo -e "  ${BLUE}# Start relay server${NC}"
echo -e "  localup-relay"
echo ""
echo -e "  ${BLUE}# Create tunnel (in another terminal)${NC}"
echo -e "  localup http --port 3000 --relay localhost:4443"
echo ""
