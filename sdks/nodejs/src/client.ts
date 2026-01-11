/**
 * LocalUp Client
 *
 * Main entry point for creating tunnels.
 *
 * Usage:
 * ```typescript
 * const listener = await localup.forward({
 *   addr: 8080,
 *   authtoken: process.env.LOCALUP_AUTHTOKEN,
 *   domain: 'myapp',
 *   transport: 'quic', // or 'websocket', 'h2'
 * });
 *
 * console.log(`Tunnel established at ${listener.url()}`);
 * ```
 */

import * as http from "node:http";
import * as https from "node:https";
import * as net from "node:net";
import { EventEmitter } from "node:events";
import type { TransportConnection, TransportStream, TransportConnector } from "./transport/base.ts";
import { WebSocketConnector } from "./transport/websocket.ts";
import { H2Connector } from "./transport/h2.ts";
import { QuicConnector, isQuicAvailable, getQuicUnavailableReason } from "./transport/quic.ts";
import type { TunnelMessage, Protocol, Endpoint, TunnelConfig } from "./protocol/types.ts";
import { createDefaultTunnelConfig } from "./protocol/types.ts";
import { logger } from "./utils/logger.ts";

// ============================================================================
// Types
// ============================================================================

/**
 * Options for forwarding traffic
 */
export interface ForwardOptions {
  /**
   * Local port or address to forward to
   * Can be a number (port) or string (host:port)
   */
  addr: number | string;

  /**
   * Authentication token (JWT)
   */
  authtoken?: string;

  /**
   * Subdomain or full domain for the tunnel
   */
  domain?: string;

  /**
   * Protocol to use: 'http', 'https', 'tcp', 'tls'
   * @default 'http'
   */
  proto?: "http" | "https" | "tcp" | "tls";

  /**
   * Relay address (host:port)
   * @default Uses LOCALUP_RELAY env or 'localhost:4443'
   */
  relay?: string;

  /**
   * Skip TLS certificate verification
   * @default false
   */
  rejectUnauthorized?: boolean;

  /**
   * Transport protocol to use
   * @default 'quic'
   */
  transport?: "quic" | "websocket" | "h2";

  /**
   * IP allowlist for the tunnel
   */
  ipAllowlist?: string[];

  /**
   * Metadata for logging/debugging
   */
  metadata?: Record<string, string>;
}

/**
 * Listener interface - represents an active tunnel
 */
export interface Listener extends EventEmitter {
  /**
   * Get the public URL of the tunnel
   */
  url(): string;

  /**
   * Get all endpoints for this tunnel
   */
  endpoints(): Endpoint[];

  /**
   * Get the tunnel ID
   */
  tunnelId(): string;

  /**
   * Close the tunnel
   */
  close(): Promise<void>;

  /**
   * Wait for the tunnel to close
   */
  wait(): Promise<void>;
}

// ============================================================================
// Utilities
// ============================================================================

/**
 * Parse relay address into host and port
 */
function parseRelayAddress(relay: string): { host: string; port: number } {
  const parts = relay.split(":");
  if (parts.length === 2) {
    return {
      host: parts[0]!,
      port: parseInt(parts[1]!, 10),
    };
  }
  return {
    host: relay,
    port: 4443,
  };
}

/**
 * Generate a tunnel ID from auth token
 * Uses a simple hash of the token for consistency
 */
function generateTunnelId(token: string): string {
  // Extract the subject from JWT if possible, otherwise use a hash
  try {
    const parts = token.split(".");
    if (parts.length === 3) {
      const payload = JSON.parse(Buffer.from(parts[1]!, "base64url").toString());
      if (payload.sub) {
        return payload.sub;
      }
    }
  } catch {
    // Not a valid JWT, use hash
  }

  // Simple hash for non-JWT tokens
  let hash = 0;
  for (let i = 0; i < token.length; i++) {
    const char = token.charCodeAt(i);
    hash = (hash << 5) - hash + char;
    hash = hash & hash;
  }
  return `tunnel-${Math.abs(hash).toString(16)}`;
}

// ============================================================================
// Implementation
// ============================================================================

class LocalupListener extends EventEmitter implements Listener {
  private connection: TransportConnection;
  private controlStream: TransportStream;
  private endpoints_: Endpoint[] = [];
  private tunnelId_: string;
  private localAddr: string;
  private localPort: number;
  private localHttps: boolean;
  private closed = false;
  private closePromise: Promise<void>;
  private closeResolve!: () => void;

  constructor(
    connection: TransportConnection,
    controlStream: TransportStream,
    tunnelId: string,
    endpoints: Endpoint[],
    localAddr: string,
    localPort: number,
    localHttps: boolean
  ) {
    super();
    this.connection = connection;
    this.controlStream = controlStream;
    this.tunnelId_ = tunnelId;
    this.endpoints_ = endpoints;
    this.localAddr = localAddr;
    this.localPort = localPort;
    this.localHttps = localHttps;

    this.closePromise = new Promise((resolve) => {
      this.closeResolve = resolve;
    });

    // Start control stream reader for ping/pong
    this.controlStreamLoop();

    // Start accepting streams
    this.acceptLoop();
  }

  /**
   * Read from control stream and handle ping/pong messages
   */
  private async controlStreamLoop(): Promise<void> {
    while (!this.closed) {
      try {
        const msg = await this.controlStream.recvMessage();
        if (!msg) {
          // Control stream closed
          break;
        }

        switch (msg.type) {
          case "Ping":
            // Respond with Pong
            logger.debug(`Received ping (timestamp: ${msg.timestamp}), sending pong...`);
            await this.controlStream.sendMessage({
              type: "Pong",
              timestamp: msg.timestamp
            });
            logger.debug(`Sent pong response`);
            break;
          case "Disconnect":
            logger.info(`Relay disconnected: ${msg.reason}`);
            this.emit("disconnect", msg.reason);
            await this.close();
            return;
          default:
            // Ignore other messages on control stream
            break;
        }
      } catch (err) {
        if (!this.closed) {
          logger.error("Control stream error:", err);
        }
        break;
      }
    }

    // Control stream closed - close the tunnel
    if (!this.closed) {
      this.closed = true;
      this.emit("close");
      this.closeResolve();
    }
  }

  url(): string {
    const endpoint = this.endpoints_[0];
    return endpoint?.publicUrl ?? "";
  }

  endpoints(): Endpoint[] {
    return [...this.endpoints_];
  }

  tunnelId(): string {
    return this.tunnelId_;
  }

  async close(): Promise<void> {
    if (this.closed) return;

    this.closed = true;

    // Send disconnect
    try {
      await this.controlStream.sendMessage({
        type: "Disconnect",
        reason: "Client closing",
      });
    } catch {
      // Ignore errors when closing
    }

    await this.controlStream.close();
    await this.connection.close();

    this.emit("close");
    this.closeResolve();
  }

  async wait(): Promise<void> {
    return this.closePromise;
  }

  private async acceptLoop(): Promise<void> {
    while (!this.closed) {
      try {
        const stream = await this.connection.acceptStream();
        if (!stream) {
          // Connection closed
          break;
        }

        // Handle stream in background
        this.handleStream(stream).catch((err) => {
          // QUIC error code 0 means graceful close - this is normal when
          // the relay closes the stream after sending data
          if (this.isGracefulCloseError(err)) {
            logger.debug("Stream closed gracefully by relay");
          } else if (!this.closed) {
            logger.error("Stream handling error:", err);
          }
        });
      } catch (err) {
        if (!this.closed) {
          logger.error("Accept loop error:", err);
        }
        break;
      }
    }

    if (!this.closed) {
      this.closed = true;
      this.emit("close");
      this.closeResolve();
    }
  }

  private async handleStream(stream: TransportStream): Promise<void> {
    try {
      const msg = await stream.recvMessage();
      if (!msg) {
        await stream.close();
        return;
      }

      switch (msg.type) {
        case "HttpRequest":
          await this.handleHttpRequest(stream, msg);
          break;
        case "HttpStreamConnect":
          await this.handleHttpStream(stream, msg);
          break;
        case "TcpConnect":
          await this.handleTcpConnect(stream, msg);
          break;
        case "Ping":
          await stream.sendMessage({ type: "Pong", timestamp: msg.timestamp });
          break;
        case "Disconnect":
          this.emit("disconnect", msg.reason);
          await this.close();
          break;
        default:
          logger.warn(`Unhandled message type: ${msg.type}`);
      }
    } catch (err) {
      logger.error("Stream handling error:", err);
    } finally {
      await stream.close();
    }
  }

  private async handleHttpRequest(
    stream: TransportStream,
    msg: Extract<TunnelMessage, { type: "HttpRequest" }>
  ): Promise<void> {
    const { method, uri, headers, body, streamId } = msg;

    // Parse the URI to get path
    const url = new URL(uri, `http://${this.localAddr}:${this.localPort}`);

    // Forward request to local server
    const protocol = this.localHttps ? https : http;
    const options: http.RequestOptions & https.RequestOptions = {
      hostname: this.localAddr,
      port: this.localPort,
      path: url.pathname + url.search,
      method,
      headers: Object.fromEntries(headers),
    };

    // For HTTPS, disable certificate verification for local servers
    if (this.localHttps) {
      (options as https.RequestOptions).rejectUnauthorized = false;
    }

    return new Promise((resolve, reject) => {
      const req = protocol.request(options, async (res) => {
        try {
          // Collect response body
          const chunks: Buffer[] = [];
          for await (const chunk of res) {
            chunks.push(chunk as Buffer);
          }
          const responseBody = Buffer.concat(chunks);

          // Send response back
          await stream.sendMessage({
            type: "HttpResponse",
            streamId,
            status: res.statusCode ?? 200,
            headers: Object.entries(res.headers)
              .filter(([, v]) => v !== undefined)
              .map(([k, v]) => [k, Array.isArray(v) ? v.join(", ") : String(v)] as [string, string]),
            body: responseBody.length > 0 ? new Uint8Array(responseBody) : null,
          });

          this.emit("request", {
            method,
            path: url.pathname,
            status: res.statusCode,
          });

          resolve();
        } catch (err) {
          reject(err);
        }
      });

      req.on("error", async (err) => {
        // Send error response
        try {
          await stream.sendMessage({
            type: "HttpResponse",
            streamId,
            status: 502,
            headers: [["content-type", "text/plain"]],
            body: new TextEncoder().encode(`Bad Gateway: ${err.message}`),
          });
        } catch {
          // Ignore
        }
        reject(err);
      });

      // Send request body
      if (body) {
        req.write(Buffer.from(body));
      }
      req.end();
    });
  }

  private async handleHttpStream(
    stream: TransportStream,
    msg: Extract<TunnelMessage, { type: "HttpStreamConnect" }>
  ): Promise<void> {
    const { streamId, initialData } = msg;

    // Create TCP connection to local server
    const socket = await this.createLocalConnection();

    // Write initial data
    socket.write(Buffer.from(initialData));

    // Bidirectional proxy
    const proxyToRemote = async () => {
      try {
        for await (const chunk of socket) {
          await stream.sendMessage({
            type: "HttpStreamData",
            streamId,
            data: new Uint8Array(chunk as Buffer),
          });
        }
      } catch {
        // Socket closed
      }
    };

    const proxyToLocal = async () => {
      try {
        while (true) {
          const msg = await stream.recvMessage();
          if (!msg) break;

          if (msg.type === "HttpStreamData") {
            socket.write(Buffer.from(msg.data));
          } else if (msg.type === "HttpStreamClose") {
            break;
          }
        }
      } catch {
        // Stream closed
      }
    };

    await Promise.all([proxyToRemote(), proxyToLocal()]);

    socket.destroy();
    await stream.sendMessage({ type: "HttpStreamClose", streamId });
  }

  private async handleTcpConnect(
    stream: TransportStream,
    msg: Extract<TunnelMessage, { type: "TcpConnect" }>
  ): Promise<void> {
    const { streamId } = msg;

    // Create TCP connection to local server
    const socket = await this.createLocalConnection();

    // Bidirectional proxy
    const proxyToRemote = async () => {
      try {
        for await (const chunk of socket) {
          await stream.sendMessage({
            type: "TcpData",
            streamId,
            data: new Uint8Array(chunk as Buffer),
          });
        }
      } catch {
        // Socket closed
      }
    };

    const proxyToLocal = async () => {
      try {
        while (true) {
          const msg = await stream.recvMessage();
          if (!msg) break;

          if (msg.type === "TcpData") {
            socket.write(Buffer.from(msg.data));
          } else if (msg.type === "TcpClose") {
            break;
          }
        }
      } catch {
        // Stream closed
      }
    };

    await Promise.all([proxyToRemote(), proxyToLocal()]);

    socket.destroy();
    await stream.sendMessage({ type: "TcpClose", streamId });
  }

  private createLocalConnection(): Promise<net.Socket> {
    return new Promise((resolve, reject) => {
      const socket = net.createConnection(
        {
          host: this.localAddr,
          port: this.localPort,
        },
        () => {
          resolve(socket);
        }
      );

      socket.on("error", reject);
    });
  }

  /**
   * Check if error is a graceful close (QUIC error code 0)
   * This happens when relay closes the stream after sending - not a real error
   */
  private isGracefulCloseError(err: unknown): boolean {
    if (err instanceof Error) {
      const msg = err.message;
      // @matrixai/quic throws "Error: write 0" for graceful close (code 0)
      // Also check for "read 0" in case read side closes first
      if (msg === "write 0" || msg === "read 0" || msg.includes("code 0")) {
        return true;
      }
      // Stream already closed is also graceful
      if (msg.includes("Stream closed") || msg.includes("stream closed")) {
        return true;
      }
    }
    return false;
  }
}

// ============================================================================
// Public API
// ============================================================================

/**
 * Create a tunnel and forward traffic to a local address
 *
 * @example
 * ```typescript
 * const listener = await localup.forward({
 *   addr: 8080,
 *   authtoken: process.env.LOCALUP_AUTHTOKEN,
 *   domain: 'myapp',
 *   transport: 'quic', // Required: specify transport protocol
 * });
 *
 * console.log(`Tunnel at: ${listener.url()}`);
 * ```
 */
export async function forward(options: ForwardOptions): Promise<Listener> {
  // Parse local address
  let localHost = "localhost";
  let localPort: number;

  if (typeof options.addr === "number") {
    localPort = options.addr;
  } else {
    const parts = options.addr.split(":");
    if (parts.length === 2) {
      localHost = parts[0]!;
      localPort = parseInt(parts[1]!, 10);
    } else {
      localPort = parseInt(options.addr, 10);
    }
  }

  // Get auth token
  const authToken = options.authtoken ?? process.env.LOCALUP_AUTHTOKEN ?? "";
  if (!authToken) {
    throw new Error("Authentication token is required. Set authtoken option or LOCALUP_AUTHTOKEN env.");
  }

  // Get relay address
  const relay = options.relay ?? process.env.LOCALUP_RELAY ?? "localhost:4443";
  const { host: relayHost, port: relayPort } = parseRelayAddress(relay);

  // Generate tunnel ID from token
  const tunnelId = generateTunnelId(authToken);

  // Get transport (default to quic)
  const transportType = options.transport ?? "quic";

  logger.info(`Using ${transportType} transport to connect to ${relayHost}:${relayPort}...`);

  // Create connector based on transport type
  let connector: TransportConnector;
  switch (transportType) {
    case "websocket":
      connector = new WebSocketConnector({
        path: "/localup",
        useTls: true,
      });
      break;
    case "h2":
      connector = new H2Connector({
        useTls: true,
        rejectUnauthorized: options.rejectUnauthorized,
      });
      break;
    case "quic":
      // Check if QUIC is available (requires Node.js 23+ with --experimental-quic)
      if (await isQuicAvailable()) {
        connector = new QuicConnector({
          verifyPeer: options.rejectUnauthorized !== false,
        });
      } else {
        throw new Error(
          `QUIC transport requested but not available. ${getQuicUnavailableReason()}`
        );
      }
      break;
    default:
      throw new Error(`Unsupported transport: ${transportType}`);
  }

  // Connect to relay
  logger.debug(`Connecting to ${relayHost}:${relayPort}...`);
  const connection = await connector.connect(relayHost, relayPort, relayHost);

  // Open control stream
  const controlStream = await connection.openStream();

  // Build protocol config
  const proto = options.proto ?? "http";
  let protocol: Protocol;
  switch (proto) {
    case "http":
      protocol = { type: "Http", subdomain: options.domain ?? null };
      break;
    case "https":
      protocol = { type: "Https", subdomain: options.domain ?? null };
      break;
    case "tcp":
      protocol = { type: "Tcp", port: 0 }; // 0 = server allocates
      break;
    case "tls":
      protocol = { type: "Tls", port: 0, sniPattern: options.domain ?? "*" };
      break;
  }

  // Build tunnel config
  const config: TunnelConfig = createDefaultTunnelConfig({
    localHost,
    localPort,
    localHttps: proto === "https",
    exitNode: { type: "Custom", address: relay },
    ipAllowlist: options.ipAllowlist ?? [],
  });

  // Send Connect message
  await controlStream.sendMessage({
    type: "Connect",
    localupId: tunnelId,
    authToken,
    protocols: [protocol],
    config,
  });

  // Wait for Connected response
  const response = await controlStream.recvMessage();
  if (!response) {
    await connection.close();
    throw new Error("Connection closed before receiving response");
  }

  if (response.type === "Disconnect") {
    await connection.close();
    throw new Error(`Connection rejected: ${response.reason}`);
  }

  if (response.type !== "Connected") {
    await connection.close();
    throw new Error(`Unexpected response: ${response.type}`);
  }

  logger.info(`Tunnel established: ${response.endpoints[0]?.publicUrl}`);

  return new LocalupListener(
    connection,
    controlStream,
    response.localupId,
    response.endpoints,
    localHost,
    localPort,
    proto === "https"
  );
}

/**
 * Default export for convenience
 */
export default { forward };
