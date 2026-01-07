import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface Tunnel {
  id: string;
  name: string;
  relay_id: string;
  relay_name: string | null;
  local_host: string;
  local_port: number;
  protocol: string;
  subdomain: string | null;
  custom_domain: string | null;
  auto_start: boolean;
  enabled: boolean;
  status: string;
  public_url: string | null;
  localup_id: string | null;
  error_message: string | null;
  created_at: string;
  updated_at: string;
}

export interface CapturedRequest {
  id: string;
  tunnel_session_id: string;
  localup_id: string;
  method: string;
  path: string;
  host: string | null;
  headers: string;
  body: string | null;
  status: number | null;
  response_headers: string | null;
  response_body: string | null;
  created_at: string;
  latency_ms: number | null;
}

export interface CreateTunnelRequest {
  name: string;
  relay_id: string;
  local_host?: string;
  local_port: number;
  protocol: string;
  subdomain?: string;
  custom_domain?: string;
  auto_start?: boolean;
}

export interface UpdateTunnelRequest {
  name?: string;
  relay_id?: string;
  local_host?: string;
  local_port?: number;
  protocol?: string;
  subdomain?: string;
  custom_domain?: string;
  auto_start?: boolean;
  enabled?: boolean;
}

/**
 * List all tunnels
 */
export async function listTunnels(): Promise<Tunnel[]> {
  return invoke<Tunnel[]>("list_tunnels");
}

/**
 * Get a single tunnel by ID
 */
export async function getTunnel(id: string): Promise<Tunnel | null> {
  return invoke<Tunnel | null>("get_tunnel", { id });
}

/**
 * Create a new tunnel
 */
export async function createTunnel(
  request: CreateTunnelRequest
): Promise<Tunnel> {
  return invoke<Tunnel>("create_tunnel", { request });
}

/**
 * Update an existing tunnel
 */
export async function updateTunnel(
  id: string,
  request: UpdateTunnelRequest
): Promise<Tunnel> {
  return invoke<Tunnel>("update_tunnel", { id, request });
}

/**
 * Delete a tunnel
 */
export async function deleteTunnel(id: string): Promise<void> {
  return invoke("delete_tunnel", { id });
}

/**
 * Start a tunnel
 */
export async function startTunnel(id: string): Promise<Tunnel> {
  return invoke<Tunnel>("start_tunnel", { id });
}

/**
 * Stop a tunnel
 */
export async function stopTunnel(id: string): Promise<Tunnel> {
  return invoke<Tunnel>("stop_tunnel", { id });
}

/**
 * Get captured requests for a tunnel
 */
export async function getCapturedRequests(tunnelId: string): Promise<CapturedRequest[]> {
  return invoke<CapturedRequest[]>("get_captured_requests", { tunnelId });
}

// ============================================================================
// Real-time Metrics Types and Functions
// ============================================================================

/** Body content types */
export type BodyContent =
  | { type: "Json"; value: unknown }
  | { type: "Text"; value: string }
  | { type: "Binary"; value: { size: number } };

/** Body data wrapper */
export interface BodyData {
  content_type: string;
  size: number;
  data: BodyContent;
}

/** HTTP request/response metric */
export interface HttpMetric {
  id: string;
  stream_id: string;
  timestamp: number;
  method: string;
  uri: string;
  request_headers: [string, string][];
  request_body: BodyData | null;
  response_status: number | null;
  response_headers: [string, string][] | null;
  response_body: BodyData | null;
  duration_ms: number | null;
  error: string | null;
}

/** Metrics event types from backend */
export type MetricsEvent =
  | { type: "request"; metric: HttpMetric }
  | { type: "response"; id: string; status: number; headers: [string, string][]; body: BodyData | null; duration_ms: number }
  | { type: "error"; id: string; error: string; duration_ms: number }
  | { type: "tcp_connection"; connection: unknown }
  | { type: "tcp_data"; connection_id: string; bytes_in: number; bytes_out: number }
  | { type: "tcp_close"; connection_id: string }
  | { type: "stats"; stats: unknown };

/** Tunnel metrics event payload */
export interface TunnelMetricsPayload {
  tunnel_id: string;
  event: MetricsEvent;
}

/** Paginated metrics response */
export interface PaginatedMetricsResponse {
  items: HttpMetric[];
  total: number;
  offset: number;
  limit: number;
}

/**
 * Get real-time metrics for a tunnel with pagination (from in-memory store)
 */
export async function getTunnelMetrics(
  tunnelId: string,
  offset?: number,
  limit?: number
): Promise<PaginatedMetricsResponse> {
  return invoke<PaginatedMetricsResponse>("get_tunnel_metrics", {
    tunnelId,
    offset,
    limit,
  });
}

/**
 * Clear metrics for a tunnel
 */
export async function clearTunnelMetrics(tunnelId: string): Promise<void> {
  return invoke("clear_tunnel_metrics", { tunnelId });
}

/** Replay request parameters */
export interface ReplayRequestParams {
  method: string;
  uri: string;
  headers: [string, string][];
  body: string | null;
}

/** Replay response */
export interface ReplayResponse {
  status: number;
  headers: [string, string][];
  body: string | null;
  duration_ms: number;
}

/**
 * Replay a captured HTTP request to the local service
 */
export async function replayRequest(
  tunnelId: string,
  request: ReplayRequestParams
): Promise<ReplayResponse> {
  return invoke<ReplayResponse>("replay_request", { tunnelId, request });
}

/**
 * Subscribe to real-time metrics events for all tunnels.
 * Returns an unsubscribe function.
 */
export async function subscribeToMetrics(
  callback: (payload: TunnelMetricsPayload) => void
): Promise<UnlistenFn> {
  return listen<TunnelMetricsPayload>("tunnel-metrics", (event) => {
    callback(event.payload);
  });
}

/**
 * Subscribe to daemon metrics for a specific tunnel.
 * This starts a background task that forwards metrics from the daemon to the frontend.
 * The events will be received via the subscribeToMetrics listener.
 */
export async function subscribeDaemonMetrics(tunnelId: string): Promise<void> {
  return invoke("subscribe_daemon_metrics", { tunnelId });
}

/**
 * Subscription ended payload
 */
export interface SubscriptionEndedPayload {
  tunnel_id: string;
}

/**
 * Subscribe to the subscription-ended event.
 * This is called when the daemon metrics subscription ends (e.g., tunnel reconnects).
 * Returns an unsubscribe function.
 */
export async function subscribeToSubscriptionEnded(
  callback: (payload: SubscriptionEndedPayload) => void
): Promise<UnlistenFn> {
  return listen<SubscriptionEndedPayload>("tunnel-metrics-subscription-ended", (event) => {
    callback(event.payload);
  });
}

// ============================================================================
// TCP Connection Types and Functions
// ============================================================================

/** TCP connection info */
export interface TcpConnection {
  id: string;
  stream_id: string;
  timestamp: string;
  remote_addr: string;
  local_addr: string;
  state: string;
  bytes_received: number;
  bytes_sent: number;
  duration_ms: number | null;
  closed_at: string | null;
  error: string | null;
}

/** Paginated TCP connections response */
export interface PaginatedTcpConnectionsResponse {
  items: TcpConnection[];
  total: number;
  offset: number;
  limit: number;
}

/**
 * Get TCP connections for a tunnel with pagination
 */
export async function getTcpConnections(
  tunnelId: string,
  offset?: number,
  limit?: number
): Promise<PaginatedTcpConnectionsResponse> {
  return invoke<PaginatedTcpConnectionsResponse>("get_tcp_connections", {
    tunnelId,
    offset,
    limit,
  });
}
