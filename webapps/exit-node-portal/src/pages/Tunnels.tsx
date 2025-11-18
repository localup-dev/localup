import { useState, useEffect } from 'react';

interface Tunnel {
  id: string;
  endpoints: Array<{
    protocol: {
      type: string;
      subdomain?: string;
      port?: number;
      domain?: string;
    };
    public_url: string;
    port?: number;
  }>;
  status: string;
  region: string;
  connected_at: string;
  local_addr?: string;
}

interface Request {
  id: number;
  tunnel_id: string;
  method: string;
  path: string;
  status_code?: number;
  created_at: string;
  latency_ms?: number;
}

export default function Tunnels() {
  const [tunnels, setTunnels] = useState<Tunnel[]>([]);
  const [requests, setRequests] = useState<Request[]>([]);
  const [selectedTunnel, setSelectedTunnel] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchTunnels();
    const interval = setInterval(fetchTunnels, 5000);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    if (selectedTunnel) {
      fetchRequests(selectedTunnel);
      const interval = setInterval(() => fetchRequests(selectedTunnel), 3000);
      return () => clearInterval(interval);
    }
  }, [selectedTunnel]);

  const fetchTunnels = async () => {
    try {
      const response = await fetch('/api/tunnels');
      if (!response.ok) throw new Error('Failed to fetch tunnels');
      const data = await response.json();
      setTunnels(data.tunnels || []);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  };

  const fetchRequests = async (tunnelId: string) => {
    try {
      const response = await fetch(`/api/requests?tunnel_id=${tunnelId}&limit=50`);
      if (!response.ok) throw new Error('Failed to fetch requests');
      const data = await response.json();
      setRequests(data.requests || []);
    } catch (err) {
      console.error('Error fetching requests:', err);
    }
  };

  const formatDate = (dateString: string) => {
    return new Date(dateString).toLocaleString();
  };

  const getStatusColor = (status: string) => {
    switch (status.toLowerCase()) {
      case 'connected':
        return 'text-green-600 bg-green-50';
      case 'disconnected':
        return 'text-red-600 bg-red-50';
      case 'connecting':
        return 'text-yellow-600 bg-yellow-50';
      default:
        return 'text-gray-600 bg-gray-50';
    }
  };

  const getStatusCodeColor = (code?: number) => {
    if (!code) return 'text-gray-600';
    if (code >= 200 && code < 300) return 'text-green-600';
    if (code >= 300 && code < 400) return 'text-blue-600';
    if (code >= 400 && code < 500) return 'text-yellow-600';
    return 'text-red-600';
  };

  if (loading) {
    return (
      <div className="min-h-screen bg-gray-900 flex items-center justify-center">
        <div className="text-gray-400">Loading tunnels...</div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gray-900">
      {/* Header */}
      <div className="border-b border-gray-800">
        <div className="max-w-7xl mx-auto px-6 py-6">
          <h1 className="text-3xl font-bold text-white">Active Tunnels</h1>
          <p className="text-gray-400 mt-2">Monitor and manage your running tunnels</p>
        </div>
      </div>

      {/* Main Content */}
      <div className="max-w-7xl mx-auto px-6 py-8">
        {error && (
          <div className="mb-4 bg-red-900/50 border border-red-500 text-red-200 px-4 py-3 rounded">
            {error}
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Tunnels List */}
          <div className="lg:col-span-1">
            <div className="bg-gray-800 rounded-lg shadow">
              <div className="px-4 py-5 border-b border-gray-700">
                <h2 className="text-lg font-medium text-white">
                  Active Tunnels ({tunnels.length})
                </h2>
              </div>
              <div className="divide-y divide-gray-700">
                {tunnels.length === 0 ? (
                  <div className="px-4 py-8 text-center text-gray-500">No active tunnels</div>
                ) : (
                  tunnels.map((tunnel) => (
                    <div
                      key={tunnel.id}
                      className={`px-4 py-4 cursor-pointer hover:bg-gray-700 transition ${
                        selectedTunnel === tunnel.id ? 'bg-gray-700' : ''
                      }`}
                      onClick={() => setSelectedTunnel(tunnel.id)}
                    >
                      <div className="flex items-center justify-between mb-2">
                        <span className="font-medium text-sm text-white truncate">
                          {tunnel.id}
                        </span>
                      </div>
                      <div
                        className={`inline-block px-2 py-1 text-xs rounded ${getStatusColor(
                          tunnel.status
                        )}`}
                      >
                        {tunnel.status}
                      </div>
                      <div className="mt-2 text-xs text-gray-400">
                        {tunnel.endpoints.map((endpoint, i) => (
                          <div key={i} className="truncate">
                            {endpoint.public_url}
                          </div>
                        ))}
                      </div>
                      <div className="mt-1 text-xs text-gray-500">
                        Connected: {formatDate(tunnel.connected_at)}
                      </div>
                    </div>
                  ))
                )}
              </div>
            </div>
          </div>

          {/* Traffic Details */}
          <div className="lg:col-span-2">
            {selectedTunnel ? (
              <div className="bg-gray-800 rounded-lg shadow">
                <div className="px-4 py-5 border-b border-gray-700">
                  <h2 className="text-lg font-medium text-white mb-4">
                    Traffic for {selectedTunnel}
                  </h2>
                </div>

                <div className="divide-y divide-gray-700 max-h-96 overflow-y-auto">
                  {requests.length === 0 ? (
                    <div className="px-4 py-8 text-center text-gray-500">
                      No HTTP requests captured
                    </div>
                  ) : (
                    requests.map((request) => (
                      <div key={request.id} className="px-4 py-3 hover:bg-gray-700">
                        <div className="flex items-center justify-between mb-1">
                          <div className="flex items-center gap-2">
                            <span className="text-xs font-medium text-white">
                              {request.method}
                            </span>
                            <span className="text-sm text-gray-300">{request.path}</span>
                          </div>
                          <div className="flex items-center gap-3">
                            {request.status_code && (
                              <span
                                className={`text-xs font-medium ${getStatusCodeColor(
                                  request.status_code
                                )}`}
                              >
                                {request.status_code}
                              </span>
                            )}
                            {request.latency_ms && (
                              <span className="text-xs text-gray-500">
                                {request.latency_ms}ms
                              </span>
                            )}
                          </div>
                        </div>
                        <div className="text-xs text-gray-500">
                          {formatDate(request.created_at)}
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </div>
            ) : (
              <div className="bg-gray-800 rounded-lg shadow px-4 py-12 text-center text-gray-500">
                Select a tunnel to view traffic details
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
