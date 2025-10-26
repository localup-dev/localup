# Homebrew Tap Setup Guide

This guide explains how to set up and maintain the Homebrew tap for Localup.

## Overview

The Homebrew tap allows users to install Localup with a simple `brew install` command. The tap lives in this repository under the `Formula/` directory.

## Repository Structure

```
localup-dev/
├── Formula/
│   ├── localup.rb         # Main formula (stable releases)
│   └── localup-head.rb    # HEAD formula (latest from main branch)
├── scripts/
│   └── build-release.sh   # Script to build release binaries
├── HOMEBREW.md            # Homebrew tap documentation
└── .github/workflows/
    └── release.yml        # Automated release workflow
```

## For Users

### Installation

```bash
# Add the tap
brew tap localup-dev/localup

# Install stable release
brew install localup

# Or install from HEAD (latest source)
brew install localup-head
```

### Commands Installed

- **`localup`** - Client CLI for creating tunnels
- **`localup-relay`** - Relay server (exit node)

### Usage

```bash
# Start relay server
localup-relay

# Create tunnel
localup http --port 3000 --relay localhost:4443
```

## For Maintainers

### Creating a Release

#### 1. Tag a New Version

```bash
git tag v0.1.0
git push origin v0.1.0
```

This triggers the GitHub Actions workflow that:
- Builds binaries for all platforms
- Creates a GitHub Release
- Automatically updates the Homebrew formula

#### 2. Manual Release (if needed)

If you need to create binaries manually:

```bash
# Build for current platform
./scripts/build-release.sh 0.1.0

# This creates:
# - dist/localup-<platform>-<arch>.tar.gz
# - dist/localup-<platform>-<arch>.tar.gz.sha256
```

Upload to GitHub Releases and update the formula SHA256 values.

### Updating the Formula

The formula is automatically updated by the release workflow. If you need to update manually:

1. **Update version:**
   ```ruby
   version "0.1.0"
   ```

2. **Update URLs:**
   ```ruby
   url "https://github.com/localup-dev/localup/releases/download/v0.1.0/localup-darwin-arm64.tar.gz"
   ```

3. **Update SHA256 checksums:**
   ```bash
   # Download the release
   curl -LO https://github.com/localup-dev/localup/releases/download/v0.1.0/localup-darwin-arm64.tar.gz

   # Calculate SHA256
   shasum -a 256 localup-darwin-arm64.tar.gz
   ```

   Update in formula:
   ```ruby
   sha256 "abc123..."
   ```

4. **Test the formula:**
   ```bash
   brew install --build-from-source ./Formula/localup.rb
   brew test localup
   brew audit --strict localup
   ```

5. **Commit and push:**
   ```bash
   git add Formula/localup.rb
   git commit -m "Update Homebrew formula to v0.1.0"
   git push
   ```

### Testing the Formula Locally

```bash
# Lint the formula
brew audit --strict Formula/localup.rb

# Install from local formula
brew install --build-from-source ./Formula/localup.rb

# Test the installation
brew test localup
localup --version
localup-relay --version

# Uninstall
brew uninstall localup
```

### Building Multi-Platform Releases

The GitHub Actions workflow builds for:
- macOS ARM64 (Apple Silicon)
- macOS AMD64 (Intel)
- Linux ARM64
- Linux AMD64

To build manually for multiple platforms, use GitHub Actions or cross-compilation tools like:
- `cross` for Linux ARM64
- GitHub Actions runners for macOS variants

### Troubleshooting

**Formula audit failures:**
```bash
brew audit --strict localup --verbose
```

**SHA256 mismatch:**
- Ensure you're downloading the correct binary
- Recalculate: `shasum -a 256 <file>`
- Update the formula with the correct hash

**Binary not found:**
- Check that binary names in formula match release artifacts
- Verify tarball contains `tunnel-cli` and `tunnel-exit-node`

**Installation errors:**
- Test locally first: `brew install --build-from-source ./Formula/localup.rb`
- Check formula syntax: `brew audit localup`

## Supported Platforms

| Platform | Architecture | Binary Name |
|----------|-------------|-------------|
| macOS | ARM64 (Apple Silicon) | `localup-darwin-arm64.tar.gz` |
| macOS | AMD64 (Intel) | `localup-darwin-amd64.tar.gz` |
| Linux | ARM64 | `localup-linux-arm64.tar.gz` |
| Linux | AMD64 | `localup-linux-amd64.tar.gz` |

## Release Checklist

Before releasing a new version:

- [ ] All tests pass: `cargo test --all`
- [ ] Linting passes: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Documentation updated (README, CHANGELOG)
- [ ] Version bumped in relevant files
- [ ] Tag created: `git tag v0.x.x`
- [ ] Tag pushed: `git push origin v0.x.x`
- [ ] GitHub Actions workflow completed successfully
- [ ] Binaries available in GitHub Releases
- [ ] Formula updated automatically (verify SHA256 hashes)
- [ ] Test installation: `brew install localup-dev/localup/localup`
- [ ] Test both `localup` and `localup-relay` commands work

## Resources

- [Homebrew Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)
- [Homebrew Acceptable Formulae](https://docs.brew.sh/Acceptable-Formulae)
- [How to Create and Maintain a Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)
- [Homebrew GitHub Actions](https://github.com/marketplace/actions/setup-homebrew)

## License

Same as main project: MIT OR Apache-2.0
