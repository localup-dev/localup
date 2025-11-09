#!/bin/bash

# Installation script for localup
# Builds from source and installs the localup binary to /usr/local/bin with sudo
# Supports macOS and Linux

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
INSTALL_PREFIX="${INSTALL_PREFIX:-/usr/local}"
INSTALL_DIR="${INSTALL_PREFIX}/bin"
RELEASE_DIR="target/release"

# Binary to install
declare -a BINARIES=(
  "localup"           # Unified CLI with all subcommands (relay, connect, agent-server, etc.)
)

# Display header
echo -e "${BLUE}╔════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║   Localup - Build & Install from Source                ║${NC}"
echo -e "${BLUE}║   Installing to: ${INSTALL_DIR}${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check if running on macOS or Linux
PLATFORM=$(uname)
case "$PLATFORM" in
  Darwin)
    echo -e "${GREEN}✓ Detected macOS${NC}"
    ;;
  Linux)
    echo -e "${GREEN}✓ Detected Linux${NC}"
    ;;
  *)
    echo -e "${RED}✗ Unsupported platform: $PLATFORM${NC}"
    echo "This script supports macOS and Linux only."
    exit 1
    ;;
esac
echo ""

# Check if Cargo is installed
if ! command -v cargo &> /dev/null; then
  echo -e "${RED}✗ Cargo not found${NC}"
  echo "Please install Rust from https://rustup.rs/"
  exit 1
fi
echo -e "${GREEN}✓ Cargo found at: $(which cargo)${NC}"
echo ""

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ] || [ ! -d "crates" ]; then
  echo -e "${RED}✗ Not in localup root directory${NC}"
  echo "Please run this script from the root of the localup repository"
  exit 1
fi
echo -e "${GREEN}✓ Running from correct directory${NC}"
echo ""

# Build localup binary in release mode
echo -e "${YELLOW}→ Building localup in release mode...${NC}"
echo "(This may take a few minutes on first build)"
echo ""

if cargo build --release 2>&1 | tail -20; then
  echo ""
  echo -e "${GREEN}✓ Build completed successfully${NC}"
else
  echo -e "${RED}✗ Build failed${NC}"
  exit 1
fi
echo ""

# Verify binary exists
echo -e "${YELLOW}→ Verifying built binary...${NC}"
missing_binaries=0
for binary in "${BINARIES[@]}"; do
  if [ -f "${RELEASE_DIR}/${binary}" ]; then
    size=$(du -h "${RELEASE_DIR}/${binary}" | cut -f1)
    echo -e "${GREEN}✓${NC} ${binary} (${size})"
  else
    echo -e "${YELLOW}⚠${NC} ${binary} (not found, will skip)"
    missing_binaries=$((missing_binaries + 1))
  fi
done
echo ""

if [ $missing_binaries -eq ${#BINARIES[@]} ]; then
  echo -e "${RED}✗ No binaries found!${NC}"
  exit 1
fi

# Create install directory if it doesn't exist
if [ ! -d "${INSTALL_DIR}" ]; then
  echo -e "${YELLOW}→ Creating directory: ${INSTALL_DIR}${NC}"
  sudo mkdir -p "${INSTALL_DIR}"
  echo -e "${GREEN}✓ Directory created${NC}"
  echo ""
fi

# Install binary
echo -e "${YELLOW}→ Installing localup to ${INSTALL_DIR}...${NC}"
echo "(You may be prompted for your password)"
echo ""

for binary in "${BINARIES[@]}"; do
  if [ -f "${RELEASE_DIR}/${binary}" ]; then
    echo -n "  Installing ${binary}... "
    if sudo cp "${RELEASE_DIR}/${binary}" "${INSTALL_DIR}/${binary}" && \
       sudo chmod +x "${INSTALL_DIR}/${binary}"; then
      echo -e "${GREEN}done${NC}"
    else
      echo -e "${RED}failed${NC}"
      exit 1
    fi
  fi
done
echo ""

# Verify installation
echo -e "${YELLOW}→ Verifying installation...${NC}"
failed=0
installed=0
for binary in "${BINARIES[@]}"; do
  if [ -f "${INSTALL_DIR}/${binary}" ]; then
    if command -v "${binary}" &> /dev/null; then
      binary_path=$(command -v "${binary}")
      version_output=$(${binary} --version 2>&1 || echo "no version info")
      echo -e "${GREEN}✓${NC} ${binary}"
      echo "    Location: ${binary_path}"
      if [ "$version_output" != "no version info" ]; then
        echo "    $version_output"
      fi
      installed=$((installed + 1))
    else
      echo -e "${RED}✗${NC} ${binary} installed but not in PATH"
      failed=$((failed + 1))
    fi
  fi
done
echo ""

if [ $failed -gt 0 ]; then
  echo -e "${YELLOW}⚠ Warning: ${binary} installed but not found in PATH${NC}"
  echo "  This might be because your PATH doesn't include ${INSTALL_DIR}"
  echo "  Make sure ${INSTALL_DIR} is in your PATH environment variable"
  echo ""
  if [ -f ~/.bashrc ]; then
    echo "  Add this to ~/.bashrc or ~/.zshrc:"
    echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
  fi
  echo ""
fi

# Success!
echo -e "${BLUE}╔════════════════════════════════════════════════════════╗${NC}"
if [ $installed -gt 0 ]; then
  echo -e "${GREEN}✓ Installation completed!${NC}"
  echo -e "${BLUE}╚════════════════════════════════════════════════════════╝${NC}"
  echo ""
  echo "Installed:"
  for binary in "${BINARIES[@]}"; do
    if [ -f "${INSTALL_DIR}/${binary}" ]; then
      echo "  • ${binary}"
    fi
  done
  echo ""
  echo "Quick start - Available subcommands:"
  echo "  ${BLUE}localup --help${NC}                    # Show all commands"
  echo "  ${BLUE}localup relay --help${NC}              # Run as relay server"
  echo "  ${BLUE}localup --port 3000 --relay ...${NC}   # Create a tunnel (standalone mode)"
  echo "  ${BLUE}localup generate-token --help${NC}     # Generate JWT auth tokens"
  echo ""
  echo "Full documentation: https://github.com/localup-dev/localup"
  echo ""
else
  echo -e "${RED}✗ Installation failed${NC}"
  echo -e "${BLUE}╚════════════════════════════════════════════════════════╝${NC}"
  exit 1
fi

exit 0
