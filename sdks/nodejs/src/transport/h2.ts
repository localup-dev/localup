/**
 * HTTP/2 Transport Implementation
 *
 * Uses Node.js built-in http2 module for transport.
 * Each HTTP/2 stream maps to a LocalUp stream.
 */

import * as http2 from "node:http2";
import * as tls from "node:tls";
import type { TunnelMessage } from "../protocol/types.ts";
import { encodeMessage, FrameAccumulator } from "../protocol/codec.ts";
import type {
  TransportStream,
  TransportConnection,
  TransportConnector,
  ConnectionStats,
} from "./base.ts";
import { TransportError, TransportErrorCode } from "./base.ts";

/**
 * HTTP/2 stream implementation
 */
class H2Stream implements TransportStream {
  readonly streamId: number;
  private stream: http2.ClientHttp2Stream;
  private closed = false;
  private accumulator = new FrameAccumulator();
  private messageWaiters: Array<(msg: TunnelMessage | null) => void> = [];
  private bytesWaiters: Array<(data: Uint8Array | null) => void> = [];
  private messageQueue: TunnelMessage[] = [];
  private bytesQueue: Uint8Array[] = [];

  constructor(streamId: number, stream: http2.ClientHttp2Stream) {
    this.streamId = streamId;
    this.stream = stream;
    this.setupHandlers();
  }

  private setupHandlers(): void {
    this.stream.on("data", (chunk: Buffer) => {
      this.accumulator.push(new Uint8Array(chunk));

      // Try to extract complete messages
      const messages = this.accumulator.readAllMessages();
      for (const msg of messages) {
        const waiter = this.messageWaiters.shift();
        if (waiter) {
          waiter(msg);
        } else {
          this.messageQueue.push(msg);
        }
      }
    });

    this.stream.on("end", () => {
      this.closed = true;
      this.notifyClose();
    });

    this.stream.on("error", (err: Error) => {
      console.error("H2 stream error:", err);
      this.closed = true;
      this.notifyClose();
    });

    this.stream.on("close", () => {
      this.closed = true;
      this.notifyClose();
    });
  }

  private notifyClose(): void {
    for (const waiter of this.messageWaiters) {
      waiter(null);
    }
    for (const waiter of this.bytesWaiters) {
      waiter(null);
    }
    this.messageWaiters = [];
    this.bytesWaiters = [];
  }

  async sendMessage(message: TunnelMessage): Promise<void> {
    if (this.closed) {
      throw new TransportError("Stream closed", TransportErrorCode.StreamClosed);
    }

    const data = encodeMessage(message);
    return new Promise((resolve, reject) => {
      this.stream.write(Buffer.from(data), (err) => {
        if (err) {
          reject(new TransportError(err.message, TransportErrorCode.IoError));
        } else {
          resolve();
        }
      });
    });
  }

  async recvMessage(): Promise<TunnelMessage | null> {
    if (this.closed && this.messageQueue.length === 0) {
      return null;
    }

    const queued = this.messageQueue.shift();
    if (queued) {
      return queued;
    }

    return new Promise((resolve) => {
      this.messageWaiters.push(resolve);
    });
  }

  async sendBytes(data: Uint8Array): Promise<void> {
    if (this.closed) {
      throw new TransportError("Stream closed", TransportErrorCode.StreamClosed);
    }

    return new Promise((resolve, reject) => {
      this.stream.write(Buffer.from(data), (err) => {
        if (err) {
          reject(new TransportError(err.message, TransportErrorCode.IoError));
        } else {
          resolve();
        }
      });
    });
  }

  async recvBytes(_maxSize: number): Promise<Uint8Array | null> {
    if (this.closed && this.bytesQueue.length === 0) {
      return null;
    }

    const queued = this.bytesQueue.shift();
    if (queued) {
      return queued;
    }

    return new Promise((resolve) => {
      this.bytesWaiters.push(resolve);
    });
  }

  async close(): Promise<void> {
    if (this.closed) return;

    this.closed = true;
    this.stream.end();
    this.stream.close();
    this.notifyClose();
  }

  isClosed(): boolean {
    return this.closed;
  }
}

/**
 * HTTP/2 connection implementation
 */
class H2Connection implements TransportConnection {
  private session: http2.ClientHttp2Session;
  private streams: Map<number, H2Stream> = new Map();
  private nextStreamId = 1;
  private closed = false;
  private pendingAcceptResolvers: Array<(stream: TransportStream | null) => void> = [];
  private incomingStreams: H2Stream[] = [];
  private stats_: ConnectionStats = {
    bytesSent: 0n,
    bytesReceived: 0n,
    streamCount: 0,
  };
  private remoteAddr: string;
  private authority: string;

  constructor(session: http2.ClientHttp2Session, remoteAddr: string, authority: string) {
    this.session = session;
    this.remoteAddr = remoteAddr;
    this.authority = authority;
    this.setupHandlers();
  }

  private setupHandlers(): void {
    this.session.on("close", () => {
      this.closed = true;
      for (const resolver of this.pendingAcceptResolvers) {
        resolver(null);
      }
    });

    this.session.on("error", (err) => {
      console.error("H2 session error:", err);
    });

    // Handle server-initiated streams (push promises)
    this.session.on("stream", (stream, headers) => {
      const streamId = Number(headers[":path"]?.replace("/stream/", "") || this.nextStreamId++);
      const h2Stream = new H2Stream(streamId, stream as unknown as http2.ClientHttp2Stream);
      this.streams.set(streamId, h2Stream);
      this.stats_.streamCount++;

      const resolver = this.pendingAcceptResolvers.shift();
      if (resolver) {
        resolver(h2Stream);
      } else {
        this.incomingStreams.push(h2Stream);
      }
    });
  }

  async openStream(): Promise<TransportStream> {
    if (this.closed) {
      throw new TransportError("Connection closed", TransportErrorCode.ConnectionClosed);
    }

    const streamId = this.nextStreamId++;

    // Create HTTP/2 stream with POST request
    const stream = this.session.request({
      ":method": "POST",
      ":path": `/stream/${streamId}`,
      ":authority": this.authority,
      "content-type": "application/octet-stream",
    });

    const h2Stream = new H2Stream(streamId, stream);
    this.streams.set(streamId, h2Stream);
    this.stats_.streamCount++;

    return h2Stream;
  }

  async acceptStream(): Promise<TransportStream | null> {
    if (this.closed && this.incomingStreams.length === 0) {
      return null;
    }

    const queued = this.incomingStreams.shift();
    if (queued) {
      return queued;
    }

    return new Promise((resolve) => {
      this.pendingAcceptResolvers.push(resolve);
    });
  }

  async close(_errorCode?: number, _reason?: string): Promise<void> {
    if (this.closed) return;

    this.closed = true;

    // Close all streams
    for (const stream of this.streams.values()) {
      await stream.close();
    }

    this.session.close();
  }

  isClosed(): boolean {
    return this.closed;
  }

  remoteAddress(): string {
    return this.remoteAddr;
  }

  stats(): ConnectionStats {
    return { ...this.stats_ };
  }
}

/**
 * HTTP/2 transport connector
 */
export class H2Connector implements TransportConnector {
  readonly protocol = "h2";
  private useTls: boolean;
  private alpnProtocol: string;
  private rejectUnauthorized: boolean;

  constructor(
    options: { useTls?: boolean; alpnProtocol?: string; rejectUnauthorized?: boolean } = {}
  ) {
    this.useTls = options.useTls ?? true;
    this.alpnProtocol = options.alpnProtocol ?? "localup-v1";
    this.rejectUnauthorized = options.rejectUnauthorized ?? false;
  }

  async connect(host: string, port: number, serverName?: string): Promise<TransportConnection> {
    return new Promise((resolve, reject) => {
      const authority = `${host}:${port}`;
      const url = this.useTls ? `https://${authority}` : `http://${authority}`;

      const options: http2.SecureClientSessionOptions = {
        rejectUnauthorized: this.rejectUnauthorized,
      };

      if (this.useTls) {
        options.ALPNProtocols = [this.alpnProtocol, "h2"];
        if (serverName) {
          options.servername = serverName;
        }
      }

      const session = http2.connect(url, options as tls.ConnectionOptions);

      const timeout = setTimeout(() => {
        session.close();
        reject(new TransportError("Connection timeout", TransportErrorCode.Timeout));
      }, 30000);

      session.on("connect", () => {
        clearTimeout(timeout);
        resolve(new H2Connection(session, authority, authority));
      });

      session.on("error", (err) => {
        clearTimeout(timeout);
        reject(new TransportError(`Connection failed: ${err.message}`, TransportErrorCode.ConnectionFailed));
      });
    });
  }
}
