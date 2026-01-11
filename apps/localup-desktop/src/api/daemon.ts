import { invoke } from "@tauri-apps/api/core";

export interface DaemonStatus {
  running: boolean;
  version: string | null;
  uptime_seconds: number | null;
  tunnel_count: number | null;
}

export interface DaemonTunnelInfo {
  id: string;
  name: string;
  relay_address: string;
  local_host: string;
  local_port: number;
  protocol: string;
  subdomain: string | null;
  custom_domain: string | null;
  status: string;
  public_url: string | null;
  localup_id: string | null;
  error_message: string | null;
  started_at: string | null;
}

/**
 * Get daemon status
 */
export async function getDaemonStatus(): Promise<DaemonStatus> {
  return invoke<DaemonStatus>("get_daemon_status");
}

/**
 * Start the daemon
 */
export async function startDaemon(): Promise<DaemonStatus> {
  return invoke<DaemonStatus>("start_daemon");
}

/**
 * Stop the daemon
 */
export async function stopDaemon(): Promise<void> {
  return invoke("stop_daemon");
}

/**
 * List tunnels from daemon
 */
export async function daemonListTunnels(): Promise<DaemonTunnelInfo[]> {
  return invoke<DaemonTunnelInfo[]>("daemon_list_tunnels");
}

/**
 * Get a tunnel from daemon
 */
export async function daemonGetTunnel(id: string): Promise<DaemonTunnelInfo> {
  return invoke<DaemonTunnelInfo>("daemon_get_tunnel", { id });
}

/**
 * Start a tunnel via daemon
 */
export async function daemonStartTunnel(params: {
  id: string;
  name: string;
  relay_address: string;
  auth_token: string;
  local_host: string;
  local_port: number;
  protocol: string;
  subdomain?: string;
  custom_domain?: string;
}): Promise<DaemonTunnelInfo> {
  return invoke<DaemonTunnelInfo>("daemon_start_tunnel", {
    id: params.id,
    name: params.name,
    relayAddress: params.relay_address,
    authToken: params.auth_token,
    localHost: params.local_host,
    localPort: params.local_port,
    protocol: params.protocol,
    subdomain: params.subdomain ?? null,
    customDomain: params.custom_domain ?? null,
  });
}

/**
 * Stop a tunnel via daemon
 */
export async function daemonStopTunnel(id: string): Promise<void> {
  return invoke("daemon_stop_tunnel", { id });
}

/**
 * Delete a tunnel via daemon
 */
export async function daemonDeleteTunnel(id: string): Promise<void> {
  return invoke("daemon_delete_tunnel", { id });
}
