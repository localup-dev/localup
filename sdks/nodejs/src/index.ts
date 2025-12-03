/**
 * LocalUp Node.js SDK
 *
 * A client library for creating secure tunnels to expose local servers.
 *
 * @example
 * ```typescript
 * import localup from '@localup/sdk';
 *
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

// Main API
export { forward, type ForwardOptions, type Listener } from "./client.ts";
export { default as localup } from "./client.ts";

// Protocol types
export type {
  Protocol,
  Endpoint,
  TunnelConfig,
  TunnelMessage,
  ExitNodeConfig,
  Region,
} from "./protocol/types.ts";

export {
  PROTOCOL_VERSION,
  MAX_FRAME_SIZE,
  createDefaultTunnelConfig,
} from "./protocol/types.ts";

// Codec
export { encodeMessage, decodeMessage, FrameAccumulator } from "./protocol/codec.ts";

// Transport
export type {
  TransportStream,
  TransportConnection,
  TransportConnector,
  ConnectionStats,
} from "./transport/base.ts";
export { TransportError, TransportErrorCode } from "./transport/base.ts";
export { WebSocketConnector } from "./transport/websocket.ts";
export { H2Connector } from "./transport/h2.ts";
export { QuicConnector, isQuicAvailable, getQuicUnavailableReason } from "./transport/quic.ts";

// Logging
export { setLogLevel, getLogLevel, logger, type LogLevel } from "./utils/logger.ts";

// Default export for convenience
import localup from "./client.ts";
export default localup;
