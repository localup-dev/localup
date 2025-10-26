class LocalupBeta < Formula
  desc "Geo-distributed tunnel system (BETA/PRE-RELEASE)"
  homepage "https://github.com/localup-dev/localup"
  version "0.0.0-beta"
  license "MIT OR Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/localup-dev/localup/releases/download/v0.0.0-beta/tunnel-macos-arm64.tar.gz"
      sha256 "PLACEHOLDER_ARM64_SHA256"
    else
      url "https://github.com/localup-dev/localup/releases/download/v0.0.0-beta/tunnel-macos-amd64.tar.gz"
      sha256 "PLACEHOLDER_AMD64_SHA256"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/localup-dev/localup/releases/download/v0.0.0-beta/tunnel-linux-arm64.tar.gz"
      sha256 "PLACEHOLDER_LINUX_ARM64_SHA256"
    else
      url "https://github.com/localup-dev/localup/releases/download/v0.0.0-beta/tunnel-linux-amd64.tar.gz"
      sha256 "PLACEHOLDER_LINUX_AMD64_SHA256"
    end
  end

  depends_on "openssl@3"

  def install
    bin.install "tunnel" => "localup"
    bin.install "tunnel-exit-node" => "localup-relay"

    # Generate shell completions (if supported)
    # generate_completions_from_executable(bin/"localup", "completion")

    # Install man pages if available
    # man1.install "man/localup.1"
  end

  def caveats
    <<~EOS
      ⚠️  This is a PRE-RELEASE version (0.0.0-beta)
      For stable releases, use: brew install localup-dev/localup/localup

      Localup has been installed with two commands:
        - localup        : Client CLI for creating tunnels
        - localup-relay  : Relay server (exit node) for hosting

      Quick start:
        # Start a relay server (development)
        localup-relay

        # Create a tunnel (in another terminal)
        localup http --port 3000 --relay localhost:4443

      For production setup, see:
        https://github.com/localup-dev/localup#relay-server-setup

      To generate a certificate for HTTPS:
        openssl req -x509 -newkey rsa:4096 -nodes \\
          -keyout key.pem -out cert.pem -days 365 \\
          -subj "/CN=localhost"
    EOS
  end

  test do
    # Test that the binaries are installed and executable
    assert_match version.to_s, shell_output("#{bin}/localup --version")
    assert_match "tunnel-exit-node", shell_output("#{bin}/localup-relay --version")
  end
end
