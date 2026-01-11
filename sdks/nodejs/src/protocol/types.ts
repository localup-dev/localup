/**
 * Protocol types matching the Rust localup-proto crate
 *
 * These types are serialized using bincode for wire communication.
 */

// ============================================================================
// Protocol Constants
// ============================================================================

export const PROTOCOL_VERSION = 1;
export const MAX_FRAME_SIZE = 16 * 1024 * 1024; // 16 MB
export const CONTROL_STREAM_ID = 0;

// ============================================================================
// Enums
// ============================================================================

/**
 * Protocol configuration - what type of tunnel to create
 */
export type Protocol =
  | { type: "Tcp"; port: number }
  | { type: "Tls"; port: number; sniPattern: string }
  | { type: "Http"; subdomain: string | null }
  | { type: "Https"; subdomain: string | null };

/**
 * Geographic regions for exit node selection
 */
export type Region =
  | "UsEast"
  | "UsWest"
  | "EuWest"
  | "EuCentral"
  | "AsiaPacific"
  | "SouthAmerica";

/**
 * Exit node configuration
 */
export type ExitNodeConfig =
  | { type: "Auto" }
  | { type: "Nearest" }
  | { type: "Specific"; region: Region }
  | { type: "MultiRegion"; regions: Region[] }
  | { type: "Custom"; address: string };

// ============================================================================
// Data Structures
// ============================================================================

/**
 * Tunnel endpoint information returned by the relay
 */
export interface Endpoint {
  protocol: Protocol;
  publicUrl: string;
  port: number | null;
}

/**
 * Tunnel configuration sent to relay
 */
export interface TunnelConfig {
  localHost: string;
  localPort: number | null;
  localHttps: boolean;
  exitNode: ExitNodeConfig;
  failover: boolean;
  ipAllowlist: string[];
  enableCompression: boolean;
  enableMultiplexing: boolean;
}

/**
 * Agent metadata for identification
 */
export interface AgentMetadata {
  hostname: string;
  platform: string;
  version: string;
  location: string | null;
}

// ============================================================================
// Tunnel Messages
// ============================================================================

/**
 * Main tunnel protocol message enum - matches Rust TunnelMessage
 *
 * Each variant has a numeric discriminant for bincode serialization.
 */
export type TunnelMessage =
  // Control messages (Stream 0)
  | { type: "Ping"; timestamp: bigint }
  | { type: "Pong"; timestamp: bigint }
  | {
      type: "Connect";
      localupId: string;
      authToken: string;
      protocols: Protocol[];
      config: TunnelConfig;
    }
  | { type: "Connected"; localupId: string; endpoints: Endpoint[] }
  | { type: "Disconnect"; reason: string }
  | { type: "DisconnectAck"; localupId: string }

  // TCP Protocol
  | { type: "TcpConnect"; streamId: number; remoteAddr: string; remotePort: number }
  | { type: "TcpData"; streamId: number; data: Uint8Array }
  | { type: "TcpClose"; streamId: number }

  // TLS/SNI Protocol
  | { type: "TlsConnect"; streamId: number; sni: string; clientHello: Uint8Array }
  | { type: "TlsData"; streamId: number; data: Uint8Array }
  | { type: "TlsClose"; streamId: number }

  // HTTP Protocol
  | {
      type: "HttpRequest";
      streamId: number;
      method: string;
      uri: string;
      headers: [string, string][];
      body: Uint8Array | null;
    }
  | {
      type: "HttpResponse";
      streamId: number;
      status: number;
      headers: [string, string][];
      body: Uint8Array | null;
    }
  | { type: "HttpChunk"; streamId: number; chunk: Uint8Array; isFinal: boolean }

  // HTTP Streaming (WebSocket, SSE, HTTP/2)
  | { type: "HttpStreamConnect"; streamId: number; host: string; initialData: Uint8Array }
  | { type: "HttpStreamData"; streamId: number; data: Uint8Array }
  | { type: "HttpStreamClose"; streamId: number }

  // Reverse Tunnel (Agent-based)
  | {
      type: "AgentRegister";
      agentId: string;
      authToken: string;
      targetAddress: string;
      metadata: AgentMetadata;
    }
  | { type: "AgentRegistered"; agentId: string }
  | { type: "AgentRejected"; reason: string }
  | {
      type: "ReverseTunnelRequest";
      localupId: string;
      remoteAddress: string;
      agentId: string;
      agentToken: string | null;
    }
  | { type: "ReverseTunnelAccept"; localupId: string; localAddress: string }
  | { type: "ReverseTunnelReject"; localupId: string; reason: string }
  | { type: "ReverseConnect"; localupId: string; streamId: number; remoteAddress: string }
  | { type: "ValidateAgentToken"; agentToken: string | null }
  | { type: "ValidateAgentTokenOk" }
  | { type: "ValidateAgentTokenReject"; reason: string }
  | {
      type: "ForwardRequest";
      localupId: string;
      streamId: number;
      remoteAddress: string;
      agentToken: string | null;
    }
  | { type: "ForwardAccept"; localupId: string; streamId: number }
  | { type: "ForwardReject"; localupId: string; streamId: number; reason: string }
  | { type: "ReverseData"; localupId: string; streamId: number; data: Uint8Array }
  | { type: "ReverseClose"; localupId: string; streamId: number; reason: string | null };

// ============================================================================
// Message Discriminants (for bincode enum serialization)
// ============================================================================

/**
 * Enum discriminants matching Rust's bincode serialization order
 */
export const MessageDiscriminant = {
  Ping: 0,
  Pong: 1,
  Connect: 2,
  Connected: 3,
  Disconnect: 4,
  DisconnectAck: 5,
  TcpConnect: 6,
  TcpData: 7,
  TcpClose: 8,
  TlsConnect: 9,
  TlsData: 10,
  TlsClose: 11,
  HttpRequest: 12,
  HttpResponse: 13,
  HttpChunk: 14,
  HttpStreamConnect: 15,
  HttpStreamData: 16,
  HttpStreamClose: 17,
  AgentRegister: 18,
  AgentRegistered: 19,
  AgentRejected: 20,
  ReverseTunnelRequest: 21,
  ReverseTunnelAccept: 22,
  ReverseTunnelReject: 23,
  ReverseConnect: 24,
  ValidateAgentToken: 25,
  ValidateAgentTokenOk: 26,
  ValidateAgentTokenReject: 27,
  ForwardRequest: 28,
  ForwardAccept: 29,
  ForwardReject: 30,
  ReverseData: 31,
  ReverseClose: 32,
} as const;

export type MessageType = keyof typeof MessageDiscriminant;

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Create default tunnel config
 */
export function createDefaultTunnelConfig(overrides: Partial<TunnelConfig> = {}): TunnelConfig {
  return {
    localHost: "localhost",
    localPort: null,
    localHttps: false,
    exitNode: { type: "Auto" },
    failover: true,
    ipAllowlist: [],
    enableCompression: false,
    enableMultiplexing: true,
    ...overrides,
  };
}
