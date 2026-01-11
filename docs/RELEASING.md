# Release Guide

This document explains how to create releases for localup, including stable and pre-release versions.

## Overview

The release process is **semi-automated** via GitHub Actions. When you push a version tag, the workflow:

1. Builds binaries for all platforms (Linux, macOS, Windows - AMD64/ARM64)
2. Calculates SHA256 checksums
3. Creates a GitHub release with all binaries
4. **Manual step**: You update the Homebrew formula using the provided scripts
5. Commit and push the updated formula

## Version Types

### Stable Releases

Format: `v1.0.0`, `v2.3.1`, etc.

- Updates `Formula/localup.rb`
- Marked as **stable** in GitHub
- Recommended for production use

### Pre-Releases (Beta/Alpha/RC)

Format: `v0.0.1-beta2`, `v1.0.0-rc1`, `v2.0.0-alpha3`

- Updates `Formula/localup-beta.rb` (separate formula)
- Marked as **pre-release** in GitHub
- For testing and early access

The workflow automatically detects pre-releases by checking if the version contains:
- `alpha`
- `beta`
- `rc`
- Any dash followed by letters (e.g., `-dev`, `-test`)

## Creating a Release

### Step 1: Ensure Everything is Ready

```bash
# Make sure you're on main branch
git checkout main
git pull origin main

# Run tests
cargo test

# Run linting
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings

# Build locally to verify
cargo build --release
```

### Step 2: Create and Push the Tag

#### For Stable Release

```bash
# Create tag
git tag v0.1.0

# Push tag to trigger release
git push origin v0.1.0
```

#### For Pre-Release (Beta)

```bash
# Create beta tag
git tag v0.0.1-beta2

# Push tag to trigger release
git push origin v0.0.1-beta2
```

### Step 3: Monitor the Release Workflow

1. Go to **GitHub Actions** → **Release workflow**
2. Watch the progress:
   - ✅ Build webapps
   - ✅ Build binaries (5 platforms in parallel)
   - ✅ Create GitHub release

### Step 4: Update the Homebrew Formula

After the release is created, update the formula:

```bash
# Option 1: Interactive (recommended)
./scripts/manual-formula-update.sh

# Option 2: Quick
./scripts/quick-formula-update.sh v0.1.0
git add Formula/localup.rb  # or Formula/localup-beta.rb
git commit -m "chore: update Homebrew formula for v0.1.0"
git push
```

The script will:
- Download SHA256SUMS.txt from the GitHub release
- Update the correct formula (stable or beta)
- Replace placeholders with real version and checksums

### Step 5: Verify the Release

After the workflow completes:

1. **Check GitHub Release**
   - Visit: https://github.com/localup-dev/localup/releases
   - Verify all binaries are attached
   - Verify installation instructions are correct

2. **Check Formula Update**
   - For stable: Check `Formula/localup.rb` was updated
   - For beta: Check `Formula/localup-beta.rb` was updated
   - Verify SHA256 hashes are real (not placeholders)
   - Verify version number matches the tag

3. **Test Installation**

   **Stable:**
   ```bash
   brew install https://raw.githubusercontent.com/localup-dev/localup/main/Formula/localup.rb
   localup --version
   brew uninstall localup
   ```

   **Beta:**
   ```bash
   brew install https://raw.githubusercontent.com/localup-dev/localup/main/Formula/localup-beta.rb
   localup --version
   brew uninstall localup-beta
   ```

## Formula Update Details

### What Gets Updated Automatically

The `scripts/update-homebrew-formula.sh` script updates:

- **Version number**: Extracted from the tag (removes `v` prefix)
- **Download URLs**: Points to the new release on GitHub
- **SHA256 hashes**: Real checksums from `SHA256SUMS.txt`
- **Class name**: `Localup` for stable, `LocalupBeta` for pre-release
- **Description**: Different text for stable vs beta
- **Caveats**: Pre-release warning for beta versions

### Manual Formula Update (if needed)

If the automatic update fails or you need to update manually:

```bash
# Download the release artifacts
gh release download v0.1.0 -p "SHA256SUMS.txt"

# Run the update script
./scripts/update-homebrew-formula.sh v0.1.0 SHA256SUMS.txt

# For beta releases
./scripts/update-homebrew-formula.sh v0.0.1-beta2 SHA256SUMS.txt Formula/localup-beta.rb

# Commit and push
git add Formula/localup.rb  # or Formula/localup-beta.rb
git commit -m "chore: update Homebrew formula for v0.1.0"
git push
```

## Version Numbering Strategy

Follow [Semantic Versioning](https://semver.org/):

### Format: `MAJOR.MINOR.PATCH`

- **MAJOR**: Breaking changes (incompatible API changes)
- **MINOR**: New features (backward-compatible)
- **PATCH**: Bug fixes (backward-compatible)

### Examples

```bash
v0.1.0        # First minor release (still in development)
v0.1.1        # Bug fix
v0.2.0        # New features
v1.0.0        # First stable release (production-ready)
v1.1.0        # New features on stable
v2.0.0        # Breaking changes

# Pre-releases
v0.1.0-alpha1  # Early testing
v0.1.0-beta1   # Feature complete, testing
v0.1.0-beta2   # Another beta with fixes
v1.0.0-rc1     # Release candidate
v1.0.0-rc2     # Another release candidate
```

## Release Checklist

Before creating a release:

- [ ] All tests pass locally (`cargo test`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] No clippy warnings (`cargo clippy`)
- [ ] CHANGELOG.md is updated
- [ ] Version bumped in `Cargo.toml` (workspace version)
- [ ] Documentation is up to date
- [ ] Breaking changes are documented (if any)

## Troubleshooting

### Release workflow failed

**Check the GitHub Actions logs:**
1. Go to Actions → Release workflow → Failed run
2. Expand the failed step
3. Common issues:
   - Build errors: Fix code and re-tag
   - Formula update fails: Check script syntax
   - Push fails: Check repository permissions

### Formula has wrong SHA256

**Re-run the update script:**
```bash
./scripts/update-homebrew-formula.sh <version> release/SHA256SUMS.txt
git add Formula/localup.rb
git commit --amend --no-edit
git push --force-with-lease
```

### Need to delete a release

```bash
# Delete remote tag
git push --delete origin v0.1.0

# Delete local tag
git tag -d v0.1.0

# Delete GitHub release (manually or via gh CLI)
gh release delete v0.1.0
```

## Post-Release

After a successful release:

1. **Announce the release** (if applicable)
2. **Monitor issues** for bug reports
3. **Update documentation** if new features were added
4. **Plan next release** based on roadmap

## Updating Pre-Release (Beta) Versions

### Example: Releasing v0.0.1-beta2

```bash
# 1. Make your changes
git add .
git commit -m "feat: add new feature for beta testing"
git push

# 2. Create beta tag
git tag v0.0.1-beta2
git push origin v0.0.1-beta2

# 3. GitHub Actions will:
#    - Build all binaries
#    - Update Formula/localup-beta.rb (not the stable formula)
#    - Create pre-release on GitHub
#    - Mark it as "Pre-release" (yellow tag)

# 4. Users install with:
brew install https://raw.githubusercontent.com/localup-dev/localup/main/Formula/localup-beta.rb
```

### Promoting Beta to Stable

When a beta is ready for stable release:

```bash
# Create stable tag (remove beta suffix)
git tag v0.1.0
git push origin v0.1.0

# This will:
# - Update Formula/localup.rb (stable formula)
# - Create stable release
# - Formula/localup-beta.rb remains at last beta version
```

## Example Release Flow

```bash
# Development cycle
git commit -m "feat: add feature A"
git commit -m "feat: add feature B"
git push

# Create first beta
git tag v0.1.0-beta1
git push origin v0.1.0-beta1
# → Updates Formula/localup-beta.rb

# More development
git commit -m "fix: bug in feature A"
git push

# Create second beta
git tag v0.1.0-beta2
git push origin v0.1.0-beta2
# → Updates Formula/localup-beta.rb again

# Ready for stable
git tag v0.1.0
git push origin v0.1.0
# → Updates Formula/localup.rb (stable)

# Next stable release
git tag v0.2.0
git push origin v0.2.0
# → Updates Formula/localup.rb
```

## GitHub Release Assets

Each release includes:

### Binaries
- `localup-linux-amd64.tar.gz` + `localup-exit-node-linux-amd64.tar.gz`
- `localup-linux-arm64.tar.gz` + `localup-exit-node-linux-arm64.tar.gz`
- `localup-macos-amd64.tar.gz` + `localup-exit-node-macos-amd64.tar.gz`
- `localup-macos-arm64.tar.gz` + `localup-exit-node-macos-arm64.tar.gz`
- `localup-windows-amd64.zip` + `localup-exit-node-windows-amd64.zip`

### Checksums
- `checksums-linux-amd64.txt`
- `checksums-linux-arm64.txt`
- `checksums-macos-amd64.txt`
- `checksums-macos-arm64.txt`
- `checksums-windows-amd64.txt`
- `SHA256SUMS.txt` (combined checksums)

### Source Code
- Auto-generated source tarball and zip from GitHub
