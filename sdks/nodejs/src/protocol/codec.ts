/**
 * Bincode-compatible codec for LocalUp protocol messages
 *
 * Wire format:
 * [4-byte BE length][bincode-serialized payload]
 *
 * Bincode uses:
 * - Little-endian for numbers
 * - Length-prefixed strings (u64 length + UTF-8 bytes)
 * - Length-prefixed vectors (u64 length + items)
 * - Enum discriminant as u32
 * - Option as 0/1 byte prefix
 */

import {
  type TunnelMessage,
  type Protocol,
  type TunnelConfig,
  type Endpoint,
  type ExitNodeConfig,
  type AgentMetadata,
  MessageDiscriminant,
  MAX_FRAME_SIZE,
} from "./types.ts";

// ============================================================================
// Buffer Writer
// ============================================================================

class BincodeWriter {
  private buffer: Uint8Array;
  private view: DataView;
  private offset: number;

  constructor(initialSize = 4096) {
    this.buffer = new Uint8Array(initialSize);
    this.view = new DataView(this.buffer.buffer);
    this.offset = 0;
  }

  private ensureCapacity(needed: number): void {
    if (this.offset + needed > this.buffer.length) {
      const newSize = Math.max(this.buffer.length * 2, this.offset + needed);
      const newBuffer = new Uint8Array(newSize);
      newBuffer.set(this.buffer);
      this.buffer = newBuffer;
      this.view = new DataView(this.buffer.buffer);
    }
  }

  writeU8(value: number): void {
    this.ensureCapacity(1);
    this.buffer[this.offset++] = value & 0xff;
  }

  writeU16(value: number): void {
    this.ensureCapacity(2);
    this.view.setUint16(this.offset, value, true); // little-endian
    this.offset += 2;
  }

  writeU32(value: number): void {
    this.ensureCapacity(4);
    this.view.setUint32(this.offset, value, true); // little-endian
    this.offset += 4;
  }

  writeU64(value: bigint): void {
    this.ensureCapacity(8);
    this.view.setBigUint64(this.offset, value, true); // little-endian
    this.offset += 8;
  }

  writeString(value: string): void {
    const bytes = new TextEncoder().encode(value);
    this.writeU64(BigInt(bytes.length));
    this.writeBytes(bytes);
  }

  writeBytes(value: Uint8Array): void {
    this.ensureCapacity(value.length);
    this.buffer.set(value, this.offset);
    this.offset += value.length;
  }

  writeLengthPrefixedBytes(value: Uint8Array): void {
    this.writeU64(BigInt(value.length));
    this.writeBytes(value);
  }

  writeOption<T>(value: T | null, write: (v: T) => void): void {
    if (value === null || value === undefined) {
      this.writeU8(0);
    } else {
      this.writeU8(1);
      write(value);
    }
  }

  writeArray<T>(values: T[], write: (v: T) => void): void {
    this.writeU64(BigInt(values.length));
    for (const v of values) {
      write(v);
    }
  }

  writeBool(value: boolean): void {
    this.writeU8(value ? 1 : 0);
  }

  getBytes(): Uint8Array {
    return this.buffer.slice(0, this.offset);
  }
}

// ============================================================================
// Buffer Reader
// ============================================================================

class BincodeReader {
  private view: DataView;
  private offset: number;
  private bytes: Uint8Array;

  constructor(buffer: Uint8Array) {
    this.bytes = buffer;
    this.view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);
    this.offset = 0;
  }

  readU8(): number {
    const value = this.bytes[this.offset]!;
    this.offset += 1;
    return value;
  }

  readU16(): number {
    const value = this.view.getUint16(this.offset, true);
    this.offset += 2;
    return value;
  }

  readU32(): number {
    const value = this.view.getUint32(this.offset, true);
    this.offset += 4;
    return value;
  }

  readU64(): bigint {
    const value = this.view.getBigUint64(this.offset, true);
    this.offset += 8;
    return value;
  }

  readString(): string {
    const len = Number(this.readU64());
    const bytes = this.readBytesRaw(len);
    return new TextDecoder().decode(bytes);
  }

  readBytesRaw(length: number): Uint8Array {
    const value = this.bytes.slice(this.offset, this.offset + length);
    this.offset += length;
    return value;
  }

  readLengthPrefixedBytes(): Uint8Array {
    const len = Number(this.readU64());
    return this.readBytesRaw(len);
  }

  readOption<T>(read: () => T): T | null {
    const hasValue = this.readU8();
    return hasValue ? read() : null;
  }

  readArray<T>(read: () => T): T[] {
    const len = Number(this.readU64());
    const result: T[] = [];
    for (let i = 0; i < len; i++) {
      result.push(read());
    }
    return result;
  }

  readBool(): boolean {
    return this.readU8() !== 0;
  }

  remaining(): number {
    return this.bytes.length - this.offset;
  }
}

// ============================================================================
// Protocol Serialization
// ============================================================================

function writeProtocol(w: BincodeWriter, protocol: Protocol): void {
  switch (protocol.type) {
    case "Tcp":
      w.writeU32(0);
      w.writeU16(protocol.port);
      break;
    case "Tls":
      w.writeU32(1);
      w.writeU16(protocol.port);
      w.writeString(protocol.sniPattern);
      break;
    case "Http":
      w.writeU32(2);
      w.writeOption(protocol.subdomain, (s) => w.writeString(s));
      break;
    case "Https":
      w.writeU32(3);
      w.writeOption(protocol.subdomain, (s) => w.writeString(s));
      break;
  }
}

function readProtocol(r: BincodeReader): Protocol {
  const disc = r.readU32();
  switch (disc) {
    case 0:
      return { type: "Tcp", port: r.readU16() };
    case 1:
      return { type: "Tls", port: r.readU16(), sniPattern: r.readString() };
    case 2:
      return { type: "Http", subdomain: r.readOption(() => r.readString()) };
    case 3:
      return { type: "Https", subdomain: r.readOption(() => r.readString()) };
    default:
      throw new Error(`Unknown protocol discriminant: ${disc}`);
  }
}

function writeExitNodeConfig(w: BincodeWriter, config: ExitNodeConfig): void {
  switch (config.type) {
    case "Auto":
      w.writeU32(0);
      break;
    case "Nearest":
      w.writeU32(1);
      break;
    case "Specific":
      w.writeU32(2);
      writeRegion(w, config.region);
      break;
    case "MultiRegion":
      w.writeU32(3);
      w.writeArray(config.regions, (r) => writeRegion(w, r));
      break;
    case "Custom":
      w.writeU32(4);
      w.writeString(config.address);
      break;
  }
}

function readExitNodeConfig(r: BincodeReader): ExitNodeConfig {
  const disc = r.readU32();
  switch (disc) {
    case 0:
      return { type: "Auto" };
    case 1:
      return { type: "Nearest" };
    case 2:
      return { type: "Specific", region: readRegion(r) };
    case 3:
      return { type: "MultiRegion", regions: r.readArray(() => readRegion(r)) };
    case 4:
      return { type: "Custom", address: r.readString() };
    default:
      throw new Error(`Unknown exit node config discriminant: ${disc}`);
  }
}

type Region = "UsEast" | "UsWest" | "EuWest" | "EuCentral" | "AsiaPacific" | "SouthAmerica";

const REGIONS: Region[] = [
  "UsEast",
  "UsWest",
  "EuWest",
  "EuCentral",
  "AsiaPacific",
  "SouthAmerica",
];

function writeRegion(w: BincodeWriter, region: Region): void {
  const idx = REGIONS.indexOf(region);
  if (idx === -1) throw new Error(`Unknown region: ${region}`);
  w.writeU32(idx);
}

function readRegion(r: BincodeReader): Region {
  const idx = r.readU32();
  const region = REGIONS[idx];
  if (!region) throw new Error(`Unknown region index: ${idx}`);
  return region;
}

function writeTunnelConfig(w: BincodeWriter, config: TunnelConfig): void {
  w.writeString(config.localHost);
  w.writeOption(config.localPort, (p) => w.writeU16(p));
  w.writeBool(config.localHttps);
  writeExitNodeConfig(w, config.exitNode);
  w.writeBool(config.failover);
  w.writeArray(config.ipAllowlist, (ip) => w.writeString(ip));
  w.writeBool(config.enableCompression);
  w.writeBool(config.enableMultiplexing);
}

function readTunnelConfig(r: BincodeReader): TunnelConfig {
  return {
    localHost: r.readString(),
    localPort: r.readOption(() => r.readU16()),
    localHttps: r.readBool(),
    exitNode: readExitNodeConfig(r),
    failover: r.readBool(),
    ipAllowlist: r.readArray(() => r.readString()),
    enableCompression: r.readBool(),
    enableMultiplexing: r.readBool(),
  };
}

function writeEndpoint(w: BincodeWriter, endpoint: Endpoint): void {
  writeProtocol(w, endpoint.protocol);
  w.writeString(endpoint.publicUrl);
  w.writeOption(endpoint.port, (p) => w.writeU16(p));
}

function readEndpoint(r: BincodeReader): Endpoint {
  return {
    protocol: readProtocol(r),
    publicUrl: r.readString(),
    port: r.readOption(() => r.readU16()),
  };
}

function writeAgentMetadata(w: BincodeWriter, metadata: AgentMetadata): void {
  w.writeString(metadata.hostname);
  w.writeString(metadata.platform);
  w.writeString(metadata.version);
  w.writeOption(metadata.location, (l) => w.writeString(l));
}

function readAgentMetadata(r: BincodeReader): AgentMetadata {
  return {
    hostname: r.readString(),
    platform: r.readString(),
    version: r.readString(),
    location: r.readOption(() => r.readString()),
  };
}

// ============================================================================
// Message Serialization
// ============================================================================

function writeMessage(w: BincodeWriter, msg: TunnelMessage): void {
  const disc = MessageDiscriminant[msg.type];
  w.writeU32(disc);

  switch (msg.type) {
    case "Ping":
      w.writeU64(msg.timestamp);
      break;
    case "Pong":
      w.writeU64(msg.timestamp);
      break;
    case "Connect":
      w.writeString(msg.localupId);
      w.writeString(msg.authToken);
      w.writeArray(msg.protocols, (p) => writeProtocol(w, p));
      writeTunnelConfig(w, msg.config);
      break;
    case "Connected":
      w.writeString(msg.localupId);
      w.writeArray(msg.endpoints, (e) => writeEndpoint(w, e));
      break;
    case "Disconnect":
      w.writeString(msg.reason);
      break;
    case "DisconnectAck":
      w.writeString(msg.localupId);
      break;
    case "TcpConnect":
      w.writeU32(msg.streamId);
      w.writeString(msg.remoteAddr);
      w.writeU16(msg.remotePort);
      break;
    case "TcpData":
      w.writeU32(msg.streamId);
      w.writeLengthPrefixedBytes(msg.data);
      break;
    case "TcpClose":
      w.writeU32(msg.streamId);
      break;
    case "TlsConnect":
      w.writeU32(msg.streamId);
      w.writeString(msg.sni);
      w.writeLengthPrefixedBytes(msg.clientHello);
      break;
    case "TlsData":
      w.writeU32(msg.streamId);
      w.writeLengthPrefixedBytes(msg.data);
      break;
    case "TlsClose":
      w.writeU32(msg.streamId);
      break;
    case "HttpRequest":
      w.writeU32(msg.streamId);
      w.writeString(msg.method);
      w.writeString(msg.uri);
      w.writeArray(msg.headers, ([k, v]) => {
        w.writeString(k);
        w.writeString(v);
      });
      w.writeOption(msg.body, (b) => w.writeLengthPrefixedBytes(b));
      break;
    case "HttpResponse":
      w.writeU32(msg.streamId);
      w.writeU16(msg.status);
      w.writeArray(msg.headers, ([k, v]) => {
        w.writeString(k);
        w.writeString(v);
      });
      w.writeOption(msg.body, (b) => w.writeLengthPrefixedBytes(b));
      break;
    case "HttpChunk":
      w.writeU32(msg.streamId);
      w.writeLengthPrefixedBytes(msg.chunk);
      w.writeBool(msg.isFinal);
      break;
    case "HttpStreamConnect":
      w.writeU32(msg.streamId);
      w.writeString(msg.host);
      w.writeLengthPrefixedBytes(msg.initialData);
      break;
    case "HttpStreamData":
      w.writeU32(msg.streamId);
      w.writeLengthPrefixedBytes(msg.data);
      break;
    case "HttpStreamClose":
      w.writeU32(msg.streamId);
      break;
    case "AgentRegister":
      w.writeString(msg.agentId);
      w.writeString(msg.authToken);
      w.writeString(msg.targetAddress);
      writeAgentMetadata(w, msg.metadata);
      break;
    case "AgentRegistered":
      w.writeString(msg.agentId);
      break;
    case "AgentRejected":
      w.writeString(msg.reason);
      break;
    case "ReverseTunnelRequest":
      w.writeString(msg.localupId);
      w.writeString(msg.remoteAddress);
      w.writeString(msg.agentId);
      w.writeOption(msg.agentToken, (t) => w.writeString(t));
      break;
    case "ReverseTunnelAccept":
      w.writeString(msg.localupId);
      w.writeString(msg.localAddress);
      break;
    case "ReverseTunnelReject":
      w.writeString(msg.localupId);
      w.writeString(msg.reason);
      break;
    case "ReverseConnect":
      w.writeString(msg.localupId);
      w.writeU32(msg.streamId);
      w.writeString(msg.remoteAddress);
      break;
    case "ValidateAgentToken":
      w.writeOption(msg.agentToken, (t) => w.writeString(t));
      break;
    case "ValidateAgentTokenOk":
      // No fields
      break;
    case "ValidateAgentTokenReject":
      w.writeString(msg.reason);
      break;
    case "ForwardRequest":
      w.writeString(msg.localupId);
      w.writeU32(msg.streamId);
      w.writeString(msg.remoteAddress);
      w.writeOption(msg.agentToken, (t) => w.writeString(t));
      break;
    case "ForwardAccept":
      w.writeString(msg.localupId);
      w.writeU32(msg.streamId);
      break;
    case "ForwardReject":
      w.writeString(msg.localupId);
      w.writeU32(msg.streamId);
      w.writeString(msg.reason);
      break;
    case "ReverseData":
      w.writeString(msg.localupId);
      w.writeU32(msg.streamId);
      w.writeLengthPrefixedBytes(msg.data);
      break;
    case "ReverseClose":
      w.writeString(msg.localupId);
      w.writeU32(msg.streamId);
      w.writeOption(msg.reason, (r) => w.writeString(r));
      break;
  }
}

function readMessage(r: BincodeReader): TunnelMessage {
  const disc = r.readU32();

  switch (disc) {
    case MessageDiscriminant.Ping:
      return { type: "Ping", timestamp: r.readU64() };
    case MessageDiscriminant.Pong:
      return { type: "Pong", timestamp: r.readU64() };
    case MessageDiscriminant.Connect:
      return {
        type: "Connect",
        localupId: r.readString(),
        authToken: r.readString(),
        protocols: r.readArray(() => readProtocol(r)),
        config: readTunnelConfig(r),
      };
    case MessageDiscriminant.Connected:
      return {
        type: "Connected",
        localupId: r.readString(),
        endpoints: r.readArray(() => readEndpoint(r)),
      };
    case MessageDiscriminant.Disconnect:
      return { type: "Disconnect", reason: r.readString() };
    case MessageDiscriminant.DisconnectAck:
      return { type: "DisconnectAck", localupId: r.readString() };
    case MessageDiscriminant.TcpConnect:
      return {
        type: "TcpConnect",
        streamId: r.readU32(),
        remoteAddr: r.readString(),
        remotePort: r.readU16(),
      };
    case MessageDiscriminant.TcpData:
      return {
        type: "TcpData",
        streamId: r.readU32(),
        data: r.readLengthPrefixedBytes(),
      };
    case MessageDiscriminant.TcpClose:
      return { type: "TcpClose", streamId: r.readU32() };
    case MessageDiscriminant.TlsConnect:
      return {
        type: "TlsConnect",
        streamId: r.readU32(),
        sni: r.readString(),
        clientHello: r.readLengthPrefixedBytes(),
      };
    case MessageDiscriminant.TlsData:
      return {
        type: "TlsData",
        streamId: r.readU32(),
        data: r.readLengthPrefixedBytes(),
      };
    case MessageDiscriminant.TlsClose:
      return { type: "TlsClose", streamId: r.readU32() };
    case MessageDiscriminant.HttpRequest:
      return {
        type: "HttpRequest",
        streamId: r.readU32(),
        method: r.readString(),
        uri: r.readString(),
        headers: r.readArray(() => [r.readString(), r.readString()] as [string, string]),
        body: r.readOption(() => r.readLengthPrefixedBytes()),
      };
    case MessageDiscriminant.HttpResponse:
      return {
        type: "HttpResponse",
        streamId: r.readU32(),
        status: r.readU16(),
        headers: r.readArray(() => [r.readString(), r.readString()] as [string, string]),
        body: r.readOption(() => r.readLengthPrefixedBytes()),
      };
    case MessageDiscriminant.HttpChunk:
      return {
        type: "HttpChunk",
        streamId: r.readU32(),
        chunk: r.readLengthPrefixedBytes(),
        isFinal: r.readBool(),
      };
    case MessageDiscriminant.HttpStreamConnect:
      return {
        type: "HttpStreamConnect",
        streamId: r.readU32(),
        host: r.readString(),
        initialData: r.readLengthPrefixedBytes(),
      };
    case MessageDiscriminant.HttpStreamData:
      return {
        type: "HttpStreamData",
        streamId: r.readU32(),
        data: r.readLengthPrefixedBytes(),
      };
    case MessageDiscriminant.HttpStreamClose:
      return { type: "HttpStreamClose", streamId: r.readU32() };
    case MessageDiscriminant.AgentRegister:
      return {
        type: "AgentRegister",
        agentId: r.readString(),
        authToken: r.readString(),
        targetAddress: r.readString(),
        metadata: readAgentMetadata(r),
      };
    case MessageDiscriminant.AgentRegistered:
      return { type: "AgentRegistered", agentId: r.readString() };
    case MessageDiscriminant.AgentRejected:
      return { type: "AgentRejected", reason: r.readString() };
    case MessageDiscriminant.ReverseTunnelRequest:
      return {
        type: "ReverseTunnelRequest",
        localupId: r.readString(),
        remoteAddress: r.readString(),
        agentId: r.readString(),
        agentToken: r.readOption(() => r.readString()),
      };
    case MessageDiscriminant.ReverseTunnelAccept:
      return {
        type: "ReverseTunnelAccept",
        localupId: r.readString(),
        localAddress: r.readString(),
      };
    case MessageDiscriminant.ReverseTunnelReject:
      return {
        type: "ReverseTunnelReject",
        localupId: r.readString(),
        reason: r.readString(),
      };
    case MessageDiscriminant.ReverseConnect:
      return {
        type: "ReverseConnect",
        localupId: r.readString(),
        streamId: r.readU32(),
        remoteAddress: r.readString(),
      };
    case MessageDiscriminant.ValidateAgentToken:
      return {
        type: "ValidateAgentToken",
        agentToken: r.readOption(() => r.readString()),
      };
    case MessageDiscriminant.ValidateAgentTokenOk:
      return { type: "ValidateAgentTokenOk" };
    case MessageDiscriminant.ValidateAgentTokenReject:
      return { type: "ValidateAgentTokenReject", reason: r.readString() };
    case MessageDiscriminant.ForwardRequest:
      return {
        type: "ForwardRequest",
        localupId: r.readString(),
        streamId: r.readU32(),
        remoteAddress: r.readString(),
        agentToken: r.readOption(() => r.readString()),
      };
    case MessageDiscriminant.ForwardAccept:
      return {
        type: "ForwardAccept",
        localupId: r.readString(),
        streamId: r.readU32(),
      };
    case MessageDiscriminant.ForwardReject:
      return {
        type: "ForwardReject",
        localupId: r.readString(),
        streamId: r.readU32(),
        reason: r.readString(),
      };
    case MessageDiscriminant.ReverseData:
      return {
        type: "ReverseData",
        localupId: r.readString(),
        streamId: r.readU32(),
        data: r.readLengthPrefixedBytes(),
      };
    case MessageDiscriminant.ReverseClose:
      return {
        type: "ReverseClose",
        localupId: r.readString(),
        streamId: r.readU32(),
        reason: r.readOption(() => r.readString()),
      };
    default:
      throw new Error(`Unknown message discriminant: ${disc}`);
  }
}

// ============================================================================
// Public API
// ============================================================================

/**
 * Encode a message to wire format
 *
 * Wire format: [4-byte BE length][bincode payload]
 */
export function encodeMessage(msg: TunnelMessage): Uint8Array {
  const w = new BincodeWriter();
  writeMessage(w, msg);
  const payload = w.getBytes();

  if (payload.length > MAX_FRAME_SIZE) {
    throw new Error(`Message too large: ${payload.length} > ${MAX_FRAME_SIZE}`);
  }

  // Prepend 4-byte big-endian length header
  const result = new Uint8Array(4 + payload.length);
  const view = new DataView(result.buffer);
  view.setUint32(0, payload.length, false); // big-endian
  result.set(payload, 4);

  return result;
}

/**
 * Decode a message from wire format
 *
 * Expects: [4-byte BE length][bincode payload]
 */
export function decodeMessage(data: Uint8Array): TunnelMessage {
  if (data.length < 4) {
    throw new Error("Buffer too small for length header");
  }

  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  const length = view.getUint32(0, false); // big-endian

  if (length > MAX_FRAME_SIZE) {
    throw new Error(`Frame too large: ${length} > ${MAX_FRAME_SIZE}`);
  }

  if (data.length < 4 + length) {
    throw new Error(`Incomplete frame: expected ${4 + length}, got ${data.length}`);
  }

  const payload = data.slice(4, 4 + length);
  const r = new BincodeReader(payload);
  return readMessage(r);
}

/**
 * Decode just the payload (without length header)
 */
export function decodeMessagePayload(payload: Uint8Array): TunnelMessage {
  const r = new BincodeReader(payload);
  return readMessage(r);
}

/**
 * Encode just the payload (without length header)
 */
export function encodeMessagePayload(msg: TunnelMessage): Uint8Array {
  const w = new BincodeWriter();
  writeMessage(w, msg);
  return w.getBytes();
}

/**
 * Frame accumulator for streaming reads
 */
export class FrameAccumulator {
  private buffer: Uint8Array;
  private writeOffset: number;

  constructor(initialSize = 65536) {
    this.buffer = new Uint8Array(initialSize);
    this.writeOffset = 0;
  }

  /**
   * Add data to the accumulator
   */
  push(data: Uint8Array): void {
    if (this.writeOffset + data.length > this.buffer.length) {
      const newSize = Math.max(this.buffer.length * 2, this.writeOffset + data.length);
      const newBuffer = new Uint8Array(newSize);
      newBuffer.set(this.buffer.slice(0, this.writeOffset));
      this.buffer = newBuffer;
    }
    this.buffer.set(data, this.writeOffset);
    this.writeOffset += data.length;
  }

  /**
   * Try to extract a complete message
   * Returns null if not enough data available
   */
  tryReadMessage(): TunnelMessage | null {
    if (this.writeOffset < 4) {
      return null;
    }

    const view = new DataView(this.buffer.buffer, 0, this.writeOffset);
    const length = view.getUint32(0, false); // big-endian

    if (length > MAX_FRAME_SIZE) {
      throw new Error(`Frame too large: ${length} > ${MAX_FRAME_SIZE}`);
    }

    const frameSize = 4 + length;
    if (this.writeOffset < frameSize) {
      return null;
    }

    // Extract and decode the message
    const payload = this.buffer.slice(4, frameSize);
    const msg = decodeMessagePayload(payload);

    // Shift remaining data to the beginning
    const remaining = this.writeOffset - frameSize;
    if (remaining > 0) {
      this.buffer.copyWithin(0, frameSize, this.writeOffset);
    }
    this.writeOffset = remaining;

    return msg;
  }

  /**
   * Get all available complete messages
   */
  readAllMessages(): TunnelMessage[] {
    const messages: TunnelMessage[] = [];
    let msg: TunnelMessage | null;
    while ((msg = this.tryReadMessage()) !== null) {
      messages.push(msg);
    }
    return messages;
  }

  /**
   * Clear the accumulator
   */
  clear(): void {
    this.writeOffset = 0;
  }

  /**
   * Get current buffer size
   */
  size(): number {
    return this.writeOffset;
  }

  /**
   * Read raw bytes from the accumulator (for non-message data)
   */
  readRawBytes(maxSize: number): Uint8Array | null {
    if (this.writeOffset === 0) {
      return null;
    }

    const toRead = Math.min(maxSize, this.writeOffset);
    const result = this.buffer.slice(0, toRead);

    // Shift remaining data
    const remaining = this.writeOffset - toRead;
    if (remaining > 0) {
      this.buffer.copyWithin(0, toRead, this.writeOffset);
    }
    this.writeOffset = remaining;

    return result;
  }
}
