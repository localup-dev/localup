/**
 * Transport abstraction layer
 *
 * Provides a common interface for different transport protocols:
 * - QUIC (via @aspect/quic or similar)
 * - HTTP/2 (via Node.js built-in http2)
 * - WebSocket (via ws or built-in WebSocket)
 */

import type { TunnelMessage } from "../protocol/types.ts";

/**
 * Transport stream - represents a bidirectional stream
 */
export interface TransportStream {
  /**
   * Unique stream ID
   */
  readonly streamId: number;

  /**
   * Send a message on this stream
   */
  sendMessage(message: TunnelMessage): Promise<void>;

  /**
   * Receive a message from this stream
   * Returns null if stream is closed
   */
  recvMessage(): Promise<TunnelMessage | null>;

  /**
   * Send raw bytes
   */
  sendBytes(data: Uint8Array): Promise<void>;

  /**
   * Receive raw bytes (up to maxSize)
   */
  recvBytes(maxSize: number): Promise<Uint8Array | null>;

  /**
   * Close the stream gracefully
   */
  close(): Promise<void>;

  /**
   * Check if stream is closed
   */
  isClosed(): boolean;
}

/**
 * Transport connection - represents a multiplexed connection
 */
export interface TransportConnection {
  /**
   * Open a new bidirectional stream
   */
  openStream(): Promise<TransportStream>;

  /**
   * Accept incoming streams from the remote
   */
  acceptStream(): Promise<TransportStream | null>;

  /**
   * Close the connection
   */
  close(errorCode?: number, reason?: string): Promise<void>;

  /**
   * Check if connection is closed
   */
  isClosed(): boolean;

  /**
   * Get remote address
   */
  remoteAddress(): string;

  /**
   * Get connection stats
   */
  stats(): ConnectionStats;
}

/**
 * Transport connector - creates connections
 */
export interface TransportConnector {
  /**
   * Protocol name
   */
  readonly protocol: string;

  /**
   * Connect to a remote address
   */
  connect(host: string, port: number, serverName?: string): Promise<TransportConnection>;
}

/**
 * Connection statistics
 */
export interface ConnectionStats {
  bytesSent: bigint;
  bytesReceived: bigint;
  streamCount: number;
  roundTripTime?: number;
}

/**
 * Transport error types
 */
export class TransportError extends Error {
  public readonly code: TransportErrorCode;
  public override readonly cause?: Error;

  constructor(message: string, code: TransportErrorCode, cause?: Error) {
    super(message, { cause });
    this.name = "TransportError";
    this.code = code;
    this.cause = cause;
  }
}

export enum TransportErrorCode {
  ConnectionFailed = "CONNECTION_FAILED",
  ConnectionClosed = "CONNECTION_CLOSED",
  StreamClosed = "STREAM_CLOSED",
  Timeout = "TIMEOUT",
  TlsError = "TLS_ERROR",
  ProtocolError = "PROTOCOL_ERROR",
  IoError = "IO_ERROR",
}
