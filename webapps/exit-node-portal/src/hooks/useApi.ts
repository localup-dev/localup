import { useQuery } from '@tanstack/react-query';
import {
  listTunnelsOptions,
  listRequestsOptions,
  listTcpConnectionsOptions,
} from '../api/client/@tanstack/react-query.gen';

/**
 * Hook to fetch list of tunnels
 * @param includeInactive - Include inactive/disconnected tunnels from history
 */
export const useTunnels = (includeInactive = false) => {
  return useQuery(
    listTunnelsOptions({
      query: {
        include_inactive: includeInactive,
      },
    })
  );
};

/**
 * Hook to fetch HTTP requests for a specific tunnel
 */
export const useTunnelRequests = (tunnelId: string | null) => {
  return useQuery({
    ...listRequestsOptions({
      query: {
        localup_id: tunnelId || undefined,
        limit: 50,
      },
    }),
    enabled: !!tunnelId,
  });
};

/**
 * Hook to fetch TCP connections for a specific tunnel
 */
export const useTunnelTcpConnections = (tunnelId: string | null) => {
  return useQuery({
    ...listTcpConnectionsOptions({
      query: {
        localup_id: tunnelId || undefined,
        limit: 100,
      },
    }),
    enabled: !!tunnelId,
  });
};
