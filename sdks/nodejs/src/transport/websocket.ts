/**
 * WebSocket Transport Implementation
 *
 * Uses native WebSocket (available in Node.js 21+, Bun, and browsers)
 * with manual stream multiplexing.
 *
 * Frame format for multiplexing:
 * [4-byte stream_id][1-byte frame_type][1-byte flags][4-byte length][payload]
 *
 * Frame types:
 * 0 = Control (Connect, Ping, etc.)
 * 1 = Data
 * 2 = Close
 * 3 = WindowUpdate
 */

import type { TunnelMessage } from "../protocol/types.ts";
import { encodeMessagePayload, decodeMessagePayload } from "../protocol/codec.ts";
import type {
  TransportStream,
  TransportConnection,
  TransportConnector,
  ConnectionStats,
} from "./base.ts";
import { TransportError, TransportErrorCode } from "./base.ts";

const HEADER_SIZE = 10; // stream_id(4) + type(1) + flags(1) + length(4)

enum FrameType {
  Control = 0,
  Data = 1,
  Close = 2,
  WindowUpdate = 3,
}

enum FrameFlags {
  None = 0,
  Fin = 1,
  Ack = 2,
  Rst = 4,
}

interface MuxFrame {
  streamId: number;
  frameType: FrameType;
  flags: number;
  payload: Uint8Array;
}

function encodeFrame(frame: MuxFrame): Uint8Array {
  const result = new Uint8Array(HEADER_SIZE + frame.payload.length);
  const view = new DataView(result.buffer);

  view.setUint32(0, frame.streamId, false); // big-endian
  result[4] = frame.frameType;
  result[5] = frame.flags;
  view.setUint32(6, frame.payload.length, false); // big-endian
  result.set(frame.payload, HEADER_SIZE);

  return result;
}

function decodeFrame(data: Uint8Array): MuxFrame {
  if (data.length < HEADER_SIZE) {
    throw new TransportError("Frame too small", TransportErrorCode.ProtocolError);
  }

  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  const streamId = view.getUint32(0, false);
  const frameType = data[4]!;
  const flags = data[5]!;
  const length = view.getUint32(6, false);

  if (data.length < HEADER_SIZE + length) {
    throw new TransportError("Incomplete frame", TransportErrorCode.ProtocolError);
  }

  const payload = data.slice(HEADER_SIZE, HEADER_SIZE + length);

  return { streamId, frameType, flags, payload };
}

/**
 * WebSocket stream implementation
 */
class WebSocketStream implements TransportStream {
  readonly streamId: number;
  private connection: WebSocketConnection;
  private closed = false;
  private messageQueue: TunnelMessage[] = [];
  private bytesQueue: Uint8Array[] = [];
  private messageWaiters: Array<(msg: TunnelMessage | null) => void> = [];
  private bytesWaiters: Array<(data: Uint8Array | null) => void> = [];

  constructor(streamId: number, connection: WebSocketConnection) {
    this.streamId = streamId;
    this.connection = connection;
  }

  async sendMessage(message: TunnelMessage): Promise<void> {
    if (this.closed) {
      throw new TransportError("Stream closed", TransportErrorCode.StreamClosed);
    }

    const payload = encodeMessagePayload(message);
    await this.connection.sendFrame({
      streamId: this.streamId,
      frameType: FrameType.Data,
      flags: FrameFlags.None,
      payload,
    });
  }

  async recvMessage(): Promise<TunnelMessage | null> {
    if (this.closed && this.messageQueue.length === 0) {
      return null;
    }

    // Check queue first
    const queued = this.messageQueue.shift();
    if (queued) {
      return queued;
    }

    // Wait for new message
    return new Promise((resolve) => {
      this.messageWaiters.push(resolve);
    });
  }

  async sendBytes(data: Uint8Array): Promise<void> {
    if (this.closed) {
      throw new TransportError("Stream closed", TransportErrorCode.StreamClosed);
    }

    await this.connection.sendFrame({
      streamId: this.streamId,
      frameType: FrameType.Data,
      flags: FrameFlags.None,
      payload: data,
    });
  }

  async recvBytes(_maxSize: number): Promise<Uint8Array | null> {
    if (this.closed && this.bytesQueue.length === 0) {
      return null;
    }

    // Check queue first
    const queued = this.bytesQueue.shift();
    if (queued) {
      return queued;
    }

    // Wait for new data
    return new Promise((resolve) => {
      this.bytesWaiters.push(resolve);
    });
  }

  async close(): Promise<void> {
    if (this.closed) return;

    this.closed = true;

    // Send close frame
    try {
      await this.connection.sendFrame({
        streamId: this.streamId,
        frameType: FrameType.Close,
        flags: FrameFlags.Fin,
        payload: new Uint8Array(0),
      });
    } catch {
      // Ignore errors when closing
    }

    // Notify waiters
    for (const waiter of this.messageWaiters) {
      waiter(null);
    }
    for (const waiter of this.bytesWaiters) {
      waiter(null);
    }
    this.messageWaiters = [];
    this.bytesWaiters = [];
  }

  isClosed(): boolean {
    return this.closed;
  }

  // Internal: called by connection when frame is received
  _onFrame(frame: MuxFrame): void {
    if (frame.frameType === FrameType.Close) {
      this.closed = true;
      for (const waiter of this.messageWaiters) {
        waiter(null);
      }
      for (const waiter of this.bytesWaiters) {
        waiter(null);
      }
      return;
    }

    if (frame.frameType === FrameType.Data) {
      // Try to decode as TunnelMessage
      try {
        const msg = decodeMessagePayload(frame.payload);

        const waiter = this.messageWaiters.shift();
        if (waiter) {
          waiter(msg);
        } else {
          this.messageQueue.push(msg);
        }
      } catch {
        // Not a TunnelMessage, treat as raw bytes
        const waiter = this.bytesWaiters.shift();
        if (waiter) {
          waiter(frame.payload);
        } else {
          this.bytesQueue.push(frame.payload);
        }
      }
    }
  }
}

/**
 * WebSocket connection implementation
 */
class WebSocketConnection implements TransportConnection {
  private ws: WebSocket;
  private streams: Map<number, WebSocketStream> = new Map();
  private nextStreamId = 1;
  private closed = false;
  private pendingAcceptResolvers: Array<(stream: TransportStream | null) => void> = [];
  private incomingStreams: WebSocketStream[] = [];
  private stats_: ConnectionStats = {
    bytesSent: 0n,
    bytesReceived: 0n,
    streamCount: 0,
  };
  private remoteAddr: string;

  constructor(ws: WebSocket, remoteAddr: string) {
    this.ws = ws;
    this.remoteAddr = remoteAddr;
    this.setupHandlers();
  }

  private setupHandlers(): void {
    this.ws.binaryType = "arraybuffer";

    this.ws.onmessage = (event: MessageEvent) => {
      const data = new Uint8Array(event.data as ArrayBuffer);
      this.stats_.bytesReceived += BigInt(data.length);

      try {
        const frame = decodeFrame(data);
        this.handleFrame(frame);
      } catch (e) {
        console.error("Failed to decode frame:", e);
      }
    };

    this.ws.onclose = () => {
      this.closed = true;
      // Close all streams
      for (const stream of this.streams.values()) {
        stream._onFrame({
          streamId: stream.streamId,
          frameType: FrameType.Close,
          flags: FrameFlags.Rst,
          payload: new Uint8Array(0),
        });
      }
      // Notify accept waiters
      for (const resolver of this.pendingAcceptResolvers) {
        resolver(null);
      }
    };

    this.ws.onerror = (event: Event) => {
      console.error("WebSocket error:", event);
    };
  }

  private handleFrame(frame: MuxFrame): void {
    let stream = this.streams.get(frame.streamId);

    // New incoming stream?
    if (!stream && frame.frameType === FrameType.Data) {
      stream = new WebSocketStream(frame.streamId, this);
      this.streams.set(frame.streamId, stream);
      this.stats_.streamCount++;

      // Notify accepters
      const resolver = this.pendingAcceptResolvers.shift();
      if (resolver) {
        resolver(stream);
      } else {
        this.incomingStreams.push(stream);
      }
    }

    if (stream) {
      stream._onFrame(frame);
    }
  }

  async sendFrame(frame: MuxFrame): Promise<void> {
    if (this.closed) {
      throw new TransportError("Connection closed", TransportErrorCode.ConnectionClosed);
    }

    const data = encodeFrame(frame);
    this.stats_.bytesSent += BigInt(data.length);
    this.ws.send(data);
  }

  async openStream(): Promise<TransportStream> {
    if (this.closed) {
      throw new TransportError("Connection closed", TransportErrorCode.ConnectionClosed);
    }

    const streamId = this.nextStreamId++;
    const stream = new WebSocketStream(streamId, this);
    this.streams.set(streamId, stream);
    this.stats_.streamCount++;

    return stream;
  }

  async acceptStream(): Promise<TransportStream | null> {
    if (this.closed && this.incomingStreams.length === 0) {
      return null;
    }

    // Check queue first
    const queued = this.incomingStreams.shift();
    if (queued) {
      return queued;
    }

    // Wait for new stream
    return new Promise((resolve) => {
      this.pendingAcceptResolvers.push(resolve);
    });
  }

  async close(_errorCode?: number, _reason?: string): Promise<void> {
    if (this.closed) return;

    this.closed = true;
    this.ws.close();
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
 * WebSocket transport connector
 */
export class WebSocketConnector implements TransportConnector {
  readonly protocol = "websocket";
  private path: string;
  private useTls: boolean;

  constructor(options: { path?: string; useTls?: boolean } = {}) {
    this.path = options.path ?? "/localup";
    this.useTls = options.useTls ?? true;
  }

  async connect(host: string, port: number, _serverName?: string): Promise<TransportConnection> {
    const protocol = this.useTls ? "wss" : "ws";
    const url = `${protocol}://${host}:${port}${this.path}`;

    return new Promise((resolve, reject) => {
      const ws = new WebSocket(url);

      const timeout = setTimeout(() => {
        ws.close();
        reject(new TransportError("Connection timeout", TransportErrorCode.Timeout));
      }, 30000);

      ws.onopen = () => {
        clearTimeout(timeout);
        resolve(new WebSocketConnection(ws, `${host}:${port}`));
      };

      ws.onerror = (event: Event) => {
        clearTimeout(timeout);
        reject(
          new TransportError(`Connection failed: ${event.type}`, TransportErrorCode.ConnectionFailed)
        );
      };
    });
  }
}
