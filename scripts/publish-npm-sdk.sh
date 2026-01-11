#!/bin/bash
#
# Publish Node.js SDK to NPM
#
# Usage:
#   ./scripts/publish-npm-sdk.sh <version>
#
# Examples:
#   ./scripts/publish-npm-sdk.sh 0.1.0
#   ./scripts/publish-npm-sdk.sh 1.0.0-beta.1
#
# This script will:
# 1. Update package.json version
# 2. Build the package
# 3. Run tests
# 4. Create and push a git tag (sdk-nodejs-v<version>)
# 5. The GitHub Action will then publish to NPM
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SDK_DIR="$ROOT_DIR/sdks/nodejs"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_step() {
    echo -e "${BLUE}==>${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

# Check arguments
if [ -z "$1" ]; then
    echo "Usage: $0 <version>"
    echo ""
    echo "Examples:"
    echo "  $0 0.1.0"
    echo "  $0 1.0.0-beta.1"
    exit 1
fi

VERSION="$1"
TAG="sdk-nodejs-v$VERSION"

# Validate version format (semver)
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
    print_error "Invalid version format: $VERSION"
    echo "Version must be semver format: X.Y.Z or X.Y.Z-prerelease"
    exit 1
fi

# Check if we're on main branch
CURRENT_BRANCH=$(git branch --show-current)
if [ "$CURRENT_BRANCH" != "main" ]; then
    print_warning "You are not on the main branch (current: $CURRENT_BRANCH)"
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Check for uncommitted changes
if ! git diff-index --quiet HEAD --; then
    print_error "You have uncommitted changes. Please commit or stash them first."
    exit 1
fi

# Check if tag already exists
if git rev-parse "$TAG" >/dev/null 2>&1; then
    print_error "Tag $TAG already exists"
    exit 1
fi

# Navigate to SDK directory
cd "$SDK_DIR"

print_step "Updating package.json version to $VERSION..."
# Use node to update package.json (more reliable than sed)
node -e "
const fs = require('fs');
const pkg = JSON.parse(fs.readFileSync('package.json', 'utf8'));
pkg.version = '$VERSION';
fs.writeFileSync('package.json', JSON.stringify(pkg, null, 2) + '\n');
"
print_success "Updated package.json"

print_step "Installing dependencies..."
bun install

print_step "Running linting..."
bun run lint

print_step "Running tests..."
bun test

print_step "Building package..."
bun run build:all

print_step "Verifying package..."
npm pack --dry-run

echo ""
print_success "Package ready for publishing!"
echo ""
echo "Package: @localup/sdk@$VERSION"
echo "Tag: $TAG"
echo ""

# Commit and tag
print_step "Committing version bump..."
git add package.json
git commit -m "chore(sdk-nodejs): bump version to $VERSION"

print_step "Creating tag $TAG..."
git tag -a "$TAG" -m "Release @localup/sdk@$VERSION"

echo ""
print_success "Local preparation complete!"
echo ""
echo "To publish, push the tag to GitHub:"
echo ""
echo "  git push origin main"
echo "  git push origin $TAG"
echo ""
echo "The GitHub Action will then publish to NPM automatically."
echo ""
echo "Or, to undo:"
echo ""
echo "  git tag -d $TAG"
echo "  git reset --soft HEAD~1"
