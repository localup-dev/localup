#!/bin/bash
# Helper script to create a new release

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Usage
usage() {
    echo "Usage: $0 <version>"
    echo ""
    echo "Examples:"
    echo "  $0 0.1.0          # Release version 0.1.0"
    echo "  $0 0.1.0-beta.1   # Pre-release version"
    echo ""
    echo "This script will:"
    echo "  1. Run tests to verify everything works"
    echo "  2. Create a git tag"
    echo "  3. Push the tag to trigger GitHub Actions release workflow"
    exit 1
}

# Check arguments
if [ $# -ne 1 ]; then
    usage
fi

VERSION=$1

# Validate version format (basic check)
if [[ ! $VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9\.]+)?$ ]]; then
    echo -e "${RED}Error: Invalid version format${NC}"
    echo "Expected format: MAJOR.MINOR.PATCH (e.g., 1.0.0)"
    echo "Or with pre-release: 1.0.0-beta.1"
    exit 1
fi

TAG="v$VERSION"

echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}  Creating Release: $TAG${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo ""

# Check if tag already exists
if git rev-parse "$TAG" >/dev/null 2>&1; then
    echo -e "${RED}Error: Tag $TAG already exists${NC}"
    echo ""
    echo "To delete the existing tag:"
    echo "  git tag -d $TAG"
    echo "  git push origin :refs/tags/$TAG"
    exit 1
fi

# Check if we're on main/master branch
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [[ "$CURRENT_BRANCH" != "main" && "$CURRENT_BRANCH" != "master" ]]; then
    echo -e "${YELLOW}Warning: You're not on main/master branch${NC}"
    echo "Current branch: $CURRENT_BRANCH"
    read -p "Continue anyway? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 1
    fi
fi

# Check for uncommitted changes
if [[ -n $(git status -s) ]]; then
    echo -e "${YELLOW}Warning: You have uncommitted changes${NC}"
    git status -s
    echo ""
    read -p "Continue anyway? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 1
    fi
fi

# Step 1: Run tests
echo -e "${BLUE}[1/4] Running tests...${NC}"
if cargo test --workspace --quiet; then
    echo -e "${GREEN}✅ All tests passed${NC}"
else
    echo -e "${RED}❌ Tests failed${NC}"
    exit 1
fi
echo ""

# Step 2: Build binaries to verify
echo -e "${BLUE}[2/4] Building release binaries...${NC}"
if cargo build --release --bin tunnel --bin tunnel-exit-node --quiet; then
    echo -e "${GREEN}✅ Binaries built successfully${NC}"
else
    echo -e "${RED}❌ Build failed${NC}"
    exit 1
fi
echo ""

# Step 3: Create tag
echo -e "${BLUE}[3/4] Creating git tag...${NC}"
read -p "Enter release notes (or press Enter for auto-generated): " RELEASE_NOTES

if [[ -z "$RELEASE_NOTES" ]]; then
    RELEASE_NOTES="Release $TAG"
fi

git tag -a "$TAG" -m "$RELEASE_NOTES"
echo -e "${GREEN}✅ Tag created: $TAG${NC}"
echo ""

# Step 4: Push tag
echo -e "${BLUE}[4/4] Pushing tag to GitHub...${NC}"
echo ""
echo -e "${YELLOW}This will trigger the GitHub Actions release workflow.${NC}"
echo "The workflow will:"
echo "  • Build Linux AMD64 binaries"
echo "  • Create a GitHub release"
echo "  • Upload release artifacts"
echo ""
read -p "Push tag to GitHub? (y/N): " -n 1 -r
echo

if [[ $REPLY =~ ^[Yy]$ ]]; then
    git push origin "$TAG"
    echo ""
    echo -e "${GREEN}✅ Tag pushed successfully!${NC}"
    echo ""
    echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  Release workflow started!${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "Monitor the workflow at:"
    echo "  $(git remote get-url origin | sed 's/.*github.com[:/]\(.*\)\.git/\1/')/actions"
    echo ""
    echo "Release will be available at:"
    echo "  $(git remote get-url origin | sed 's/.*github.com[:/]\(.*\)\.git/\1/')/releases/tag/$TAG"
else
    echo ""
    echo -e "${YELLOW}Tag created locally but not pushed.${NC}"
    echo ""
    echo "To push later:"
    echo "  git push origin $TAG"
    echo ""
    echo "To delete the local tag:"
    echo "  git tag -d $TAG"
fi
