// React Query hooks for tunnel management

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import type { TunnelInfo, TunnelConfig, MetricsStats, HttpMetric, TcpMetric } from '../types/tunnel';

// Query keys
export const tunnelKeys = {
  all: ['tunnels'] as const,
  lists: () => [...tunnelKeys.all, 'list'] as const,
  list: () => [...tunnelKeys.lists()] as const,
  details: () => [...tunnelKeys.all, 'detail'] as const,
  detail: (id: string) => [...tunnelKeys.details(), id] as const,
  metrics: (id: string) => [...tunnelKeys.detail(id), 'metrics'] as const,
  requests: (id: string, offset: number, limit: number) =>
    [...tunnelKeys.detail(id), 'requests', offset, limit] as const,
  tcpConnections: (id: string, offset: number, limit: number) =>
    [...tunnelKeys.detail(id), 'tcp', offset, limit] as const,
};

// Fetch all tunnels
export function useTunnels() {
  return useQuery({
    queryKey: tunnelKeys.list(),
    queryFn: async () => {
      const tunnels = await invoke<TunnelInfo[]>('list_tunnels');
      return tunnels;
    },
    refetchInterval: 2000, // Refresh every 2 seconds
  });
}

// Fetch single tunnel
export function useTunnel(tunnelId: string | undefined) {
  return useQuery({
    queryKey: tunnelKeys.detail(tunnelId || ''),
    queryFn: async () => {
      if (!tunnelId) return null;
      const tunnel = await invoke<TunnelInfo | null>('get_tunnel', { tunnelId });
      return tunnel;
    },
    enabled: !!tunnelId,
    refetchInterval: 2000,
  });
}

// Fetch tunnel metrics
export function useTunnelMetrics(tunnelId: string | undefined) {
  return useQuery({
    queryKey: tunnelKeys.metrics(tunnelId || ''),
    queryFn: async () => {
      if (!tunnelId) return null;
      const metrics = await invoke<MetricsStats | null>('get_tunnel_metrics', { tunnelId });
      return metrics;
    },
    enabled: !!tunnelId,
    refetchInterval: 1000, // Refresh every second for real-time feel
  });
}

// Fetch HTTP requests
export function useTunnelRequests(tunnelId: string | undefined, offset: number = 0, limit: number = 50) {
  return useQuery({
    queryKey: tunnelKeys.requests(tunnelId || '', offset, limit),
    queryFn: async () => {
      if (!tunnelId) return [];
      const requests = await invoke<HttpMetric[]>('get_tunnel_requests', {
        tunnelId,
        offset,
        limit,
      });
      return requests;
    },
    enabled: !!tunnelId,
    refetchInterval: 1000,
  });
}

// Fetch TCP connections
export function useTunnelTcpConnections(
  tunnelId: string | undefined,
  offset: number = 0,
  limit: number = 50
) {
  return useQuery({
    queryKey: tunnelKeys.tcpConnections(tunnelId || '', offset, limit),
    queryFn: async () => {
      if (!tunnelId) return [];
      const connections = await invoke<TcpMetric[]>('get_tunnel_tcp_connections', {
        tunnelId,
        offset,
        limit,
      });
      return connections;
    },
    enabled: !!tunnelId,
    refetchInterval: 1000,
  });
}

// Create tunnel mutation
export function useCreateTunnel() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (config: TunnelConfig) => {
      // Add a timeout wrapper
      const timeoutMs = 60000; // 60 seconds
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error('Tunnel creation timed out after 60 seconds. Please check your relay server and try again.')), timeoutMs);
      });

      const createPromise = invoke<TunnelInfo>('create_tunnel', { config });
      const tunnel = await Promise.race([createPromise, timeoutPromise]);
      return tunnel;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: tunnelKeys.lists() });
    },
  });
}

// Stop tunnel mutation
export function useStopTunnel() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (tunnelId: string) => {
      console.log('🛑 [useTunnels] Stopping tunnel:', tunnelId);
      const startTime = performance.now();

      try {
        await invoke('stop_tunnel', { tunnelId });
        const stopTime = performance.now() - startTime;
        console.log(`✅ [useTunnels] Tunnel stopped successfully (${stopTime.toFixed(2)}ms)`);
      } catch (error) {
        const stopTime = performance.now() - startTime;
        console.error(`❌ [useTunnels] Failed to stop tunnel (${stopTime.toFixed(2)}ms):`, error);
        throw error;
      }
    },
    onSuccess: (_, tunnelId) => {
      console.log('🔄 [useTunnels] Invalidating queries after stopping:', tunnelId);
      queryClient.invalidateQueries({ queryKey: tunnelKeys.lists() });
    },
    onError: (error, tunnelId) => {
      console.error('❌ [useTunnels] Stop tunnel mutation error:', tunnelId, error);
    },
  });
}

// Clear metrics mutation
export function useClearMetrics() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (tunnelId: string) => {
      await invoke('clear_tunnel_metrics', { tunnelId });
    },
    onSuccess: (_, tunnelId) => {
      queryClient.invalidateQueries({ queryKey: tunnelKeys.metrics(tunnelId) });
      queryClient.invalidateQueries({ queryKey: tunnelKeys.requests(tunnelId, 0, 50) });
      queryClient.invalidateQueries({ queryKey: tunnelKeys.tcpConnections(tunnelId, 0, 50) });
    },
  });
}

// Stop all tunnels mutation
export function useStopAllTunnels() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      await invoke('stop_all_tunnels');
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: tunnelKeys.lists() });
    },
  });
}
