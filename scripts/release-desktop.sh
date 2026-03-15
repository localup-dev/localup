#!/bin/bash
set -e

# =============================================================================
# LocalUp Desktop Release Script
# =============================================================================
#
# Complete local release workflow for the Tauri desktop app:
#   1. Bump version across all config files
#   2. Build frontend (Vite + React)
#   3. Build Tauri app (creates .app, .dmg, updater artifacts)
#   4. Optionally notarize DMG with Apple
#   5. Generate latest.json for Tauri auto-updater
#   6. Create GitHub Release with all artifacts via `gh`
#
# Usage:
#   ./scripts/release-desktop.sh <version> [options]
#
# Arguments:
#   <version>           Version to release (e.g. 0.2.0, 0.2.0-beta.1)
#
# Options:
#   --skip-build        Skip the build step (use existing artifacts)
#   --skip-notarize     Skip Apple notarization (default: skipped unless creds set)
#   --skip-tests        Skip running tests before building
#   --draft             Create the GitHub release as a draft
#   --dry-run           Show what would be done without making changes
#   --no-push           Build and prepare artifacts but don't create GitHub release
#
# Required Environment Variables (for notarization):
#   APPLE_ID                            Apple ID for notarization
#   APPLE_ID_PASSWORD                   App-specific password
#   APPLE_TEAM_ID                       Apple Developer Team ID
#
# Optional Environment Variables:
#   TAURI_SIGNING_PRIVATE_KEY           Tauri updater signing key
#   TAURI_SIGNING_PRIVATE_KEY_PASSWORD  Password for Tauri signing key
#
# Examples:
#   ./scripts/release-desktop.sh 0.2.0
#   ./scripts/release-desktop.sh 0.2.0 --skip-tests --draft
#   ./scripts/release-desktop.sh 0.2.0 --dry-run
#   ./scripts/release-desktop.sh 0.2.0 --skip-build   # reuse last build
#
# =============================================================================

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

print_header() {
    echo ""
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${CYAN}  $1${NC}"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}
print_step()    { echo -e "  ${BLUE}▸${NC} $1"; }
print_success() { echo -e "  ${GREEN}✓${NC} $1"; }
print_warning() { echo -e "  ${YELLOW}⚠${NC} $1"; }
print_error()   { echo -e "  ${RED}✗${NC} $1"; }

# ── Parse arguments ──────────────────────────────────────────────────────────

VERSION=""
SKIP_BUILD=false
SKIP_NOTARIZE=true   # default: skip unless Apple creds are present
SKIP_TESTS=false
DRAFT=false
DRY_RUN=false
NO_PUSH=false

usage() {
    head -42 "$0" | tail -37
    exit 1
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-build)     SKIP_BUILD=true;    shift ;;
        --skip-notarize)  SKIP_NOTARIZE=true; shift ;;
        --skip-tests)     SKIP_TESTS=true;    shift ;;
        --draft)          DRAFT=true;         shift ;;
        --dry-run)        DRY_RUN=true;       shift ;;
        --no-push)        NO_PUSH=true;       shift ;;
        -h|--help)        usage ;;
        -*)
            print_error "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
        *)
            if [ -z "$VERSION" ]; then
                VERSION="$1"
            else
                print_error "Unexpected argument: $1"
                exit 1
            fi
            shift
            ;;
    esac
done

if [ -z "$VERSION" ]; then
    print_error "Version argument is required"
    echo ""
    usage
fi

# Validate version format
if [[ ! $VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9\.]+)?$ ]]; then
    print_error "Invalid version format: $VERSION"
    echo "  Expected: MAJOR.MINOR.PATCH (e.g. 0.2.0) or MAJOR.MINOR.PATCH-PRE (e.g. 0.2.0-beta.1)"
    exit 1
fi

TAG="v$VERSION"

# Auto-enable notarization if Apple credentials are available
if [ -n "$APPLE_ID" ] && [ -n "$APPLE_ID_PASSWORD" ] && [ -n "$APPLE_TEAM_ID" ]; then
    SKIP_NOTARIZE=false
fi

# Detect pre-release
IS_PRERELEASE=false
if [[ "$VERSION" =~ (alpha|beta|rc|-[a-zA-Z]) ]]; then
    IS_PRERELEASE=true
fi

# ── Change to project root ───────────────────────────────────────────────────

cd "$(dirname "$0")/.."
PROJECT_ROOT=$(pwd)
DESKTOP_DIR="$PROJECT_ROOT/apps/localup-desktop"
TAURI_DIR="$DESKTOP_DIR/src-tauri"

# ── Architecture detection ───────────────────────────────────────────────────

ARCH=$(uname -m)
case "$ARCH" in
    arm64|aarch64) RUST_TARGET="aarch64-apple-darwin"; ARCH_LABEL="aarch64" ;;
    x86_64)        RUST_TARGET="x86_64-apple-darwin";  ARCH_LABEL="x86_64"  ;;
    *)
        print_error "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

# ── Paths ────────────────────────────────────────────────────────────────────

# Tauri bundles go to workspace root target (since it's a workspace member)
BUNDLE_DIR="$PROJECT_ROOT/target/$RUST_TARGET/release/bundle"
# Fallback: sometimes Tauri puts bundles under the app's own target
BUNDLE_DIR_ALT="$TAURI_DIR/target/$RUST_TARGET/release/bundle"

OUTPUT_DIR="$PROJECT_ROOT/release/$VERSION"

# =============================================================================
# Validate Environment
# =============================================================================
print_header "Validating Environment"

# Check we're on macOS
if [ "$(uname)" != "Darwin" ]; then
    print_error "This script only runs on macOS (Tauri desktop build)"
    exit 1
fi
print_success "Running on macOS ($ARCH)"

# Check required tools
MISSING_TOOLS=()
for cmd in bun cargo gh; do
    if ! command -v "$cmd" &> /dev/null; then
        MISSING_TOOLS+=("$cmd")
    fi
done

if [ ${#MISSING_TOOLS[@]} -ne 0 ]; then
    print_error "Missing required tools: ${MISSING_TOOLS[*]}"
    echo ""
    echo "  Install with:"
    echo "    bun   → curl -fsSL https://bun.sh/install | bash"
    echo "    cargo → curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo "    gh    → brew install gh"
    exit 1
fi
print_success "Required tools: bun, cargo, gh"

# Check gh is authenticated
if ! gh auth status &> /dev/null; then
    print_error "GitHub CLI is not authenticated. Run: gh auth login"
    exit 1
fi
print_success "GitHub CLI authenticated"

# Check Tauri CLI is available
if ! (cd "$DESKTOP_DIR" && bun run tauri --version) &> /dev/null; then
    print_warning "Tauri CLI not found, will install via bun"
fi

# Notarization credentials check
if [ "$SKIP_NOTARIZE" = false ]; then
    if [ -z "$APPLE_ID" ] || [ -z "$APPLE_ID_PASSWORD" ] || [ -z "$APPLE_TEAM_ID" ]; then
        print_error "Apple credentials required for notarization"
        echo "  Set: APPLE_ID, APPLE_ID_PASSWORD, APPLE_TEAM_ID"
        echo "  Or notarization will be auto-skipped"
        SKIP_NOTARIZE=true
    else
        print_success "Apple credentials configured"
    fi
fi

# Tauri updater signing key check
HAS_SIGNING_KEY=false
if [ -n "$TAURI_SIGNING_PRIVATE_KEY" ]; then
    HAS_SIGNING_KEY=true
    print_success "Tauri signing key configured"
else
    print_warning "No TAURI_SIGNING_PRIVATE_KEY set - updater signatures will be missing"
fi

# Check if tag already exists
if git rev-parse "$TAG" &> /dev/null; then
    print_error "Tag $TAG already exists"
    echo ""
    echo "  To delete it:  git tag -d $TAG && git push origin :refs/tags/$TAG"
    exit 1
fi
print_success "Tag $TAG is available"

# Check for uncommitted changes
if [ -n "$(git status -s)" ]; then
    print_warning "You have uncommitted changes"
    if [ "$DRY_RUN" = false ]; then
        read -p "  Continue anyway? (y/N): " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            echo "  Aborted."
            exit 1
        fi
    fi
fi

# Summary
echo ""
echo -e "  ${BOLD}Release configuration:${NC}"
echo "    Version:       $VERSION ($TAG)"
echo "    Pre-release:   $IS_PRERELEASE"
echo "    Architecture:  $ARCH_LABEL ($RUST_TARGET)"
echo "    Skip Build:    $SKIP_BUILD"
echo "    Skip Tests:    $SKIP_TESTS"
echo "    Notarize:      $([ "$SKIP_NOTARIZE" = true ] && echo 'No' || echo 'Yes')"
echo "    Signing Key:   $([ "$HAS_SIGNING_KEY" = true ] && echo 'Yes' || echo 'No')"
echo "    Draft Release: $DRAFT"
echo "    Dry Run:       $DRY_RUN"
echo "    No Push:       $NO_PUSH"

if [ "$DRY_RUN" = false ]; then
    echo ""
    read -p "  Proceed? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "  Aborted."
        exit 0
    fi
fi

# =============================================================================
# Step 1: Bump Version
# =============================================================================
print_header "Step 1: Bump Version to $VERSION"

# Files that contain the version
TAURI_CONF="$TAURI_DIR/tauri.conf.json"
DESKTOP_CARGO="$TAURI_DIR/Cargo.toml"
DESKTOP_PKG="$DESKTOP_DIR/package.json"

bump_version_in_file() {
    local file="$1"
    local description="$2"

    if [ "$DRY_RUN" = true ]; then
        echo "  [DRY RUN] Would update $description"
        return
    fi

    case "$file" in
        *.json)
            # Use a temp file approach for JSON version bumping
            if [[ "$file" == *tauri.conf.json ]]; then
                # tauri.conf.json: "version": "X.Y.Z" at top level
                sed -i '' 's/"version": "[^"]*"/"version": "'"$VERSION"'"/' "$file"
            else
                # package.json: "version": "X.Y.Z" (first occurrence)
                sed -i '' '0,/"version": "[^"]*"/{s/"version": "[^"]*"/"version": "'"$VERSION"'"/}' "$file"
            fi
            ;;
        *.toml)
            # Cargo.toml: version = "X.Y.Z" (first occurrence under [package])
            sed -i '' '0,/^version = "[^"]*"/{s/^version = "[^"]*"/version = "'"$VERSION"'"/}' "$file"
            ;;
    esac

    print_success "Updated $description"
}

bump_version_in_file "$TAURI_CONF"    "tauri.conf.json"
bump_version_in_file "$DESKTOP_CARGO" "src-tauri/Cargo.toml"
bump_version_in_file "$DESKTOP_PKG"   "package.json"

# Verify versions match
if [ "$DRY_RUN" = false ]; then
    V_TAURI=$(grep '"version"' "$TAURI_CONF" | head -1 | sed 's/.*"version": "\([^"]*\)".*/\1/')
    V_CARGO=$(grep '^version' "$DESKTOP_CARGO" | head -1 | sed 's/version = "\([^"]*\)"/\1/')
    V_PKG=$(grep '"version"' "$DESKTOP_PKG" | head -1 | sed 's/.*"version": "\([^"]*\)".*/\1/')

    if [ "$V_TAURI" != "$VERSION" ] || [ "$V_CARGO" != "$VERSION" ] || [ "$V_PKG" != "$VERSION" ]; then
        print_error "Version mismatch after bump!"
        echo "    tauri.conf.json: $V_TAURI"
        echo "    Cargo.toml:      $V_CARGO"
        echo "    package.json:    $V_PKG"
        exit 1
    fi
    print_success "All versions set to $VERSION"
fi

# =============================================================================
# Step 2: Run Tests
# =============================================================================
if [ "$SKIP_TESTS" = false ] && [ "$SKIP_BUILD" = false ]; then
    print_header "Step 2: Running Tests"

    if [ "$DRY_RUN" = true ]; then
        echo "  [DRY RUN] Would run: cargo test --workspace --quiet"
    else
        print_step "Running workspace tests..."
        if cargo test --workspace --quiet 2>&1; then
            print_success "All tests passed"
        else
            print_error "Tests failed. Fix them before releasing."
            exit 1
        fi
    fi
else
    print_header "Step 2: Tests (Skipped)"
fi

# =============================================================================
# Step 3: Build Tauri App
# =============================================================================
if [ "$SKIP_BUILD" = false ]; then
    print_header "Step 3: Building Tauri Desktop App"

    if [ "$DRY_RUN" = true ]; then
        echo "  [DRY RUN] Would run: bun install && bun run tauri build --target $RUST_TARGET"
    else
        print_step "Installing frontend dependencies..."
        (cd "$DESKTOP_DIR" && bun install --frozen-lockfile 2>/dev/null || bun install)
        print_success "Frontend dependencies installed"

        print_step "Building Tauri app (this may take a while)..."
        (cd "$DESKTOP_DIR" && bun run tauri build --target "$RUST_TARGET")
        print_success "Tauri build completed"
    fi
else
    print_header "Step 3: Build (Skipped)"
    print_warning "Using existing build artifacts"
fi

# =============================================================================
# Step 4: Locate and Verify Artifacts
# =============================================================================
print_header "Step 4: Locating Build Artifacts"

# Determine actual bundle directory
if [ -d "$BUNDLE_DIR" ]; then
    ACTUAL_BUNDLE_DIR="$BUNDLE_DIR"
elif [ -d "$BUNDLE_DIR_ALT" ]; then
    ACTUAL_BUNDLE_DIR="$BUNDLE_DIR_ALT"
else
    if [ "$DRY_RUN" = true ]; then
        echo "  [DRY RUN] Would look for bundles in:"
        echo "    $BUNDLE_DIR"
        echo "    $BUNDLE_DIR_ALT"
        ACTUAL_BUNDLE_DIR="$BUNDLE_DIR"
    else
        print_error "Bundle directory not found"
        echo "  Checked:"
        echo "    $BUNDLE_DIR"
        echo "    $BUNDLE_DIR_ALT"
        exit 1
    fi
fi

# Find DMG
DMG_FILE=""
if [ "$DRY_RUN" = false ]; then
    # Tauri names DMGs like: LocalUp_0.2.0_aarch64.dmg
    for f in "$ACTUAL_BUNDLE_DIR/dmg/"*.dmg; do
        if [ -f "$f" ]; then
            DMG_FILE="$f"
            break
        fi
    done

    if [ -z "$DMG_FILE" ]; then
        print_error "No .dmg found in $ACTUAL_BUNDLE_DIR/dmg/"
        exit 1
    fi
    DMG_SIZE=$(ls -lh "$DMG_FILE" | awk '{print $5}')
    print_success "DMG: $(basename "$DMG_FILE") ($DMG_SIZE)"
fi

# Find updater artifacts (.tar.gz and .sig in macos/ dir)
TAR_GZ_FILE=""
SIG_FILE=""
if [ "$DRY_RUN" = false ]; then
    for f in "$ACTUAL_BUNDLE_DIR/macos/"*.tar.gz; do
        if [ -f "$f" ]; then
            TAR_GZ_FILE="$f"
            break
        fi
    done
    for f in "$ACTUAL_BUNDLE_DIR/macos/"*.tar.gz.sig; do
        if [ -f "$f" ]; then
            SIG_FILE="$f"
            break
        fi
    done

    if [ -n "$TAR_GZ_FILE" ]; then
        TAR_SIZE=$(ls -lh "$TAR_GZ_FILE" | awk '{print $5}')
        print_success "Updater tar.gz: $(basename "$TAR_GZ_FILE") ($TAR_SIZE)"
    else
        print_warning "No updater .tar.gz found (auto-update won't work)"
    fi

    if [ -n "$SIG_FILE" ]; then
        print_success "Updater signature: $(basename "$SIG_FILE")"
    else
        print_warning "No updater .sig found (auto-update won't work)"
    fi
fi

# =============================================================================
# Step 5: Notarize DMG (macOS only)
# =============================================================================
print_header "Step 5: Notarize DMG"

if [ "$SKIP_NOTARIZE" = true ]; then
    print_warning "Notarization skipped"
    echo "  Users may need to run: xattr -cr /Applications/LocalUp.app"
elif [ "$DRY_RUN" = true ]; then
    echo "  [DRY RUN] Would notarize: $(basename "$DMG_FILE")"
else
    print_step "Submitting to Apple notary service..."
    xcrun notarytool submit "$DMG_FILE" \
        --apple-id "$APPLE_ID" \
        --password "$APPLE_ID_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" \
        --wait

    print_step "Stapling notarization ticket..."
    xcrun stapler staple "$DMG_FILE"
    print_success "Notarization complete"
fi

# =============================================================================
# Step 6: Prepare Release Directory
# =============================================================================
print_header "Step 6: Preparing Release Artifacts"

mkdir -p "$OUTPUT_DIR"

# Standardized artifact names for the release
RELEASE_DMG_NAME="LocalUp-macos-${ARCH_LABEL}.dmg"
RELEASE_TAR_NAME="LocalUp-macos-${ARCH_LABEL}.app.tar.gz"

if [ "$DRY_RUN" = true ]; then
    echo "  [DRY RUN] Would copy artifacts to $OUTPUT_DIR"
else
    # Copy DMG
    cp "$DMG_FILE" "$OUTPUT_DIR/$RELEASE_DMG_NAME"
    print_success "Prepared: $RELEASE_DMG_NAME"

    # Copy updater tar.gz
    if [ -n "$TAR_GZ_FILE" ]; then
        cp "$TAR_GZ_FILE" "$OUTPUT_DIR/$RELEASE_TAR_NAME"
        print_success "Prepared: $RELEASE_TAR_NAME"
    fi

    # Copy signature
    if [ -n "$SIG_FILE" ]; then
        SIGNATURE=$(cat "$SIG_FILE")
        echo "$SIGNATURE" > "$OUTPUT_DIR/${RELEASE_TAR_NAME}.sig"
        print_success "Prepared: ${RELEASE_TAR_NAME}.sig"
    else
        SIGNATURE=""
    fi

    # Generate checksums
    (cd "$OUTPUT_DIR" && shasum -a 256 *.dmg *.tar.gz 2>/dev/null > SHA256SUMS.txt || true)
    print_success "Generated: SHA256SUMS.txt"
fi

# =============================================================================
# Step 7: Generate latest.json for Tauri Auto-Updater
# =============================================================================
print_header "Step 7: Generating latest.json"

PUB_DATE=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

# GitHub release download URL
REPO_URL=$(gh repo view --json url -q '.url' 2>/dev/null || echo "https://github.com/localup-dev/localup")
RELEASE_URL="$REPO_URL/releases/download/$TAG"

LATEST_JSON="$OUTPUT_DIR/latest.json"

# Build platforms object based on current architecture
# (CI builds for all architectures; local builds only for current machine)
if [ "$ARCH_LABEL" = "aarch64" ]; then
    PLATFORM_KEY="darwin-aarch64"
else
    PLATFORM_KEY="darwin-x86_64"
fi

if [ "$DRY_RUN" = true ]; then
    echo "  [DRY RUN] Would generate latest.json with:"
    echo "    version:  $VERSION"
    echo "    platform: $PLATFORM_KEY"
    echo "    url:      $RELEASE_URL/$RELEASE_DMG_NAME"
else
    cat > "$LATEST_JSON" << EOF
{
  "version": "$VERSION",
  "notes": "See $REPO_URL/releases/tag/$TAG for release notes.",
  "pub_date": "$PUB_DATE",
  "platforms": {
    "$PLATFORM_KEY": {
      "url": "$RELEASE_URL/$RELEASE_DMG_NAME",
      "signature": "$SIGNATURE"
    }
  }
}
EOF

    print_success "Generated: latest.json"
    echo ""
    cat "$LATEST_JSON"
fi

# =============================================================================
# Step 8: Create Git Tag
# =============================================================================
print_header "Step 8: Git Tag"

if [ "$DRY_RUN" = true ]; then
    echo "  [DRY RUN] Would create tag: $TAG"
elif [ "$NO_PUSH" = true ]; then
    print_warning "Skipping git tag (--no-push)"
else
    # Commit version bump if there are changes
    if [ -n "$(git status -s)" ]; then
        print_step "Committing version bump..."
        git add \
            "$TAURI_CONF" \
            "$DESKTOP_CARGO" \
            "$DESKTOP_PKG"
        git commit -m "chore: bump desktop app version to $VERSION"
        print_success "Version bump committed"
    fi

    print_step "Creating tag $TAG..."
    git tag -a "$TAG" -m "Release $TAG"
    print_success "Tag $TAG created"

    print_step "Pushing commit and tag..."
    git push origin HEAD
    git push origin "$TAG"
    print_success "Pushed to remote"
fi

# =============================================================================
# Step 9: Create GitHub Release
# =============================================================================
print_header "Step 9: GitHub Release"

if [ "$NO_PUSH" = true ]; then
    print_warning "Skipping GitHub release (--no-push)"
    echo ""
    echo "  Artifacts are in: $OUTPUT_DIR/"
    echo ""
    echo "  To create the release manually:"
    echo "    gh release create $TAG $OUTPUT_DIR/* --title \"Release $TAG\""
elif [ "$DRY_RUN" = true ]; then
    echo "  [DRY RUN] Would create GitHub release $TAG with:"
    echo "    $RELEASE_DMG_NAME"
    [ -n "$TAR_GZ_FILE" ] && echo "    $RELEASE_TAR_NAME"
    [ -n "$SIG_FILE" ]    && echo "    ${RELEASE_TAR_NAME}.sig"
    echo "    latest.json"
    echo "    SHA256SUMS.txt"
else
    print_step "Creating GitHub release..."

    # Build file list
    RELEASE_FILES=()
    RELEASE_FILES+=("$OUTPUT_DIR/$RELEASE_DMG_NAME")
    [ -f "$OUTPUT_DIR/$RELEASE_TAR_NAME" ]     && RELEASE_FILES+=("$OUTPUT_DIR/$RELEASE_TAR_NAME")
    [ -f "$OUTPUT_DIR/${RELEASE_TAR_NAME}.sig" ] && RELEASE_FILES+=("$OUTPUT_DIR/${RELEASE_TAR_NAME}.sig")
    [ -f "$OUTPUT_DIR/latest.json" ]           && RELEASE_FILES+=("$OUTPUT_DIR/latest.json")
    [ -f "$OUTPUT_DIR/SHA256SUMS.txt" ]        && RELEASE_FILES+=("$OUTPUT_DIR/SHA256SUMS.txt")

    # Build gh flags
    GH_FLAGS=()
    GH_FLAGS+=("--title" "Release $TAG")
    [ "$DRAFT" = true ]        && GH_FLAGS+=("--draft")
    [ "$IS_PRERELEASE" = true ] && GH_FLAGS+=("--prerelease")

    NOTARIZE_NOTE=""
    if [ "$SKIP_NOTARIZE" = true ]; then
        NOTARIZE_NOTE='> **Note for macOS users:** The app is not yet notarized. If you see _"LocalUp is damaged and can'\''t be opened"_, run:
> ```bash
> xattr -cr /Applications/LocalUp.app
> ```
> Then open the app again.'
    fi

    gh release create "$TAG" \
        "${RELEASE_FILES[@]}" \
        "${GH_FLAGS[@]}" \
        --notes "$(cat <<EOF
## LocalUp Desktop $TAG

### macOS
- **$([ "$ARCH_LABEL" = "aarch64" ] && echo "Apple Silicon (M1/M2/M3/M4)" || echo "Intel")**: \`$RELEASE_DMG_NAME\`

$NOTARIZE_NOTE

### Auto-Update
Existing installations will be prompted to update automatically via the built-in updater.

### Verify Download
\`\`\`bash
shasum -a 256 -c SHA256SUMS.txt
\`\`\`
EOF
)"

    RELEASE_PAGE="$REPO_URL/releases/tag/$TAG"
    print_success "GitHub release created"
    echo ""
    echo -e "  ${BOLD}$RELEASE_PAGE${NC}"
fi

# =============================================================================
# Summary
# =============================================================================
print_header "Release Complete!"

echo ""
echo -e "  ${BOLD}Version:${NC}  $VERSION"
echo -e "  ${BOLD}Tag:${NC}      $TAG"
echo ""
echo "  Local artifacts:"
echo "    $OUTPUT_DIR/$RELEASE_DMG_NAME"
[ -f "$OUTPUT_DIR/$RELEASE_TAR_NAME" ] && echo "    $OUTPUT_DIR/$RELEASE_TAR_NAME"
echo "    $OUTPUT_DIR/latest.json"
echo "    $OUTPUT_DIR/SHA256SUMS.txt"

if [ "$NO_PUSH" = false ] && [ "$DRY_RUN" = false ]; then
    echo ""
    echo "  GitHub Release:"
    echo "    $REPO_URL/releases/tag/$TAG"
    echo ""
    echo "  Download URL:"
    echo "    $RELEASE_URL/$RELEASE_DMG_NAME"
    echo ""
    echo "  Auto-Updater Endpoint:"
    echo "    $REPO_URL/releases/latest/download/latest.json"
fi

if [ "$DRY_RUN" = true ]; then
    echo ""
    print_warning "This was a dry run. Nothing was changed."
fi

echo ""
