#!/bin/bash
# Install locally built binaries to /usr/local/bin
# Usage: ./scripts/install-local.sh

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Localup - Install from Local Build${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo ""

# Check if running from project root
if [ ! -f "Cargo.toml" ]; then
    echo -e "${RED}Error: Must run from project root directory${NC}"
    exit 1
fi

# Check if binaries exist
LOCALUP_BIN="target/release/localup"
RELAY_BIN="target/release/localup-relay"

if [ ! -f "$LOCALUP_BIN" ] || [ ! -f "$RELAY_BIN" ]; then
    echo -e "${YELLOW}Binaries not found. Building release binaries...${NC}"
    echo ""
    cargo build --release -p tunnel-cli -p tunnel-exit-node
    echo ""
fi

# Verify binaries exist after build
if [ ! -f "$LOCALUP_BIN" ] || [ ! -f "$RELAY_BIN" ]; then
    echo -e "${RED}Error: Failed to build binaries${NC}"
    exit 1
fi

# Show binary information
echo -e "${YELLOW}📦 Binary Information:${NC}"
echo ""
echo -e "  ${BLUE}localup:${NC}"
$LOCALUP_BIN --version | sed 's/^/    /'
echo ""
echo -e "  ${BLUE}localup-relay:${NC}"
$RELAY_BIN --version | sed 's/^/    /'
echo ""

# Confirm installation
echo -e "${YELLOW}This will install binaries to:${NC}"
echo -e "  /usr/local/bin/localup"
echo -e "  /usr/local/bin/localup-relay"
echo ""
read -p "Continue with installation? (y/N) " -n 1 -r
echo ""

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}Installation cancelled.${NC}"
    exit 0
fi

echo ""
echo -e "${YELLOW}🔧 Installing binaries...${NC}"

# Install localup
echo -e "  Installing localup..."
sudo cp "$LOCALUP_BIN" /usr/local/bin/localup
sudo chmod +x /usr/local/bin/localup

# Install localup-relay
echo -e "  Installing localup-relay..."
sudo cp "$RELAY_BIN" /usr/local/bin/localup-relay
sudo chmod +x /usr/local/bin/localup-relay

echo ""
echo -e "${GREEN}═══════════════════════════════════════════════${NC}"
echo -e "${GREEN}✅ Installation complete!${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════${NC}"
echo ""

# Verify installation
echo -e "${YELLOW}🔍 Verifying installation:${NC}"
echo ""
echo -e "  ${BLUE}localup:${NC}"
which localup
localup --version | sed 's/^/    /'
echo ""
echo -e "  ${BLUE}localup-relay:${NC}"
which localup-relay
localup-relay --version | sed 's/^/    /'
echo ""

# Show next steps
echo -e "${GREEN}🚀 Ready to use!${NC}"
echo ""
echo -e "${YELLOW}Quick start:${NC}"
echo -e "  ${BLUE}# Start relay server${NC}"
echo -e "  localup-relay"
echo ""
echo -e "  ${BLUE}# Create tunnel (in another terminal)${NC}"
echo -e "  localup --port 3000 --relay localhost:4443 --subdomain myapp --token demo-token"
echo ""
