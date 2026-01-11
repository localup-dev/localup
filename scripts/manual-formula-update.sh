#!/bin/bash
# Manual Homebrew Formula Update Script
# This script helps you manually update the formula after a release
# Usage: ./scripts/manual-formula-update.sh

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Homebrew Formula Manual Update Script${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════${NC}"
echo ""

# Step 1: Detect latest release from git tags
echo -e "${YELLOW}📋 Step 1: Detecting latest release...${NC}"
LATEST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")

if [ -z "$LATEST_TAG" ]; then
  echo -e "${RED}❌ No git tags found. Please create a release first.${NC}"
  exit 1
fi

echo -e "  Latest tag: ${GREEN}$LATEST_TAG${NC}"
echo ""

# Step 2: Ask user to confirm or enter different version
echo -e "${YELLOW}❓ Which version do you want to update the formula for?${NC}"
echo -e "   Press Enter to use ${GREEN}$LATEST_TAG${NC}, or type a different version:"
read -r USER_VERSION

if [ -n "$USER_VERSION" ]; then
  VERSION="$USER_VERSION"
else
  VERSION="$LATEST_TAG"
fi

echo -e "  Using version: ${GREEN}$VERSION${NC}"
echo ""

# Step 3: Check if release exists on GitHub
echo -e "${YELLOW}🔍 Step 2: Checking if release exists on GitHub...${NC}"

# Check if gh CLI is available
if ! command -v gh &> /dev/null; then
  echo -e "${YELLOW}⚠️  GitHub CLI (gh) not found. Skipping release check.${NC}"
  echo -e "   Install with: brew install gh${NC}"
else
  if gh release view "$VERSION" &>/dev/null; then
    echo -e "  ${GREEN}✓ Release $VERSION exists on GitHub${NC}"
  else
    echo -e "${RED}❌ Release $VERSION not found on GitHub${NC}"
    echo -e "${YELLOW}   Create it with: git tag $VERSION && git push origin $VERSION${NC}"
    exit 1
  fi
fi
echo ""

# Step 4: Download checksums
echo -e "${YELLOW}📥 Step 3: Downloading SHA256SUMS.txt from release...${NC}"

CHECKSUMS_FILE="/tmp/localup-SHA256SUMS-$VERSION.txt"

if command -v gh &> /dev/null; then
  # Try to download with gh CLI
  if gh release download "$VERSION" -p "SHA256SUMS.txt" -O "$CHECKSUMS_FILE" 2>/dev/null; then
    echo -e "  ${GREEN}✓ Downloaded SHA256SUMS.txt${NC}"
  else
    echo -e "${RED}❌ Failed to download SHA256SUMS.txt from release${NC}"
    echo -e "${YELLOW}   Please ensure the release has SHA256SUMS.txt attached${NC}"
    exit 1
  fi
else
  # Manual download with curl
  DOWNLOAD_URL="https://github.com/localup-dev/localup/releases/download/$VERSION/SHA256SUMS.txt"
  if curl -sL "$DOWNLOAD_URL" -o "$CHECKSUMS_FILE"; then
    echo -e "  ${GREEN}✓ Downloaded SHA256SUMS.txt${NC}"
  else
    echo -e "${RED}❌ Failed to download SHA256SUMS.txt${NC}"
    echo -e "${YELLOW}   URL: $DOWNLOAD_URL${NC}"
    exit 1
  fi
fi
echo ""

# Step 5: Determine which formula to update
echo -e "${YELLOW}📝 Step 4: Determining formula type...${NC}"

if [[ "$VERSION" =~ (alpha|beta|rc|-[a-zA-Z]) ]]; then
  FORMULA_TYPE="beta"
  FORMULA_FILE="Formula/localup-beta.rb"
  echo -e "  Detected: ${YELLOW}PRE-RELEASE${NC}"
else
  FORMULA_TYPE="stable"
  FORMULA_FILE="Formula/localup.rb"
  echo -e "  Detected: ${GREEN}STABLE${NC}"
fi

echo -e "  Will update: ${BLUE}$FORMULA_FILE${NC}"
echo ""

# Step 6: Run the update script
echo -e "${YELLOW}🔧 Step 5: Updating formula...${NC}"

if [ ! -f "scripts/update-homebrew-formula.sh" ]; then
  echo -e "${RED}❌ update-homebrew-formula.sh not found${NC}"
  exit 1
fi

chmod +x scripts/update-homebrew-formula.sh
./scripts/update-homebrew-formula.sh "$VERSION" "$CHECKSUMS_FILE" "$FORMULA_FILE"

echo ""

# Step 7: Show the changes
echo -e "${YELLOW}📄 Step 6: Review changes...${NC}"
echo ""
git diff "$FORMULA_FILE" || echo "No changes detected"
echo ""

# Step 8: Ask to commit
echo -e "${YELLOW}❓ Do you want to commit these changes?${NC}"
echo -e "   [y/N]:"
read -r SHOULD_COMMIT

if [[ "$SHOULD_COMMIT" =~ ^[Yy]$ ]]; then
  git add "$FORMULA_FILE"

  if [ "$FORMULA_TYPE" = "beta" ]; then
    COMMIT_MSG="chore: update Homebrew beta formula for $VERSION"
  else
    COMMIT_MSG="chore: update Homebrew formula for $VERSION"
  fi

  git commit -m "$COMMIT_MSG"
  echo -e "${GREEN}✓ Changes committed${NC}"
  echo ""

  # Ask to push
  echo -e "${YELLOW}❓ Do you want to push to origin?${NC}"
  echo -e "   [y/N]:"
  read -r SHOULD_PUSH

  if [[ "$SHOULD_PUSH" =~ ^[Yy]$ ]]; then
    git push origin HEAD:main
    echo -e "${GREEN}✓ Changes pushed to main${NC}"
  else
    echo -e "${YELLOW}⚠️  Changes committed but not pushed${NC}"
    echo -e "   Push with: ${BLUE}git push origin HEAD:main${NC}"
  fi
else
  echo -e "${YELLOW}⚠️  Changes not committed${NC}"
  echo -e "   To commit manually:"
  echo -e "   ${BLUE}git add $FORMULA_FILE${NC}"
  echo -e "   ${BLUE}git commit -m 'chore: update Homebrew formula for $VERSION'${NC}"
  echo -e "   ${BLUE}git push${NC}"
fi

echo ""

# Step 9: Test installation
echo -e "${YELLOW}🧪 Step 7: Test the formula?${NC}"
echo -e "   [y/N]:"
read -r SHOULD_TEST

if [[ "$SHOULD_TEST" =~ ^[Yy]$ ]]; then
  echo ""
  echo -e "${BLUE}Testing installation...${NC}"

  # Check if already installed
  if [ "$FORMULA_TYPE" = "beta" ]; then
    PACKAGE_NAME="localup-beta"
  else
    PACKAGE_NAME="localup"
  fi

  if brew list "$PACKAGE_NAME" &>/dev/null; then
    echo -e "${YELLOW}⚠️  $PACKAGE_NAME is already installed${NC}"
    echo -e "${YELLOW}   Uninstalling first...${NC}"
    brew uninstall "$PACKAGE_NAME"
  fi

  echo -e "${BLUE}Installing from formula...${NC}"
  brew install "$FORMULA_FILE"

  echo ""
  echo -e "${BLUE}Testing binaries...${NC}"
  localup --version || echo -e "${RED}❌ localup failed${NC}"
  localup-relay --version || echo -e "${RED}❌ localup-relay failed${NC}"

  echo ""
  echo -e "${YELLOW}Do you want to uninstall after testing?${NC}"
  echo -e "   [Y/n]:"
  read -r SHOULD_UNINSTALL

  if [[ ! "$SHOULD_UNINSTALL" =~ ^[Nn]$ ]]; then
    brew uninstall "$PACKAGE_NAME"
    echo -e "${GREEN}✓ Uninstalled${NC}"
  fi
fi

echo ""
echo -e "${GREEN}═══════════════════════════════════════════════${NC}"
echo -e "${GREEN}✅ Formula update complete!${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════${NC}"

# Cleanup
rm -f "$CHECKSUMS_FILE"

echo ""
echo -e "Formula updated: ${BLUE}$FORMULA_FILE${NC}"
echo -e "Version: ${GREEN}$VERSION${NC}"
echo -e "Type: ${YELLOW}$FORMULA_TYPE${NC}"
echo ""
