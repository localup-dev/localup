#!/usr/bin/env bun
/**
 * Test script to verify the SDK works against a real LocalUp relay
 *
 * QUICK START:
 *
 * Terminal 1 - Start a local HTTP server (the app to expose):
 *   python3 -m http.server 8080
 *
 * Terminal 2 - Run the test against a QUIC relay:
 *   # Install QUIC support (one-time)
 *   bun add @matrixai/quic
 *
 *   # Generate token (use your relay's secret)
 *   cargo run -p localup-cli -- generate-token \
 *     --secret "your-secret" \
 *     --localup-id "test-sdk"
 *
 *   # Run with QUIC
 *   LOCALUP_AUTHTOKEN="eyJ..." \
 *   LOCALUP_RELAY="tunnel.kfs.es:4443" \
 *   LOCALUP_TRANSPORT="quic" \
 *   bun run examples/test-against-relay.ts
 *
 *   # Or with WebSocket (no extra dependencies)
 *   LOCALUP_AUTHTOKEN="eyJ..." \
 *   LOCALUP_RELAY="localhost:4443" \
 *   LOCALUP_TRANSPORT="websocket" \
 *   bun run examples/test-against-relay.ts
 *
 * TRANSPORT OPTIONS:
 *   Set LOCALUP_TRANSPORT to:
 *   - quic       (best performance, requires: bun add @matrixai/quic)
 *   - websocket  (good compatibility, built-in)
 *   - h2         (HTTP/2, maximum compatibility, built-in)
 */

import localup, { isQuicAvailable, getQuicUnavailableReason } from "../src/index.ts";

async function main() {
  console.log("=".repeat(60));
  console.log("LocalUp Node.js SDK Integration Test");
  console.log("=".repeat(60));
  console.log();

  // Check prerequisites
  const authToken = process.env.LOCALUP_AUTHTOKEN;
  const relay = process.env.LOCALUP_RELAY ?? "localhost:4443";
  const localPort = parseInt(process.env.LOCAL_PORT ?? "8080", 10);
  const transport = (process.env.LOCALUP_TRANSPORT ?? "quic") as "quic" | "websocket" | "h2";

  if (!authToken) {
    console.error("ERROR: LOCALUP_AUTHTOKEN environment variable is required");
    console.error("\nGenerate a token with:");
    console.error('  cargo run -p localup-cli -- generate-token --secret "your-secret" --localup-id "test-sdk"');
    console.error("\nThen run:");
    console.error('  LOCALUP_AUTHTOKEN="<paste-token-here>" LOCALUP_TRANSPORT="quic" node --experimental-quic examples/test-against-relay.ts');
    process.exit(1);
  }

  console.log(`Configuration:`);
  console.log(`  Relay:       ${relay}`);
  console.log(`  Local Port:  ${localPort}`);
  console.log(`  Transport:   ${transport}`);
  console.log(`  Token:       ${authToken.substring(0, 30)}...`);
  console.log();

  // Check QUIC availability if using QUIC transport
  if (transport === "quic") {
    const quicAvailable = await isQuicAvailable();
    if (quicAvailable) {
      console.log("✓ QUIC is available (using @matrixai/quic)");
    } else {
      console.error("✗ QUIC is NOT available");
      console.error(`  Reason: ${getQuicUnavailableReason()}`);
      console.error("\n  Options:");
      console.error("    1. Install QUIC: bun add @matrixai/quic");
      console.error("    2. Or use WebSocket: LOCALUP_TRANSPORT=websocket bun run examples/test-against-relay.ts");
      process.exit(1);
    }
    console.log();
  }

  try {
    console.log("[1/4] Connecting to relay...");

    const listener = await localup.forward({
      addr: localPort,
      authtoken: authToken,
      domain: "test-sdk",
      relay: relay,
      transport: transport,
      rejectUnauthorized: false, // Allow self-signed certs
      proto: "http",
    });

    console.log();
    console.log("[2/4] Tunnel established!");
    console.log(`  Public URL:  ${listener.url()}`);
    console.log(`  Tunnel ID:   ${listener.tunnelId()}`);

    console.log(`  Endpoints:`);
    for (const ep of listener.endpoints()) {
      console.log(`    - ${ep.publicUrl}`);
    }

    console.log();
    console.log("[3/4] Listening for requests...");
    console.log();
    console.log("  Test with:");
    console.log(`    curl ${listener.url()}`);
    console.log();

    listener.on("request", ({ method, path, status }) => {
      const statusColor = status < 400 ? "\x1b[32m" : "\x1b[31m";
      console.log(`  ${statusColor}[${method}] ${path} -> ${status}\x1b[0m`);
    });

    listener.on("disconnect", (reason: string) => {
      console.log(`  \x1b[33mDisconnected: ${reason}\x1b[0m`);
    });

    listener.on("close", () => {
      console.log("[4/4] Tunnel closed");
    });

    // Keep running for 5 minutes or until Ctrl+C
    console.log("Press Ctrl+C to close (auto-closes in 5 minutes)");
    console.log();

    const timeout = setTimeout(() => {
      console.log("\nTimeout reached, closing tunnel...");
      listener.close();
    }, 5 * 60 * 1000);

    process.on("SIGINT", () => {
      clearTimeout(timeout);
      console.log("\nClosing tunnel...");
      listener.close();
    });

    await listener.wait();
    console.log("\nTest completed successfully!");
  } catch (err) {
    const error = err as Error;
    console.error("\n\x1b[31mERROR:\x1b[0m", error.message);

    if (error.message.includes("ECONNREFUSED")) {
      console.error("\n\x1b[33mThe relay server is not running or not reachable.\x1b[0m");
      console.error(`\nCheck that the relay at ${relay} is running.`);
    } else if (error.message.includes("Authentication") || error.message.includes("JWT")) {
      console.error("\n\x1b[33mAuthentication failed.\x1b[0m");
      console.error("\nGenerate a valid token with the relay's secret:");
      console.error('  cargo run -p localup-cli -- generate-token --secret "your-secret" --localup-id "test-sdk"');
    } else if (error.message.includes("QUIC") || error.message.includes("@matrixai/quic")) {
      console.error("\n\x1b[33mQUIC transport failed.\x1b[0m");
      console.error("\nOptions:");
      console.error("  1. Install QUIC: bun add @matrixai/quic");
      console.error("  2. Use WebSocket: LOCALUP_TRANSPORT=websocket");
    }

    console.error("\nFull error:", error);
    process.exit(1);
  }
}

main();
