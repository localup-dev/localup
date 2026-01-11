/**
 * TCP Tunnel Example: Expose a local TCP server
 *
 * This example creates a simple TCP echo server and exposes it via LocalUp.
 * TCP tunnels get a dedicated port on the relay server.
 *
 * Prerequisites:
 * 1. Set LOCALUP_AUTHTOKEN environment variable
 * 2. Set LOCALUP_RELAY environment variable (e.g., "tunnel.example.com:4443")
 * 3. Optionally set LOCALUP_TRANSPORT ("quic", "websocket", or "h2")
 *
 * Usage:
 *   LOCALUP_AUTHTOKEN=xxx LOCALUP_RELAY=tunnel.example.com:4443 bun run examples/tcp.ts
 *
 * Testing:
 *   nc <relay-host> <allocated-port>
 *   # Type messages and see them echoed back
 */

import localup, { setLogLevel } from "../src/index.ts";
import * as net from "node:net";

// Create a simple TCP echo server
function createEchoServer(port: number): Promise<void> {
  return new Promise((resolve) => {
    const server = net.createServer((socket) => {
      const clientAddr = `${socket.remoteAddress}:${socket.remotePort}`;
      console.log(`[TCP] Client connected: ${clientAddr}`);

      socket.on("data", (data) => {
        const message = data.toString().trim();
        console.log(`[TCP] Received from ${clientAddr}: ${message}`);

        // Echo back with prefix
        socket.write(`Echo: ${message}\n`);
      });

      socket.on("close", () => {
        console.log(`[TCP] Client disconnected: ${clientAddr}`);
      });

      socket.on("error", (err) => {
        console.error(`[TCP] Socket error: ${err.message}`);
      });

      // Send welcome message
      socket.write("Welcome to the TCP echo server!\n");
      socket.write("Type anything and it will be echoed back.\n");
    });

    server.listen(port, () => {
      console.log(`TCP echo server running on port ${port}`);
      resolve();
    });
  });
}

async function main() {
  const PORT = 19000;

  // Enable debug logging to see ping/pong messages
  // setLogLevel("debug");

  // Start local TCP server
  await createEchoServer(PORT);

  // Create TCP tunnel
  console.log("\nCreating LocalUp TCP tunnel...");

  const listener = await localup.forward({
    addr: PORT,
    authtoken: process.env.LOCALUP_AUTHTOKEN,
    relay: process.env.LOCALUP_RELAY,
    transport: (process.env.LOCALUP_TRANSPORT as "quic" | "websocket" | "h2") ?? "quic",
    proto: "tcp",
    rejectUnauthorized: false,
  });

  console.log(`\nTCP Tunnel established!`);
  console.log(`   Tunnel ID: ${listener.tunnelId()}`);
  console.log(`\nEndpoints:`);
  for (const endpoint of listener.endpoints()) {
    console.log(`   - ${endpoint.publicUrl}`);
    if (endpoint.port) {
      console.log(`     Port: ${endpoint.port}`);
    }
  }
  console.log("\nTest with: nc <relay-host> <port>");
  console.log("Press Ctrl+C to close");

  listener.on("close", () => {
    console.log("\nTunnel closed");
  });

  await listener.wait();
}

main().catch((err) => {
  console.error("Error:", err.message);
  process.exit(1);
});
