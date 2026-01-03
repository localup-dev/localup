import { invoke } from "@tauri-apps/api/core";

export interface RelayServer {
  id: string;
  name: string;
  address: string;
  jwt_token: string | null;
  protocol: string;
  insecure: boolean;
  is_default: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateRelayRequest {
  name: string;
  address: string;
  jwt_token?: string | null;
  protocol?: string;
  insecure?: boolean;
  is_default?: boolean;
}

export interface UpdateRelayRequest {
  name?: string;
  address?: string;
  jwt_token?: string | null;
  protocol?: string;
  insecure?: boolean;
  is_default?: boolean;
}

export interface TestRelayResult {
  success: boolean;
  latency_ms: number | null;
  error: string | null;
}

export async function listRelays(): Promise<RelayServer[]> {
  return invoke<RelayServer[]>("list_relays");
}

export async function getRelay(id: string): Promise<RelayServer | null> {
  return invoke<RelayServer | null>("get_relay", { id });
}

export async function addRelay(request: CreateRelayRequest): Promise<RelayServer> {
  return invoke<RelayServer>("add_relay", { request });
}

export async function updateRelay(id: string, request: UpdateRelayRequest): Promise<RelayServer> {
  return invoke<RelayServer>("update_relay", { id, request });
}

export async function deleteRelay(id: string): Promise<void> {
  return invoke<void>("delete_relay", { id });
}

export async function testRelay(id: string): Promise<TestRelayResult> {
  return invoke<TestRelayResult>("test_relay", { id });
}
