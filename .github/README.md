# GitHub Actions Workflows

This directory contains CI/CD workflows for automated testing, building, and releasing.

## Workflows

### 📋 CI Workflow ([`ci.yml`](workflows/ci.yml))
**Triggers:** Push to main/master/develop, Pull Requests

**What it does:**
- Runs full test suite (including benchmark smoke test)
- Checks code formatting with `cargo fmt`
- Runs Clippy linting
- Builds entire workspace in release mode

**Optimizations:**
- Uses `mold` linker for **2-3x faster** linking
- Pins Rust version (1.90.0) for reproducible builds
- Caches dependencies and build artifacts

### 🚀 Release Workflow ([`release.yml`](workflows/release.yml))
**Triggers:** Push tags matching `v*.*.*` (e.g., v1.0.0)

**What it builds:**
- `tunnel` - CLI client binary
- `tunnel-exit-node` - Exit node server binary

**Outputs:**
- `tunnel-linux-amd64.tar.gz`
- `tunnel-exit-node-linux-amd64.tar.gz`
- `checksums-linux-amd64.txt`

**Optimizations:**
- Uses `mold` linker for **faster builds** (~30-50% faster linking)
- Strips binaries for smaller size
- Pins Rust 1.90.0 for reproducible builds
- Caches all dependencies

## Performance Benefits

### Mold Linker
- **2-3x faster** linking compared to GNU ld
- **30-50% faster** overall build times for large projects
- Parallel linking by default
- Lower memory usage

### Rust Version Pinning
- **Reproducible builds** across all environments
- **Predictable behavior** - no surprises from Rust updates
- **Security** - control when to adopt new Rust versions

## Quick Start

### Creating a Release

**Option 1: Use the helper script**
```bash
./scripts/create-release.sh 0.1.0
```

**Option 2: Manual**
```bash
# Create and push tag
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0

# Monitor at: https://github.com/OWNER/REPO/actions
```

### Testing Locally

```bash
# Run tests (same as CI)
cargo test --workspace

# Check formatting
cargo fmt --all -- --check

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Build release binaries
cargo build --release
```

## Updating Rust Version

To update the Rust version used in CI:

1. Update `RUST_VERSION` in both workflows:
   ```yaml
   env:
     RUST_VERSION: "1.91.0"  # New version
   ```

2. Test locally first:
   ```bash
   rustup install 1.91.0
   rustup default 1.91.0
   cargo test --workspace
   ```

3. Commit and push changes

## Multi-Platform Builds (Optional)

An example multi-platform workflow is available:
- [`release-multiplatform.yml.example`](workflows/release-multiplatform.yml.example)

Supports:
- Linux (AMD64, ARM64)
- macOS (Intel, Apple Silicon)
- Windows (AMD64)

To enable:
```bash
mv workflows/release-multiplatform.yml.example workflows/release-multiplatform.yml
rm workflows/release.yml  # Optional
```

## Troubleshooting

### Workflow not triggering
- Check repository Settings → Actions → General
- Ensure Actions are enabled
- Verify tag format matches `v*.*.*`

### Build failures
1. Test locally: `cargo build --release`
2. Check workflow logs in Actions tab
3. Verify Rust version compatibility

### Mold linker errors
If mold is not available or causes issues, remove the mold steps from workflows.

## Cache Management

Caches are automatically managed:
- **Registry**: `~/.cargo/registry`
- **Git index**: `~/.cargo/git`
- **Build artifacts**: `target/`

Caches expire after 7 days of inactivity.

To clear caches: Repository Settings → Actions → Caches → Delete

## Documentation

- [Full Release Guide](.github/RELEASE.md)
- [Benchmark Testing](../BENCHMARKS.md)
- [Project Structure](../CLAUDE.md)

## Monitoring

View workflow runs:
- **All workflows**: https://github.com/OWNER/REPO/actions
- **Releases**: https://github.com/OWNER/REPO/releases
- **CI status**: Badge in README (add if needed)

## Status Badges

Add to README.md:

```markdown
[![CI](https://github.com/OWNER/REPO/workflows/CI/badge.svg)](https://github.com/OWNER/REPO/actions/workflows/ci.yml)
[![Release](https://github.com/OWNER/REPO/workflows/Release/badge.svg)](https://github.com/OWNER/REPO/actions/workflows/release.yml)
```
