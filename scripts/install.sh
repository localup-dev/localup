#!/bin/bash
# Installation script for tunnel CLI
# Usage: curl -sSL https://raw.githubusercontent.com/OWNER/REPO/main/scripts/install.sh | bash

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Configuration
GITHUB_REPO="localup-dev/localup"  # Replace with actual repo
BINARY_NAME="tunnel"
INSTALL_DIR="/usr/local/bin"
TEMP_DIR=$(mktemp -d)

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

echo -e "${BLUE}═══════════════════════════════════════${NC}"
echo -e "${BLUE}  Tunnel Installation Script${NC}"
echo -e "${BLUE}═══════════════════════════════════════${NC}"
echo ""

# Check OS and architecture
if [ "$OS" != "linux" ]; then
    echo -e "${RED}Error: This script only supports Linux${NC}"
    echo "For other platforms, please build from source or download from releases"
    exit 1
fi

if [ "$ARCH" != "x86_64" ]; then
    echo -e "${RED}Error: This script only supports x86_64 (AMD64) architecture${NC}"
    echo "Detected architecture: $ARCH"
    exit 1
fi

echo -e "${GREEN}✓ Platform detected: Linux AMD64${NC}"
echo ""

# Check for required tools
echo -e "${BLUE}Checking dependencies...${NC}"
for cmd in curl tar sha256sum; do
    if ! command -v $cmd &> /dev/null; then
        echo -e "${RED}Error: $cmd is required but not installed${NC}"
        exit 1
    fi
    echo -e "${GREEN}✓ $cmd found${NC}"
done
echo ""

# Check if we need sudo
NEED_SUDO=false
if [ ! -w "$INSTALL_DIR" ]; then
    NEED_SUDO=true
    echo -e "${YELLOW}Note: sudo is required to install to $INSTALL_DIR${NC}"
    echo -e "${YELLOW}Alternatively, you can install to ~/.local/bin without sudo${NC}"
    echo ""
    read -p "Install to $INSTALL_DIR with sudo? (y/n): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        INSTALL_DIR="$HOME/.local/bin"
        mkdir -p "$INSTALL_DIR"
        echo -e "${BLUE}Installing to $INSTALL_DIR${NC}"
        echo ""
    fi
fi

# Get latest release version
echo -e "${BLUE}Fetching latest release...${NC}"
LATEST_RELEASE=$(curl -s https://api.github.com/repos/$GITHUB_REPO/releases/latest | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_RELEASE" ]; then
    echo -e "${RED}Error: Could not fetch latest release${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Latest version: $LATEST_RELEASE${NC}"
echo ""

# Download binary
DOWNLOAD_URL="https://github.com/$GITHUB_REPO/releases/download/$LATEST_RELEASE/tunnel-linux-amd64.tar.gz"
CHECKSUM_URL="https://github.com/$GITHUB_REPO/releases/download/$LATEST_RELEASE/checksums-linux-amd64.txt"

echo -e "${BLUE}Downloading $BINARY_NAME...${NC}"
cd "$TEMP_DIR"

if ! curl -sL "$DOWNLOAD_URL" -o tunnel-linux-amd64.tar.gz; then
    echo -e "${RED}Error: Failed to download binary${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Downloaded${NC}"
echo ""

# Download and verify checksum
echo -e "${BLUE}Verifying checksum...${NC}"
if ! curl -sL "$CHECKSUM_URL" -o checksums.txt; then
    echo -e "${YELLOW}Warning: Could not download checksums, skipping verification${NC}"
else
    if sha256sum -c checksums.txt --ignore-missing --status; then
        echo -e "${GREEN}✓ Checksum verified${NC}"
    else
        echo -e "${RED}Error: Checksum verification failed${NC}"
        echo "This could indicate a corrupted download or security issue"
        exit 1
    fi
fi
echo ""

# Extract
echo -e "${BLUE}Extracting...${NC}"
tar -xzf tunnel-linux-amd64.tar.gz
echo -e "${GREEN}✓ Extracted${NC}"
echo ""

# Install
echo -e "${BLUE}Installing to $INSTALL_DIR...${NC}"
if [ "$NEED_SUDO" = true ] && [ "$INSTALL_DIR" = "/usr/local/bin" ]; then
    sudo mv tunnel "$INSTALL_DIR/"
    sudo chmod +x "$INSTALL_DIR/tunnel"
else
    mv tunnel "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/tunnel"
fi

echo -e "${GREEN}✓ Installed${NC}"
echo ""

# Cleanup
cd - > /dev/null
rm -rf "$TEMP_DIR"

# Verify installation
echo -e "${BLUE}Verifying installation...${NC}"
if command -v tunnel &> /dev/null; then
    VERSION=$(tunnel --version 2>&1 || echo "unknown")
    echo -e "${GREEN}✓ tunnel successfully installed!${NC}"
    echo ""
    echo -e "${BLUE}═══════════════════════════════════════${NC}"
    echo -e "${GREEN}  Installation Complete!${NC}"
    echo -e "${BLUE}═══════════════════════════════════════${NC}"
    echo ""
    echo "Installed to: $INSTALL_DIR/tunnel"
    echo "Version: $VERSION"
    echo ""
    echo "Get started:"
    echo "  tunnel --port 3000 --token YOUR_TOKEN"
    echo ""
    echo "For help:"
    echo "  tunnel --help"
else
    echo -e "${YELLOW}Warning: tunnel command not found in PATH${NC}"
    echo ""
    echo "The binary was installed to: $INSTALL_DIR/tunnel"
    echo ""
    if [ "$INSTALL_DIR" = "$HOME/.local/bin" ]; then
        echo "Add to your PATH by adding this line to ~/.bashrc or ~/.zshrc:"
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
        echo "Then reload your shell:"
        echo "  source ~/.bashrc  # or source ~/.zshrc"
    fi
fi
