import { invoke } from "@tauri-apps/api/core";

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
  error_message: string | null;
  created_at: string;
  updated_at: string;
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
