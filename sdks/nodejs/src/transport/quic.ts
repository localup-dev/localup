/**
 * QUIC Transport Implementation
 *
 * Uses @matrixai/quic which is built on Cloudflare's quiche library.
 *
 * QUIC is the preferred transport for LocalUp due to:
 * - UDP-based (better performance, no head-of-line blocking)
 * - Native multiplexing (built into the protocol)
 * - 0-RTT connection establishment
 * - Built-in TLS 1.3
 *
 * INSTALLATION:
 *   npm install @matrixai/quic
 *   # or
 *   bun add @matrixai/quic
 *
 * Note: @matrixai/quic requires native compilation and may not work on all platforms.
 * If QUIC is not available, use WebSocket or HTTP/2 transport instead.
 */

import type { TunnelMessage } from "../protocol/types.ts";
import { encodeMessage, FrameAccumulator } from "../protocol/codec.ts";
import type {
  TransportStream,
  TransportConnection,
  TransportConnector,
  ConnectionStats,
} from "./base.ts";
import { TransportError, TransportErrorCode } from "./base.ts";
import * as crypto from "node:crypto";

// We use dynamic imports and 'any' types to make @matrixai/quic optional
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let QUICClient: any = null;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let EventQUICConnectionStream: any = null;
let quicLoadAttempted = false;
let quicLoadError: Error | null = null;

async function loadQuicModule(): Promise<boolean> {
  if (quicLoadAttempted) {
    return QUICClient !== null;
  }
  quicLoadAttempted = true;

  try {
    // Dynamic import of @matrixai/quic package
    const quicModule = await import("@matrixai/quic");
    QUICClient = quicModule.QUICClient;
    EventQUICConnectionStream = quicModule.events?.EventQUICConnectionStream;
    return true;
  } catch (err) {
    console.error("Error loading QUIC module:", err);
    quicLoadError = err as Error;
    return false;
  }
}

/**
 * Check if QUIC is available
 */
export async function isQuicAvailable(): Promise<boolean> {
  return loadQuicModule();
}

/**
 * Get the error message explaining why QUIC is not available
 */
export function getQuicUnavailableReason(): string {
  if (quicLoadError) {
    const msg = quicLoadError.message || String(quicLoadError);
    if (msg.includes("Cannot find module") || msg.includes("MODULE_NOT_FOUND")) {
      return "QUIC requires @matrixai/quic package. Install with: npm install @matrixai/quic";
    }
    return `QUIC module failed to load: ${msg}`;
  }
  return "QUIC is not available in this runtime";
}

/**
 * QUIC stream implementation wrapping @matrixai/quic QUICStream
 *
 * @matrixai/quic uses Web Streams API:
 * - stream.readable: ReadableStream<Uint8Array>
 * - stream.writable: WritableStream<Uint8Array>
 */
class QuicStreamImpl implements TransportStream {
  readonly streamId: number;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private stream: any;
  // Track read and write closed separately for bidirectional streams
  private readClosed = false;
  private writeClosed = false;
  private accumulator = new FrameAccumulator();
  private messageQueue: TunnelMessage[] = [];
  private messageWaiters: Array<(msg: TunnelMessage | null) => void> = [];
  private readerStarted = false;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private reader: any = null;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private writer: any = null;

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  constructor(streamId: number, stream: any) {
    this.streamId = streamId;
    this.stream = stream;
  }

  private startReader(): void {
    if (this.readerStarted) return;
    this.readerStarted = true;

    // Get a reader from the Web Streams API ReadableStream
    this.reader = this.stream.readable.getReader();

    const readLoop = async () => {
      try {
        while (!this.readClosed) {
          const { value, done } = await this.reader.read();
          if (done) break;

          const data = value instanceof Uint8Array ? value : new Uint8Array(value);
          this.accumulator.push(data);

          const messages = this.accumulator.readAllMessages();
          for (const msg of messages) {
            const waiter = this.messageWaiters.shift();
            if (waiter) {
              waiter(msg);
            } else {
              this.messageQueue.push(msg);
            }
          }
        }
      } catch {
        // Stream closed or error
      } finally {
        this.handleReadClose();
      }
    };

    readLoop();
  }

  private handleReadClose(): void {
    this.readClosed = true;
    // Notify waiters that no more messages will arrive
    for (const waiter of this.messageWaiters) {
      waiter(null);
    }
    this.messageWaiters = [];
  }

  async sendMessage(message: TunnelMessage): Promise<void> {
    if (this.writeClosed) {
      throw new TransportError("Stream closed", TransportErrorCode.StreamClosed);
    }

    const data = encodeMessage(message);
    await this.sendBytes(data);
  }

  async recvMessage(): Promise<TunnelMessage | null> {
    this.startReader();

    if (this.readClosed && this.messageQueue.length === 0) {
      return null;
    }

    const queued = this.messageQueue.shift();
    if (queued) {
      return queued;
    }

    if (this.readClosed) {
      return null;
    }

    return new Promise((resolve) => {
      this.messageWaiters.push(resolve);
    });
  }

  async sendBytes(data: Uint8Array): Promise<void> {
    if (this.writeClosed) {
      throw new TransportError("Stream closed", TransportErrorCode.StreamClosed);
    }

    // Get writer if not already obtained
    if (!this.writer) {
      this.writer = this.stream.writable.getWriter();
    }

    // Write using Web Streams API WritableStreamDefaultWriter
    await this.writer.write(data);
  }

  async recvBytes(maxSize: number): Promise<Uint8Array | null> {
    this.startReader();

    if (this.readClosed) {
      return null;
    }

    const bytes = this.accumulator.readRawBytes(maxSize);
    return bytes;
  }

  async close(): Promise<void> {
    if (this.readClosed && this.writeClosed) return;
    this.readClosed = true;
    this.writeClosed = true;

    try {
      // Release the writer if obtained
      if (this.writer) {
        await this.writer.close();
      }
      // Cancel the reader if obtained
      if (this.reader) {
        await this.reader.cancel();
      }
      // Destroy the stream
      await this.stream.destroy();
    } catch {
      // Ignore close errors
    }
  }

  isClosed(): boolean {
    return this.readClosed && this.writeClosed;
  }
}

/**
 * QUIC connection implementation wrapping @matrixai/quic QUICClient
 */
class QuicConnectionImpl implements TransportConnection {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private client: any;
  private streams: Map<number, QuicStreamImpl> = new Map();
  private nextStreamId = 0;
  private closed = false;
  private incomingStreams: QuicStreamImpl[] = [];
  private streamWaiters: Array<(stream: TransportStream | null) => void> = [];
  private remoteAddr: string;
  private eventListenerSetup = false;

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  constructor(client: any, remoteAddr: string) {
    this.client = client;
    this.remoteAddr = remoteAddr;
  }

  private setupEventListeners(): void {
    if (this.eventListenerSetup) return;
    this.eventListenerSetup = true;

    // Listen for incoming streams via event
    if (EventQUICConnectionStream) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      this.client.connection.addEventListener(EventQUICConnectionStream.name, (event: any) => {
        if (this.closed) return;

        const stream = event.detail;
        const streamId = this.nextStreamId++;
        const quicStream = new QuicStreamImpl(streamId, stream);
        this.streams.set(streamId, quicStream);

        const waiter = this.streamWaiters.shift();
        if (waiter) {
          waiter(quicStream);
        } else {
          this.incomingStreams.push(quicStream);
        }
      });
    }

    // Listen for connection close
    this.client.connection.addEventListener("close", () => {
      this.handleClose();
    });
  }

  private handleClose(): void {
    this.closed = true;
    for (const waiter of this.streamWaiters) {
      waiter(null);
    }
    this.streamWaiters = [];
  }

  async openStream(): Promise<TransportStream> {
    if (this.closed) {
      throw new TransportError("Connection closed", TransportErrorCode.ConnectionClosed);
    }

    // Open a new bidirectional stream using newStream()
    const stream = this.client.connection.newStream("bidi");
    const streamId = this.nextStreamId++;
    const quicStream = new QuicStreamImpl(streamId, stream);
    this.streams.set(streamId, quicStream);

    return quicStream;
  }

  async acceptStream(): Promise<TransportStream | null> {
    this.setupEventListeners();

    if (this.closed) {
      return null;
    }

    const queued = this.incomingStreams.shift();
    if (queued) {
      return queued;
    }

    if (this.closed) {
      return null;
    }

    return new Promise((resolve) => {
      this.streamWaiters.push(resolve);
    });
  }

  async close(_errorCode?: number, _reason?: string): Promise<void> {
    if (this.closed) return;
    this.closed = true;

    for (const stream of this.streams.values()) {
      await stream.close();
    }

    try {
      await this.client.destroy();
    } catch {
      // Ignore close errors
    }

    this.handleClose();
  }

  isClosed(): boolean {
    return this.closed;
  }

  remoteAddress(): string {
    return this.remoteAddr;
  }

  stats(): ConnectionStats {
    return {
      bytesSent: 0n,
      bytesReceived: 0n,
      streamCount: this.streams.size,
    };
  }
}

/**
 * Crypto utilities for @matrixai/quic client
 */
const quicCrypto = {
  ops: {
    async randomBytes(data: ArrayBuffer): Promise<void> {
      const buffer = Buffer.from(data);
      crypto.randomFillSync(buffer);
    },
  },
};

/**
 * QUIC transport connector using @matrixai/quic
 */
export class QuicConnector implements TransportConnector {
  readonly protocol = "quic";
  private verifyPeer: boolean;
  private caCert?: string | Buffer;

  constructor(options: { verifyPeer?: boolean; caCert?: string | Buffer } = {}) {
    this.verifyPeer = options.verifyPeer ?? false;
    this.caCert = options.caCert;
  }

  async connect(host: string, port: number, serverName?: string): Promise<TransportConnection> {
    const available = await loadQuicModule();
    if (!available || !QUICClient) {
      throw new TransportError(
        getQuicUnavailableReason(),
        TransportErrorCode.ConnectionFailed
      );
    }

    try {
      // Create QUIC client with @matrixai/quic
      // The crypto parameter is required for generating random bytes
      const client = await QUICClient.createQUICClient({
        host,
        port,
        serverName: serverName ?? host,
        crypto: quicCrypto,
        config: {
          verifyPeer: this.verifyPeer,
          ca: this.caCert ? [this.caCert] : undefined,
          applicationProtos: ["localup-v1"],
          // Keep-alive settings to prevent idle timeout
          // maxIdleTimeout of 0 means infinite (no timeout)
          maxIdleTimeout: 0,
          // Send keep-alive frames every 15 seconds
          keepAliveIntervalTime: 15000,
        },
      });

      return new QuicConnectionImpl(client, `${host}:${port}`);
    } catch (err) {
      const errMsg = (err as Error).message || String(err);
      throw new TransportError(
        `QUIC connection to ${host}:${port} failed: ${errMsg}`,
        TransportErrorCode.ConnectionFailed,
        err as Error
      );
    }
  }
}
