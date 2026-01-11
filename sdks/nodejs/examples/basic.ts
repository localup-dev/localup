/**
 * Basic Example: Expose a local HTTP server
 *
 * Prerequisites:
 * 1. Start a local server on port 8080
 * 2. Set LOCALUP_AUTHTOKEN environment variable
 * 3. Set LOCALUP_RELAY environment variable (optional, defaults to localhost:4443)
 *
 * Usage:
 *   bun run examples/basic.ts
 */

import localup from "../src/index.ts";

async function main() {
  console.log("Starting LocalUp tunnel...");

  const listener = await localup.forward({
    // The port your app is running on
    addr: 8080,

    // Authentication token (from env or explicit)
    authtoken: process.env.LOCALUP_AUTHTOKEN,

    // Subdomain for your tunnel
    domain: "test123",

    // Relay server address (optional)
    relay: "tunnel.kfs.es:4443",

    // Skip TLS verification for local development
    rejectUnauthorized: false,
  });

  console.log(`Tunnel established at: ${listener.url()}`);
  console.log(`Tunnel ID: ${listener.tunnelId()}`);
  console.log("Press Ctrl+C to close the tunnel");

  // Handle events
  listener.on("request", (info) => {
    console.log(`[${info.method}] ${info.path} -> ${info.status}`);
  });

  listener.on("close", () => {
    console.log("Tunnel closed");
  });

  // Keep the process running
  await listener.wait();
}

main().catch((err) => {
  console.error("Error:", err.message);
  process.exit(1);
});
