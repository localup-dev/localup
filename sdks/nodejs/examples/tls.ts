/**
 * TLS Tunnel Example: Expose a local TLS server with SNI routing
 *
 * This example creates a simple TLS server and exposes it via LocalUp.
 * TLS tunnels use SNI (Server Name Indication) for routing, allowing
 * the relay to forward TLS connections without terminating them.
 *
 * Prerequisites:
 * 1. Set LOCALUP_AUTHTOKEN environment variable
 * 2. Set LOCALUP_RELAY environment variable (e.g., "tunnel.example.com:4443")
 * 3. Optionally set LOCALUP_TRANSPORT ("quic", "websocket", or "h2")
 * 4. Generate test certificates (see below)
 *
 * Generate self-signed certificates for testing:
 *   openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=localhost"
 *
 * Usage:
 *   LOCALUP_AUTHTOKEN=xxx LOCALUP_RELAY=tunnel.example.com:4443 bun run examples/tls.ts
 *
 * Testing:
 *   openssl s_client -connect <relay-host>:<port> -servername <your-sni-pattern>
 */

import localup, { setLogLevel } from "../src/index.ts";
import * as tls from "node:tls";
import * as fs from "node:fs";
import * as path from "node:path";

// Create a simple TLS server
function createTlsServer(port: number): Promise<void> {
  return new Promise((resolve, reject) => {
    // Check for certificate files
    const certPath = path.join(process.cwd(), "cert.pem");
    const keyPath = path.join(process.cwd(), "key.pem");

    if (!fs.existsSync(certPath) || !fs.existsSync(keyPath)) {
      console.log("Certificate files not found. Creating self-signed certificates...");

      // Generate self-signed certificate using openssl
      const { execSync } = require("node:child_process");
      try {
        execSync(
          `openssl req -x509 -newkey rsa:2048 -keyout "${keyPath}" -out "${certPath}" -days 365 -nodes -subj "/CN=localhost"`,
          { stdio: "inherit" }
        );
        console.log("Self-signed certificates created successfully.\n");
      } catch (err) {
        reject(new Error("Failed to create certificates. Please install openssl or create them manually."));
        return;
      }
    }

    const options: tls.TlsOptions = {
      key: fs.readFileSync(keyPath),
      cert: fs.readFileSync(certPath),
    };

    const server = tls.createServer(options, (socket) => {
      const clientAddr = `${socket.remoteAddress}:${socket.remotePort}`;
      console.log(`[TLS] Secure connection from: ${clientAddr}`);
      console.log(`[TLS] Protocol: ${socket.getProtocol()}`);
      console.log(`[TLS] Cipher: ${socket.getCipher()?.name}`);

      socket.on("data", (data) => {
        const message = data.toString().trim();
        console.log(`[TLS] Received: ${message}`);

        // Echo back
        socket.write(`Secure Echo: ${message}\n`);
      });

      socket.on("close", () => {
        console.log(`[TLS] Connection closed: ${clientAddr}`);
      });

      socket.on("error", (err) => {
        console.error(`[TLS] Socket error: ${err.message}`);
      });

      // Send welcome message
      socket.write("Welcome to the secure TLS server!\n");
      socket.write("Your connection is encrypted.\n");
    });

    server.listen(port, () => {
      console.log(`TLS server running on port ${port}`);
      resolve();
    });

    server.on("error", reject);
  });
}

async function main() {
  const PORT = 9443;
  const SNI_PATTERN = process.env.LOCALUP_SNI ?? "secure.example.com";

  // Enable debug logging to see ping/pong messages
  // setLogLevel("debug");

  // Start local TLS server
  await createTlsServer(PORT);

  // Create TLS tunnel with SNI routing
  console.log("\nCreating LocalUp TLS tunnel...");

  const listener = await localup.forward({
    addr: PORT,
    authtoken: process.env.LOCALUP_AUTHTOKEN,
    relay: process.env.LOCALUP_RELAY,
    transport: (process.env.LOCALUP_TRANSPORT as "quic" | "websocket" | "h2") ?? "quic",
    proto: "tls",
    domain: SNI_PATTERN, // SNI pattern for routing
    rejectUnauthorized: false,
  });

  console.log(`\nTLS Tunnel established!`);
  console.log(`   Tunnel ID: ${listener.tunnelId()}`);
  console.log(`   SNI Pattern: ${SNI_PATTERN}`);
  console.log(`\nEndpoints:`);
  for (const endpoint of listener.endpoints()) {
    console.log(`   - ${endpoint.publicUrl}`);
    if (endpoint.port) {
      console.log(`     Port: ${endpoint.port}`);
    }
  }
  console.log(`\nTest with:`);
  console.log(`   openssl s_client -connect <relay-host>:<port> -servername ${SNI_PATTERN}`);
  console.log("\nPress Ctrl+C to close");

  listener.on("close", () => {
    console.log("\nTunnel closed");
  });

  await listener.wait();
}

main().catch((err) => {
  console.error("Error:", err.message);
  process.exit(1);
});
