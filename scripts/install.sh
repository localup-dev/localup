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

LATEST_VERSION=""

if command -v gh &> /dev/null; then
  # Use GitHub CLI (most reliable)
  # gh output: "Release name\tType\tTag\tDate"
  # Extract column 3 which is the tag (e.g., v0.0.1-beta14)
  LATEST_VERSION=$(gh release list --repo localup-dev/localup --limit 1 2>/dev/null | awk -F'\t' '{print $3}' | head -1)
fi

# Fallback to curl if gh not available or failed
if [ -z "$LATEST_VERSION" ]; then
  # Fetch release list (includes prereleases) and get the first one (most recent)
  # Using /releases endpoint instead of /releases/latest to include prereleases
  RELEASE_JSON=$(curl -s "https://api.github.com/repos/localup-dev/localup/releases?per_page=1")

  if command -v jq &> /dev/null; then
    LATEST_VERSION=$(echo "$RELEASE_JSON" | jq -r '.[0].tag_name' 2>/dev/null)
  else
    # Fallback: use grep and more careful parsing for JSON
    # Extract the first tag_name from the releases list
    LATEST_VERSION=$(echo "$RELEASE_JSON" | tr ',' '\n' | grep '"tag_name"' | cut -d'"' -f4 | head -1)
  fi
fi

# Validate version format (should start with v, e.g., v0.0.1-beta13)
if [ -z "$LATEST_VERSION" ] || [ "$LATEST_VERSION" = "null" ]; then
  echo -e "${RED}❌ Could not fetch latest release version${NC}"
  echo -e "${YELLOW}ℹ️  GitHub API error or rate limit reached${NC}"
  echo -e "${YELLOW}Try again later or install manually from:${NC}"
  echo -e "${BLUE}  https://github.com/localup-dev/localup/releases${NC}"
  exit 1
fi

# Additional validation: ensure version looks valid (starts with v)
if ! [[ "$LATEST_VERSION" =~ ^v[0-9] ]]; then
  echo -e "${RED}❌ Invalid version format: $LATEST_VERSION${NC}"
  echo -e "${YELLOW}Expected format like: v0.0.1, v1.0.0, etc.${NC}"
  exit 1
fi

echo -e "  Latest version: ${GREEN}$LATEST_VERSION${NC}"
echo ""

# Construct download URLs
TUNNEL_FILE="localup-${PLATFORM}-${ARCH_NAME}.${EXT}"
RELAY_FILE="localup-relay-${PLATFORM}-${ARCH_NAME}.${EXT}"
AGENT_FILE="localup-agent-server-${PLATFORM}-${ARCH_NAME}.${EXT}"
CHECKSUMS_FILE="checksums-${PLATFORM}-${ARCH_NAME}.txt"

BASE_URL="https://github.com/localup-dev/localup/releases/download/${LATEST_VERSION}"

TUNNEL_URL="${BASE_URL}/${TUNNEL_FILE}"
RELAY_URL="${BASE_URL}/${RELAY_FILE}"
AGENT_URL="${BASE_URL}/${AGENT_FILE}"
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

echo -e "  Downloading agent server..."
if curl -L -f -o "$AGENT_FILE" "$AGENT_URL"; then
  echo -e "  ${GREEN}✓ Downloaded $AGENT_FILE${NC}"
else
  echo -e "  ${RED}✗ Failed to download $AGENT_FILE${NC}"
  echo -e "  ${YELLOW}⚠️  Agent server may not be available in this release${NC}"
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
  if [ -f "$AGENT_FILE" ]; then
    tar -xzf "$AGENT_FILE"
  fi
  echo -e "  ${GREEN}✓ Extracted binaries${NC}"
elif [ "$EXT" = "zip" ]; then
  unzip -q "$TUNNEL_FILE"
  unzip -q "$RELAY_FILE"
  if [ -f "$AGENT_FILE" ]; then
    unzip -q "$AGENT_FILE"
  fi
  echo -e "  ${GREEN}✓ Extracted binaries${NC}"
fi

echo ""

# Make binaries executable on Unix
if [ "$PLATFORM" != "windows" ]; then
  chmod +x localup localup-relay
  if [ -f "localup-agent-server" ]; then
    chmod +x localup-agent-server
  fi
fi

# Show installation summary
echo -e "${GREEN}═══════════════════════════════════════════════${NC}"
echo -e "${GREEN}✅ Download complete!${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════${NC}"
echo ""
echo -e "${YELLOW}📂 Files downloaded to:${NC} $(pwd)"
echo ""

# Auto-install binaries to PATH on Unix platforms
if [ "$PLATFORM" = "linux" ] || [ "$PLATFORM" = "macos" ]; then
  echo -e "${YELLOW}📋 Installing binaries to /usr/local/bin/...${NC}"
  echo ""

  # Collect binaries to install
  BINS_TO_INSTALL="localup localup-relay"
  if [ -f "localup-agent-server" ]; then
    BINS_TO_INSTALL="$BINS_TO_INSTALL localup-agent-server"
  fi

  # Attempt installation with sudo
  if sudo -n true 2>/dev/null; then
    # User has sudo without password prompt
    echo -e "  Installing: $BINS_TO_INSTALL"
    if sudo mv $BINS_TO_INSTALL /usr/local/bin/ 2>/dev/null; then
      echo -e "  ${GREEN}✓ Binaries installed to /usr/local/bin/${NC}"
    else
      echo -e "  ${RED}✗ Failed to install with sudo${NC}"
      echo -e "  ${YELLOW}You can manually install by running:${NC}"
      echo -e "  sudo mv $BINS_TO_INSTALL /usr/local/bin/"
      exit 1
    fi
  else
    # Prompt for password
    echo -e "  ${BLUE}Sudo password required to install to /usr/local/bin/${NC}"
    if sudo mv $BINS_TO_INSTALL /usr/local/bin/; then
      echo -e "  ${GREEN}✓ Binaries installed to /usr/local/bin/${NC}"
    else
      echo -e "  ${RED}✗ Installation failed${NC}"
      echo -e "  ${YELLOW}You can manually install by running:${NC}"
      echo -e "  sudo mv $BINS_TO_INSTALL /usr/local/bin/"
      exit 1
    fi
  fi

  echo ""

  # Verify installation
  echo -e "${YELLOW}🔍 Verifying installation...${NC}"
  INSTALL_FAILED=0

  if command -v localup &> /dev/null; then
    echo -e "  ${GREEN}✓ localup${NC}"
  else
    echo -e "  ${RED}✗ localup not found in PATH${NC}"
    INSTALL_FAILED=1
  fi

  if command -v localup-relay &> /dev/null; then
    echo -e "  ${GREEN}✓ localup-relay${NC}"
  else
    echo -e "  ${RED}✗ localup-relay not found in PATH${NC}"
    INSTALL_FAILED=1
  fi

  if [ -f "/usr/local/bin/localup-agent-server" ]; then
    if command -v localup-agent-server &> /dev/null; then
      echo -e "  ${GREEN}✓ localup-agent-server${NC}"
    else
      echo -e "  ${RED}✗ localup-agent-server not found in PATH${NC}"
      INSTALL_FAILED=1
    fi
  fi

  echo ""

  if [ $INSTALL_FAILED -eq 0 ]; then
    echo -e "${GREEN}═══════════════════════════════════════════════${NC}"
    echo -e "${GREEN}✅ Installation successful!${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${YELLOW}🚀 Quick start:${NC}"
    echo -e "  ${BLUE}# Start relay server${NC}"
    echo -e "  localup-relay"
    echo ""
    echo -e "  ${BLUE}# Create tunnel (in another terminal)${NC}"
    echo -e "  localup http --port 3000 --relay localhost:4443"
    echo ""
  else
    echo -e "${YELLOW}⚠️  Some binaries may not be in PATH${NC}"
    echo -e "You can verify with: localup --version"
    echo ""
  fi
else
  # Windows instructions
  echo -e "${YELLOW}📋 Next steps:${NC}"
  echo ""
  echo -e "  ${BLUE}# Run binaries:${NC}"
  echo -e "  .\\localup.exe --version"
  echo -e "  .\\localup-relay.exe --version"
  if [ -f "localup-agent-server.exe" ]; then
    echo -e "  .\\localup-agent-server.exe --version"
  fi
  echo ""
  echo -e "  ${BLUE}# Add to PATH or move to desired location${NC}"
  echo ""
  echo -e "${YELLOW}📂 Files location:${NC} $(pwd)"
  echo ""
  echo -e "${YELLOW}🚀 Quick start:${NC}"
  echo -e "  ${BLUE}# Start relay server${NC}"
  echo -e "  .\\localup-relay.exe"
  echo ""
  echo -e "  ${BLUE}# Create tunnel (in another terminal)${NC}"
  echo -e "  .\\localup.exe http --port 3000 --relay localhost:4443"
  echo ""
fi
