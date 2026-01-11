/**
 * Express Example: Expose an Express server
 *
 * This example creates a simple Express server and exposes it via LocalUp.
 *
 * Prerequisites:
 * 1. Set LOCALUP_AUTHTOKEN environment variable
 * 2. Set LOCALUP_RELAY environment variable (e.g., "tunnel.example.com:4443")
 * 3. Optionally set LOCALUP_TRANSPORT ("quic", "websocket", or "h2")
 *
 * Usage:
 *   LOCALUP_AUTHTOKEN=xxx LOCALUP_RELAY=tunnel.example.com:4443 bun run examples/express.ts
 */

import localup, { setLogLevel } from "../src/index.ts";
import * as http from "node:http";

// Minimal Express-like server for demo
function createServer(port: number): Promise<void> {
  return new Promise((resolve) => {
    const server = http.createServer((req, res) => {
      const now = new Date().toISOString();
      console.log(`[${now}] ${req.method} ${req.url}`);

      // Simple router
      if (req.url === "/") {
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ message: "Hello from LocalUp!", timestamp: now }));
      } else if (req.url === "/health") {
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ status: "ok" }));
      } else {
        res.writeHead(404, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: "Not Found" }));
      }
    });

    server.listen(port, () => {
      console.log(`Local server running on http://localhost:${port}`);
      resolve();
    });
  });
}

async function main() {
  const PORT = 3000;

  // Enable debug logging to see ping/pong messages
  // setLogLevel("debug");

  // Start local server
  await createServer(PORT);

  // Create tunnel
  console.log("\nCreating LocalUp tunnel...");

  const listener = await localup.forward({
    addr: PORT,
    authtoken: process.env.LOCALUP_AUTHTOKEN,
    domain: "express-demo",
    relay: process.env.LOCALUP_RELAY,
    transport: (process.env.LOCALUP_TRANSPORT as "quic" | "websocket" | "h2") ?? "quic",
    rejectUnauthorized: false,
  });

  console.log(`\nTunnel established!`);
  console.log(`   Public URL: ${listener.url()}`);
  console.log(`   Local:      http://localhost:${PORT}`);
  console.log(`\nEndpoints:`);
  for (const endpoint of listener.endpoints()) {
    console.log(`   - ${endpoint.publicUrl}`);
  }
  console.log("\nPress Ctrl+C to close");

  // Log requests
  listener.on("request", ({ method, path, status }) => {
    const statusColor = status < 400 ? "\x1b[32m" : "\x1b[31m";
    console.log(`${statusColor}[${method}] ${path} -> ${status}\x1b[0m`);
  });

  await listener.wait();
}

main().catch((err) => {
  console.error("Error:", err.message);
  process.exit(1);
});
