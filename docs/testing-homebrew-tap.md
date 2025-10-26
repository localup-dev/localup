# Testing Homebrew Tap Locally

This guide shows how to test the Homebrew tap and formula locally before publishing.

## Prerequisites

- Homebrew installed (`brew --version`)
- Rust toolchain installed
- OpenSSL installed

## Step 1: Build the Binaries

First, we need to build the actual binaries that the formula will install:

```bash
# From the project root
cd /Users/davidviejo/projects/kfs/localup-dev

# Build the CLI tool
cd crates/tunnel-cli
cargo build --release

# Build the relay server
cd ../tunnel-exit-node
cargo build --release

# Verify binaries exist
ls -lh ../../target/release/tunnel-cli
ls -lh ../../target/release/tunnel-exit-node
```

## Step 2: Create Local Tarball

Create a tarball that mimics what would be in a GitHub Release:

```bash
# Back to project root
cd /Users/davidviejo/projects/kfs/localup-dev

# Create a test tarball
tar -czf /tmp/localup-test.tar.gz \
  -C target/release \
  tunnel-cli \
  tunnel-exit-node

# Calculate SHA256 (we'll need this)
shasum -a 256 /tmp/localup-test.tar.gz
```

## Step 3: Create a Test Formula

Create a test formula that points to the local tarball:

```bash
# Create test formula directory
mkdir -p /tmp/homebrew-test
cd /tmp/homebrew-test

# Create test formula
cat > localup-test.rb <<'EOF'
class LocalupTest < Formula
  desc "Geo-distributed tunnel system (TEST VERSION)"
  homepage "https://github.com/localup-dev/localup"
  version "0.1.0-test"
  license "MIT OR Apache-2.0"

  # Point to local tarball
  url "file:///tmp/localup-test.tar.gz"
  sha256 "REPLACE_WITH_SHA256_FROM_STEP_2"

  depends_on "openssl@3"

  def install
    bin.install "tunnel-cli" => "localup"
    bin.install "tunnel-exit-node" => "localup-relay"
  end

  def caveats
    <<~EOS
      🧪 TEST VERSION 🧪

      Localup has been installed with two commands:
        - localup        : Client CLI
        - localup-relay  : Relay server

      Quick test:
        localup-relay --version
        localup --version
    EOS
  end

  test do
    assert_match "tunnel-cli", shell_output("#{bin}/localup --version 2>&1", 1)
    assert_match "tunnel-exit-node", shell_output("#{bin}/localup-relay --version 2>&1", 1)
  end
end
EOF
```

Now update the SHA256 in the formula:

```bash
# Get the SHA256
SHA256=$(shasum -a 256 /tmp/localup-test.tar.gz | awk '{print $1}')

# Replace in formula
sed -i '' "s/REPLACE_WITH_SHA256_FROM_STEP_2/$SHA256/" localup-test.rb

# Verify
grep sha256 localup-test.rb
```

## Step 4: Test the Formula

### 4.1 Audit the Formula

```bash
brew audit --strict /tmp/homebrew-test/localup-test.rb
```

Expected output: No errors (warnings about GitHub are OK for local testing)

### 4.2 Install from the Formula

```bash
# Install
brew install /tmp/homebrew-test/localup-test.rb

# Check installation
which localup
which localup-relay

# Test the commands
localup --version
localup-relay --version
```

### 4.3 Test Functionality

```bash
# Terminal 1: Start relay (with test certificates)
cd /Users/davidviejo/projects/kfs/localup-dev

# Generate test cert if not exists
if [ ! -f cert.pem ]; then
  openssl req -x509 -newkey rsa:4096 -nodes \
    -keyout key.pem -out cert.pem -days 365 \
    -subj "/CN=localhost"
fi

# Start relay
localup-relay

# Terminal 2: Test with a simple HTTP server
python3 -m http.server 3000 &
HTTP_PID=$!

# Wait a moment for server to start
sleep 2

# Terminal 3: Create tunnel (if CLI supports it)
# Note: This might fail if CLI isn't fully implemented yet
localup http --port 3000 --relay localhost:4443 --subdomain test || echo "CLI not fully implemented yet"

# Clean up
kill $HTTP_PID
```

### 4.4 Test Uninstall

```bash
# Uninstall
brew uninstall localup-test

# Verify removed
which localup || echo "✅ localup removed"
which localup-relay || echo "✅ localup-relay removed"
```

## Step 5: Test the Real Formula (Against Project)

Test the actual formula in the repo:

```bash
cd /Users/davidviejo/projects/kfs/localup-dev

# Audit the formula
brew audit --strict Formula/localup-head.rb

# Install from HEAD (builds from source)
brew install Formula/localup-head.rb

# Test it works
localup-relay --version
localup --version

# Uninstall
brew uninstall localup-head
```

## Step 6: Test via Tap (Local Tap)

Test as if it were a real tap:

```bash
# Create a local tap
brew tap-new localup-dev/test-tap

# Find where brew creates taps
TAP_DIR=$(brew --repository)/Library/Taps/localup-dev/homebrew-test-tap

# Copy our formula there
cp /Users/davidviejo/projects/kfs/localup-dev/Formula/localup-head.rb \
   $TAP_DIR/Formula/

# Install from the tap
brew install localup-dev/test-tap/localup-head

# Test
localup --version
localup-relay --version

# Uninstall
brew uninstall localup-head

# Remove test tap
brew untap localup-dev/test-tap
```

## Quick Test Script

Save this as `test-homebrew.sh` in the project root:

```bash
#!/bin/bash
set -e

echo "🧪 Testing Homebrew Formula Locally"
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

PROJECT_ROOT="/Users/davidviejo/projects/kfs/localup-dev"
cd "$PROJECT_ROOT"

# Step 1: Build binaries
echo "📦 Step 1: Building binaries..."
cargo build --release -p tunnel-cli
cargo build --release -p tunnel-exit-node
echo -e "${GREEN}✓ Binaries built${NC}"
echo ""

# Step 2: Create tarball
echo "📦 Step 2: Creating tarball..."
tar -czf /tmp/localup-test.tar.gz \
  -C target/release \
  tunnel-cli \
  tunnel-exit-node
SHA256=$(shasum -a 256 /tmp/localup-test.tar.gz | awk '{print $1}')
echo "SHA256: $SHA256"
echo -e "${GREEN}✓ Tarball created${NC}"
echo ""

# Step 3: Create test formula
echo "📝 Step 3: Creating test formula..."
mkdir -p /tmp/homebrew-test
cat > /tmp/homebrew-test/localup-test.rb <<EOF
class LocalupTest < Formula
  desc "Geo-distributed tunnel system (TEST)"
  homepage "https://github.com/localup-dev/localup"
  version "0.1.0-test"
  url "file:///tmp/localup-test.tar.gz"
  sha256 "$SHA256"

  def install
    bin.install "tunnel-cli" => "localup"
    bin.install "tunnel-exit-node" => "localup-relay"
  end

  test do
    system "#{bin}/localup", "--version"
    system "#{bin}/localup-relay", "--version"
  end
end
EOF
echo -e "${GREEN}✓ Test formula created${NC}"
echo ""

# Step 4: Audit
echo "🔍 Step 4: Auditing formula..."
brew audit /tmp/homebrew-test/localup-test.rb || true
echo ""

# Step 5: Install
echo "📥 Step 5: Installing formula..."
brew install /tmp/homebrew-test/localup-test.rb
echo -e "${GREEN}✓ Installed${NC}"
echo ""

# Step 6: Test
echo "✅ Step 6: Testing installation..."
echo "  localup version:"
localup --version || echo -e "${RED}✗ localup failed${NC}"
echo ""
echo "  localup-relay version:"
localup-relay --version || echo -e "${RED}✗ localup-relay failed${NC}"
echo ""

# Step 7: Verify paths
echo "📍 Step 7: Verifying installation paths..."
echo "  localup: $(which localup)"
echo "  localup-relay: $(which localup-relay)"
echo ""

echo "🎉 Test complete!"
echo ""
echo "To uninstall: brew uninstall localup-test"
echo "To clean up: rm -rf /tmp/homebrew-test /tmp/localup-test.tar.gz"
```

Make it executable and run:

```bash
chmod +x test-homebrew.sh
./test-homebrew.sh
```

## Cleanup

After testing:

```bash
# Uninstall test formula
brew uninstall localup-test 2>/dev/null || true
brew uninstall localup-head 2>/dev/null || true

# Remove test files
rm -rf /tmp/homebrew-test
rm -f /tmp/localup-test.tar.gz

# Remove test tap (if created)
brew untap localup-dev/test-tap 2>/dev/null || true
```

## Troubleshooting

### Error: "SHA256 mismatch"
- Rebuild the tarball
- Recalculate SHA256: `shasum -a 256 /tmp/localup-test.tar.gz`
- Update formula

### Error: "Binary not found"
- Check tarball contents: `tar -tzf /tmp/localup-test.tar.gz`
- Ensure binaries exist in `target/release/`

### Error: "Permission denied"
- Make binaries executable: `chmod +x target/release/tunnel-*`

### CLI/Relay not working
- Check if they're fully implemented
- Test directly: `./target/release/tunnel-cli --version`
- Check for missing dependencies: `otool -L target/release/tunnel-cli` (macOS)

## Next Steps

Once local testing passes:
1. Create a GitHub Release with proper tarballs
2. Update Formula/localup.rb with real URLs and SHA256s
3. Test against the actual GitHub Release
4. Publish the tap: `brew tap localup-dev/localup`
