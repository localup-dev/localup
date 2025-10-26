# Homebrew Formulae

This directory contains Homebrew formulae for installing `localup`.

## Available Formulae

### `localup.rb` (Stable Release)

Installs the latest stable release from GitHub releases.

**Auto-updated**: This formula is automatically updated by the release workflow when a new version is tagged.

**Installation**:
```bash
# Option 1: Install directly from this repo
brew install localup-dev/localup/localup

# Option 2: Tap first, then install
brew tap localup-dev/localup
brew install localup
```

### `localup-head.rb` (Development)

Builds and installs from the latest `main` branch source code.

**Installation**:
```bash
# Install HEAD version
brew install --HEAD localup-dev/localup/localup

# Or install local formula file
brew install Formula/localup-head.rb
```

## How It Works

### Release Process

When you create a new release tag (e.g., `v0.1.0`):

1. **GitHub Actions** builds binaries for all platforms
2. **SHA256 checksums** are calculated for each binary
3. **GitHub Release** is created with binaries and checksums
4. **Manual step**: Maintainer updates the formula with:
   ```bash
   ./scripts/manual-formula-update.sh
   # or
   ./scripts/quick-formula-update.sh v0.1.0
   ```
5. **Formula is updated** with:
   - New version number
   - Updated download URLs
   - Real SHA256 hashes (replaces placeholders)
6. **Commit and push** the updated formula

### Manual Formula Update

We provide two helper scripts for manual updates:

#### Option 1: Interactive Script (Recommended)

```bash
# Run the interactive script - it will guide you through everything
./scripts/manual-formula-update.sh

# The script will:
# ✓ Detect the latest release version
# ✓ Download checksums from GitHub
# ✓ Determine which formula to update (stable vs beta)
# ✓ Update the formula
# ✓ Show you the changes
# ✓ Ask if you want to commit & push
# ✓ Optionally test the installation
```

#### Option 2: Quick Update (No Prompts)

```bash
# Update for latest tag
./scripts/quick-formula-update.sh

# Or specify a version
./scripts/quick-formula-update.sh v0.1.0
./scripts/quick-formula-update.sh v0.0.1-beta2

# Then commit manually
git add Formula/localup.rb  # or Formula/localup-beta.rb
git commit -m "chore: update Homebrew formula for v0.1.0"
git push
```

#### Option 3: Direct Script Call

```bash
# After creating a release, run:
./scripts/update-homebrew-formula.sh v0.1.0 release/SHA256SUMS.txt

# For beta releases:
./scripts/update-homebrew-formula.sh v0.0.1-beta2 release/SHA256SUMS.txt Formula/localup-beta.rb

# Commit the changes
git add Formula/localup.rb
git commit -m "chore: update Homebrew formula for v0.1.0"
git push
```

## Testing Formulae Locally

### Test Stable Formula

```bash
# Use the test script
./test-homebrew.sh

# Or manually test
brew install Formula/localup.rb
localup --version
localup-relay --version
brew uninstall localup
```

### Test HEAD Formula

```bash
brew install Formula/localup-head.rb
localup --version
localup-relay --version
brew uninstall localup-head
```

### Audit Formula

```bash
brew audit --strict Formula/localup.rb
brew audit --strict Formula/localup-head.rb
```

## Setting Up a Homebrew Tap

To make installation easier for users, you can create a tap repository:

### 1. Create Tap Repository

Create a new repository: `github.com/localup-dev/homebrew-localup`

### 2. Add Formula to Tap

```bash
# Clone the tap repo
git clone https://github.com/localup-dev/homebrew-localup.git

# Copy formula
cp Formula/localup.rb homebrew-localup/Formula/

# Commit and push
cd homebrew-localup
git add Formula/localup.rb
git commit -m "Add localup formula"
git push
```

### 3. Update Release Workflow

Modify `.github/workflows/release.yml` to push the updated formula to the tap repository instead of (or in addition to) this repo.

### 4. Users Install

```bash
brew tap localup-dev/localup
brew install localup
```

## Installed Commands

Both formulae install two commands:

- **`localup`** - Client CLI for creating tunnels (built from `tunnel-cli`)
- **`localup-relay`** - Relay server for hosting exit nodes (built from `tunnel-exit-node`)

## Dependencies

Both formulae depend on:
- `openssl@3` - Required for TLS/HTTPS functionality

## Troubleshooting

### Formula Not Found

```bash
# Error: No available formula with the name "localup"
```

**Solution**: The formula needs to be in a tap or installed from a file:
```bash
brew install Formula/localup.rb
# OR
brew tap localup-dev/localup
brew install localup
```

### SHA256 Mismatch

```bash
# Error: SHA256 mismatch
```

**Solution**: The release binaries may have changed. Re-run the formula update script:
```bash
./scripts/update-homebrew-formula.sh v0.1.0 release/SHA256SUMS.txt
```

### Binary Not Found

```bash
# Error: No such file or directory @ rb_sysopen - /path/to/tunnel-cli
```

**Solution**: The archive structure may have changed. Ensure the release archives contain `tunnel` and `tunnel-exit-node` at the root level.

## Resources

- [Homebrew Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)
- [Homebrew Acceptable Formulae](https://docs.brew.sh/Acceptable-Formulae)
- [Creating Taps](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)
