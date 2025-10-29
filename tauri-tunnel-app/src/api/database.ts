import { invoke } from '@tauri-apps/api/core';

// Types matching the Rust models
export interface Relay {
  id: string;
  name: string;
  description: string | null;
  address: string;
  region: string;
  is_default: boolean;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface Tunnel {
  id: string;
  name: string;
  description: string | null;
  local_host: string;
  auth_token: string;
  exit_node_config: string;
  failover: boolean;
  connection_timeout: number;
  status: string;
  last_connected_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface Protocol {
  id: string;
  tunnel_id: string;
  protocol_type: string;
  local_port: number;
  remote_port: number | null;
  subdomain: string | null;
  custom_domain: string | null;
  created_at: string;
}

// Relay operations
export const relayApi = {
  create: async (params: {
    name: string;
    address: string;
    region: string;
    description?: string;
  }) => {
    console.log('📡 [API] db_create_relay called with:', params);
    const startTime = performance.now();
    try {
      const result = await invoke<Relay>('db_create_relay', {
        name: params.name,
        address: params.address,
        region: params.region,
        description: params.description || null,
      });
      const time = performance.now() - startTime;
      console.log(`✅ [API] db_create_relay succeeded (${time.toFixed(2)}ms):`, result);
      return result;
    } catch (error) {
      const time = performance.now() - startTime;
      console.error(`❌ [API] db_create_relay failed (${time.toFixed(2)}ms):`, error);
      throw error;
    }
  },

  get: async (id: string) => {
    console.log(`📡 [API] db_get_relay called with id: ${id}`);
    const startTime = performance.now();
    try {
      const result = await invoke<Relay | null>('db_get_relay', { id });
      const time = performance.now() - startTime;
      console.log(`✅ [API] db_get_relay succeeded (${time.toFixed(2)}ms):`, result);
      return result;
    } catch (error) {
      const time = performance.now() - startTime;
      console.error(`❌ [API] db_get_relay failed (${time.toFixed(2)}ms):`, error);
      throw error;
    }
  },

  list: async () => {
    console.log('📡 [API] db_list_relays called');
    const startTime = performance.now();
    try {
      const result = await invoke<Relay[]>('db_list_relays');
      const time = performance.now() - startTime;
      console.log(`✅ [API] db_list_relays succeeded (${time.toFixed(2)}ms), found ${result.length} relay(s)`);
      return result;
    } catch (error) {
      const time = performance.now() - startTime;
      console.error(`❌ [API] db_list_relays failed (${time.toFixed(2)}ms):`, error);
      throw error;
    }
  },

  update: async (params: {
    id: string;
    name?: string;
    address?: string;
    region?: string;
    description?: string;
    status?: string;
  }) => {
    console.log('📡 [API] db_update_relay called with:', params);
    const startTime = performance.now();
    try {
      const result = await invoke<Relay>('db_update_relay', {
        id: params.id,
        name: params.name || null,
        address: params.address || null,
        region: params.region || null,
        description: params.description || null,
        status: params.status || null,
      });
      const time = performance.now() - startTime;
      console.log(`✅ [API] db_update_relay succeeded (${time.toFixed(2)}ms):`, result);
      return result;
    } catch (error) {
      const time = performance.now() - startTime;
      console.error(`❌ [API] db_update_relay failed (${time.toFixed(2)}ms):`, error);
      throw error;
    }
  },

  delete: async (id: string) => {
    console.log(`📡 [API] db_delete_relay called with id: ${id}`);
    const startTime = performance.now();
    try {
      const result = await invoke<void>('db_delete_relay', { id });
      const time = performance.now() - startTime;
      console.log(`✅ [API] db_delete_relay succeeded (${time.toFixed(2)}ms)`);
      return result;
    } catch (error) {
      const time = performance.now() - startTime;
      console.error(`❌ [API] db_delete_relay failed (${time.toFixed(2)}ms):`, error);
      throw error;
    }
  },
};

// Tunnel operations
export const tunnelApi = {
  create: (params: {
    name: string;
    localHost: string;
    authToken: string;
    exitNodeConfig: string;
    description?: string;
    failover?: boolean;
    connectionTimeout?: number;
  }) =>
    invoke<Tunnel>('db_create_tunnel', {
      name: params.name,
      local_host: params.localHost,
      auth_token: params.authToken,
      exit_node_config: params.exitNodeConfig,
      description: params.description || null,
      failover: params.failover ?? true,
      connection_timeout: params.connectionTimeout ?? 30000,
    }),

  get: (id: string) => invoke<Tunnel | null>('db_get_tunnel', { id }),

  list: () => invoke<Tunnel[]>('db_list_tunnels'),

  updateStatus: (params: {
    id: string;
    status: string;
    lastConnectedAt?: string;
  }) =>
    invoke<Tunnel>('db_update_tunnel_status', {
      id: params.id,
      status: params.status,
      last_connected_at: params.lastConnectedAt || null,
    }),

  delete: (id: string) => invoke<void>('db_delete_tunnel', { id }),
};

// Protocol operations
export const protocolApi = {
  create: (params: {
    tunnelId: string;
    protocolType: string;
    localPort: number;
    remotePort?: number;
    subdomain?: string;
    customDomain?: string;
  }) =>
    invoke<Protocol>('db_create_protocol', {
      tunnel_id: params.tunnelId,
      protocol_type: params.protocolType,
      local_port: params.localPort,
      remote_port: params.remotePort || null,
      subdomain: params.subdomain || null,
      custom_domain: params.customDomain || null,
    }),

  listForTunnel: (tunnelId: string) =>
    invoke<Protocol[]>('db_list_protocols_for_tunnel', {
      tunnel_id: tunnelId,
    }),

  delete: (id: string) => invoke<void>('db_delete_protocol', { id }),
};
