# Release Process

This document describes how to create releases for the tunnel project.

## GitHub Actions Workflows

### 1. CI Workflow (`ci.yml`)
**Triggers:** Push to main/master/develop branches, Pull Requests
**Purpose:** Run tests, linting, and build checks

**What it does:**
- Runs all tests in the workspace
- Checks code formatting with `cargo fmt`
- Runs Clippy linting
- Builds the entire workspace in release mode

### 2. Release Workflow (`release.yml`)
**Triggers:** Push tags matching `v*.*.*` (e.g., v1.0.0, v0.1.2)
**Purpose:** Build and release Linux AMD64 binaries

**What it builds:**
- `tunnel` - CLI client for creating tunnels
- `tunnel-exit-node` - Exit node server

**Outputs:**
- `tunnel-linux-amd64.tar.gz` - CLI client binary
- `tunnel-exit-node-linux-amd64.tar.gz` - Exit node server binary
- `checksums-linux-amd64.txt` - SHA256 checksums

### 3. Multi-Platform Release (Optional)
**File:** `release-multiplatform.yml.example`
**Purpose:** Build binaries for multiple platforms

To enable multi-platform releases:
```bash
mv .github/workflows/release-multiplatform.yml.example .github/workflows/release-multiplatform.yml
rm .github/workflows/release.yml  # Optional: remove single-platform workflow
```

**Supported platforms:**
- Linux AMD64 (`x86_64-unknown-linux-gnu`)
- Linux ARM64 (`aarch64-unknown-linux-gnu`)
- macOS Intel (`x86_64-apple-darwin`)
- macOS Apple Silicon (`aarch64-apple-darwin`)
- Windows AMD64 (`x86_64-pc-windows-msvc`)

## Creating a Release

### Step 1: Update Version Numbers

Update version in `Cargo.toml`:
```toml
[workspace.package]
version = "0.1.0"  # Change this
```

### Step 2: Update CHANGELOG (Recommended)

Document changes in `CHANGELOG.md`:
```markdown
## [0.1.0] - 2025-01-15
### Added
- Feature X
- Feature Y

### Fixed
- Bug Z
```

### Step 3: Commit Changes

```bash
git add Cargo.toml CHANGELOG.md
git commit -m "chore: bump version to 0.1.0"
git push origin main
```

### Step 4: Create and Push Tag

```bash
# Create annotated tag
git tag -a v0.1.0 -m "Release v0.1.0"

# Push tag to GitHub
git push origin v0.1.0
```

### Step 5: Monitor Workflow

1. Go to GitHub → Actions tab
2. Watch the "Release" workflow run
3. Verify builds complete successfully

### Step 6: Verify Release

1. Go to GitHub → Releases
2. Find the new release (v0.1.0)
3. Download and test binaries:
   ```bash
   # Download
   wget https://github.com/OWNER/REPO/releases/download/v0.1.0/tunnel-linux-amd64.tar.gz

   # Verify checksum
   wget https://github.com/OWNER/REPO/releases/download/v0.1.0/checksums-linux-amd64.txt
   sha256sum -c checksums-linux-amd64.txt

   # Extract and test
   tar -xzf tunnel-linux-amd64.tar.gz
   ./tunnel --version
   ```

## Version Numbering

We follow [Semantic Versioning](https://semver.org/):

- **MAJOR** (1.0.0): Incompatible API changes
- **MINOR** (0.1.0): Add functionality (backwards-compatible)
- **PATCH** (0.0.1): Bug fixes (backwards-compatible)

### Pre-release versions:
- `v0.1.0-alpha.1` - Alpha release
- `v0.1.0-beta.1` - Beta release
- `v0.1.0-rc.1` - Release candidate

## Troubleshooting

### Workflow fails on tag push

**Problem:** GitHub Actions workflow not triggering
**Solution:** Check permissions in repository Settings → Actions → General

### Build fails

**Problem:** Compilation errors
**Solution:**
1. Test locally: `cargo build --release`
2. Check CI logs for specific errors
3. Fix issues and delete/recreate tag:
   ```bash
   git tag -d v0.1.0
   git push origin :refs/tags/v0.1.0
   # Fix issues, commit, then recreate tag
   ```

### Release assets not uploading

**Problem:** softprops/action-gh-release fails
**Solution:** Ensure `permissions: contents: write` is set in workflow

### Checksums don't match

**Problem:** Downloaded file checksum doesn't match
**Solution:**
1. Re-download the file
2. Check if workflow completed successfully
3. Verify no files were modified after building

## Manual Release (Fallback)

If GitHub Actions is unavailable, create releases manually:

```bash
# Build binaries
cargo build --release --bin tunnel
cargo build --release --bin tunnel-exit-node

# Strip binaries
strip target/release/tunnel
strip target/release/tunnel-exit-node

# Create archives
tar -czf tunnel-linux-amd64.tar.gz -C target/release tunnel
tar -czf tunnel-exit-node-linux-amd64.tar.gz -C target/release tunnel-exit-node

# Create checksums
sha256sum *.tar.gz > checksums-linux-amd64.txt

# Create GitHub release manually and upload files
```

## CI/CD Best Practices

1. ✅ **Always test before tagging** - Run `cargo test` locally
2. ✅ **Use annotated tags** - Include release notes: `git tag -a v0.1.0 -m "message"`
3. ✅ **Verify checksums** - Always verify downloaded binaries
4. ✅ **Keep CHANGELOG updated** - Document all changes
5. ✅ **Test binaries** - Download and test release artifacts
6. ✅ **Use semantic versioning** - Follow semver conventions

## Automated Release Notes

GitHub automatically generates release notes from commits. To improve:

1. Use conventional commits:
   - `feat:` for new features
   - `fix:` for bug fixes
   - `docs:` for documentation
   - `chore:` for maintenance

2. Reference issues: `fix: resolve connection timeout (#123)`

3. Add breaking changes: `feat!: change API endpoint structure`

## Advanced: Release from CI

For automated releases on every merge to main:

```yaml
on:
  push:
    branches: [main]

# Add version bump and tag creation steps
# Use semantic-release or similar tool
```

This is not recommended for manual version control.
