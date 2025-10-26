# Scripts Directory

Utility scripts for building, releasing, and managing localup.

## 📋 Quick Reference

### Formula Update Scripts

| Script | When to Use | Interactive? | Auto-commit? |
|--------|-------------|--------------|--------------|
| **manual-formula-update.sh** | Best for first-time users, guided process | ✅ Yes | ✅ Optional |
| **quick-formula-update.sh** | Fast updates, automation scripts | ❌ No | ❌ No |
| **update-homebrew-formula.sh** | Called by other scripts, CI/CD | ❌ No | ❌ No |

### Build & Release Scripts

| Script | Purpose |
|--------|---------|
| **build-release.sh** | Build release binaries locally |
| **create-release.sh** | Legacy release script |
| **install.sh** | Install localup from source |

---

## 🔧 Formula Update Scripts

### 1. Interactive Update (Recommended)

**Use when:** You want a guided experience with confirmations

```bash
./scripts/manual-formula-update.sh
```

**Features:**
- ✅ Auto-detects latest release version
- ✅ Downloads checksums from GitHub automatically
- ✅ Determines stable vs beta formula
- ✅ Shows diff of changes
- ✅ Prompts before committing
- ✅ Prompts before pushing
- ✅ Can test installation
- ✅ Colorful output
- ✅ Error handling

**Example Output:**
```
═══════════════════════════════════════════════
   Homebrew Formula Manual Update Script
═══════════════════════════════════════════════

📋 Step 1: Detecting latest release...
  Latest tag: v0.1.0

❓ Which version do you want to update the formula for?
   Press Enter to use v0.1.0, or type a different version:
v0.1.0

🔍 Step 2: Checking if release exists on GitHub...
  ✓ Release v0.1.0 exists on GitHub

📥 Step 3: Downloading SHA256SUMS.txt from release...
  ✓ Downloaded SHA256SUMS.txt

📝 Step 4: Determining formula type...
  Detected: STABLE
  Will update: Formula/localup.rb

🔧 Step 5: Updating formula...
...
```

---

### 2. Quick Update (Fast, No Prompts)

**Use when:** You want a fast update without prompts

```bash
# Update for latest tag
./scripts/quick-formula-update.sh

# Or specify version
./scripts/quick-formula-update.sh v0.1.0
./scripts/quick-formula-update.sh v0.0.1-beta2
```

**Features:**
- ⚡ Fast execution
- ✅ Auto-detects stable vs beta
- ✅ Downloads checksums automatically
- ❌ No prompts (good for scripts)
- ❌ Doesn't commit (you commit manually)

**Example Output:**
```
📋 Updating formula for version: v0.1.0
📥 Downloading SHA256SUMS.txt...
📝 Updating STABLE formula
✅ Done! Formula updated: Formula/localup.rb

Next steps:
  git add Formula/localup.rb
  git commit -m 'chore: update Homebrew formula for v0.1.0'
  git push
```

---

### 3. Direct Script (Low-level)

**Use when:** Called by CI/CD or other scripts

```bash
# Stable release
./scripts/update-homebrew-formula.sh v0.1.0 /path/to/SHA256SUMS.txt

# Beta release
./scripts/update-homebrew-formula.sh v0.0.1-beta2 /path/to/SHA256SUMS.txt Formula/localup-beta.rb
```

**Parameters:**
1. `<version>` - Version to update (e.g., `v0.1.0`)
2. `<checksums-file>` - Path to SHA256SUMS.txt file
3. `[formula-file]` - Optional: Formula file to update (auto-detected if omitted)

---

## 📦 Build Scripts

### build-release.sh

Build release binaries locally

```bash
./scripts/build-release.sh
```

### install.sh

Install localup from source

```bash
./scripts/install.sh
```

---

## 🚀 Common Workflows

### Update Formula After GitHub Release

```bash
# Option 1: Interactive (recommended)
./scripts/manual-formula-update.sh

# Option 2: Quick
./scripts/quick-formula-update.sh v0.1.0
git add Formula/localup.rb
git commit -m "chore: update Homebrew formula for v0.1.0"
git push
```

### Update Beta Formula

```bash
# Interactive
./scripts/manual-formula-update.sh
# (it will auto-detect it's a beta version)

# Quick
./scripts/quick-formula-update.sh v0.0.1-beta2
git add Formula/localup-beta.rb
git commit -m "chore: update Homebrew beta formula for v0.0.1-beta2"
git push
```

### Test Formula Locally

```bash
# After updating the formula
brew install Formula/localup.rb
localup --version
localup-relay --version
brew uninstall localup
```

---

## 🛠️ Requirements

### For Formula Update Scripts

**Required:**
- `git` - For detecting tags and committing
- `bash` - For running scripts

**Optional (but recommended):**
- `gh` (GitHub CLI) - For downloading release assets
  ```bash
  brew install gh
  gh auth login
  ```

  If not installed, scripts will fall back to `curl`

### For Build Scripts

- Rust toolchain (`rustup`)
- Bun (for webapps)
- Platform-specific build tools

---

## 🐛 Troubleshooting

### "Failed to download SHA256SUMS.txt"

**Problem:** The release doesn't have SHA256SUMS.txt attached

**Solution:**
1. Make sure the GitHub release exists
2. Check that SHA256SUMS.txt is attached to the release
3. If using `gh`, make sure you're authenticated: `gh auth login`

### "No git tags found"

**Problem:** No release tags exist in the repository

**Solution:**
```bash
# Create a tag first
git tag v0.1.0
git push origin v0.1.0

# Then run the script
./scripts/manual-formula-update.sh
```

### "Release not found on GitHub"

**Problem:** The tag exists locally but the release isn't published

**Solution:**
```bash
# Push the tag to trigger the release workflow
git push origin v0.1.0

# Wait for GitHub Actions to complete
# Then run the formula update script
```

---

## 📖 See Also

- [Formula README](../Formula/README.md) - Homebrew formula documentation
- [Releasing Guide](../docs/RELEASING.md) - Complete release process
- [GitHub Actions](.github/workflows/release.yml) - Automated release workflow
