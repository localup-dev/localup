class LocalupHead < Formula
  desc "Geo-distributed tunnel system (HEAD version, built from source)"
  homepage "https://github.com/localup-dev/localup"
  head "https://github.com/localup-dev/localup.git", branch: "main"
  license "MIT OR Apache-2.0"

  depends_on "rust" => :build
  depends_on "openssl@3"

  def install
    # Build the release binaries
    system "cargo", "build", "--release", "-p", "tunnel-cli"
    system "cargo", "build", "--release", "-p", "tunnel-exit-node"

    # Install binaries (note: binary names in target/release)
    bin.install "target/release/tunnel" => "localup"
    bin.install "target/release/tunnel-exit-node" => "localup-relay"
  end

  def caveats
    <<~EOS
      This is the HEAD version built from source.

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
    EOS
  end

  test do
    assert_match "localup", shell_output("#{bin}/localup --version")
    assert_match "tunnel-exit-node", shell_output("#{bin}/localup-relay --version")
  end
end
