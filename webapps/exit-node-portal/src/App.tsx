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

interface TcpConnection {
  id: number;
  tunnel_id: string;
  created_at: string;
  bytes_sent: number;
  bytes_received: number;
}

function App() {
  const [tunnels, setTunnels] = useState<Tunnel[]>([]);
  const [requests, setRequests] = useState<Request[]>([]);
  const [tcpConnections, setTcpConnections] = useState<TcpConnection[]>([]);
  const [selectedTunnel, setSelectedTunnel] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<'http' | 'tcp'>('http');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchTunnels();
    const interval = setInterval(fetchTunnels, 5000);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    if (selectedTunnel) {
      if (activeTab === 'http') {
        fetchRequests(selectedTunnel);
        const interval = setInterval(() => fetchRequests(selectedTunnel), 3000);
        return () => clearInterval(interval);
      } else {
        fetchTcpConnections(selectedTunnel);
        const interval = setInterval(() => fetchTcpConnections(selectedTunnel), 3000);
        return () => clearInterval(interval);
      }
    }
  }, [selectedTunnel, activeTab]);

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

  const fetchTcpConnections = async (tunnelId: string) => {
    try {
      const response = await fetch(`/api/tcp-connections?tunnel_id=${tunnelId}&limit=50`);
      if (!response.ok) throw new Error('Failed to fetch TCP connections');
      const data = await response.json();
      setTcpConnections(data.connections || []);
    } catch (err) {
      console.error('Error fetching TCP connections:', err);
    }
  };

  const deleteTunnel = async (tunnelId: string) => {

    try {
      const response = await fetch(`/api/tunnels/${tunnelId}`, {
        method: 'DELETE',
      });
      if (!response.ok) throw new Error('Failed to delete tunnel');
      await fetchTunnels();
      if (selectedTunnel === tunnelId) {
        setSelectedTunnel(null);
        setRequests([]);
        setTcpConnections([]);
      }
    } catch (err) {
      alert(err instanceof Error ? err.message : 'Failed to delete tunnel');
    }
  };

  const formatDate = (dateString: string) => {
    return new Date(dateString).toLocaleString();
  };

  const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
  };

  const getStatusColor = (status: string) => {
    switch (status.toLowerCase()) {
      case 'connected': return 'text-green-600 bg-green-50';
      case 'disconnected': return 'text-red-600 bg-red-50';
      case 'connecting': return 'text-yellow-600 bg-yellow-50';
      default: return 'text-gray-600 bg-gray-50';
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
      <div className="min-h-screen bg-gray-50 flex items-center justify-center">
        <div className="text-gray-600">Loading tunnels...</div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gray-50">
      <header className="bg-white border-b border-gray-200">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-4">
          <h1 className="text-2xl font-bold text-gray-900">Tunnel Exit Node Portal</h1>
          <p className="text-sm text-gray-600 mt-1">Monitor and manage active tunnels</p>
        </div>
      </header>

      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {error && (
          <div className="mb-4 bg-red-50 border border-red-200 text-red-700 px-4 py-3 rounded">
            {error}
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Tunnels List */}
          <div className="lg:col-span-1">
            <div className="bg-white rounded-lg shadow">
              <div className="px-4 py-5 border-b border-gray-200">
                <h2 className="text-lg font-medium text-gray-900">
                  Active Tunnels ({tunnels.length})
                </h2>
              </div>
              <div className="divide-y divide-gray-200">
                {tunnels.length === 0 ? (
                  <div className="px-4 py-8 text-center text-gray-500">
                    No active tunnels
                  </div>
                ) : (
                  tunnels.map((tunnel) => (
                    <div
                      key={tunnel.id}
                      className={`px-4 py-4 cursor-pointer hover:bg-gray-50 transition ${
                        selectedTunnel === tunnel.id ? 'bg-blue-50' : ''
                      }`}
                      onClick={() => setSelectedTunnel(tunnel.id)}
                    >
                      <div className="flex items-center justify-between mb-2">
                        <span className="font-medium text-sm text-gray-900 truncate">
                          {tunnel.id}
                        </span>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            deleteTunnel(tunnel.id);
                          }}
                          className="text-red-600 hover:text-red-800 text-xs"
                        >
                          Delete
                        </button>
                      </div>
                      <div className={`inline-block px-2 py-1 text-xs rounded ${getStatusColor(tunnel.status)}`}>
                        {tunnel.status}
                      </div>
                      <div className="mt-2 text-xs text-gray-600">
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
              <div className="bg-white rounded-lg shadow">
                <div className="px-4 py-5 border-b border-gray-200">
                  <h2 className="text-lg font-medium text-gray-900 mb-4">
                    Traffic for {selectedTunnel}
                  </h2>
                  <div className="flex gap-2">
                    <button
                      onClick={() => setActiveTab('http')}
                      className={`px-4 py-2 text-sm font-medium rounded ${
                        activeTab === 'http'
                          ? 'bg-blue-600 text-white'
                          : 'bg-gray-100 text-gray-700 hover:bg-gray-200'
                      }`}
                    >
                      HTTP Requests
                    </button>
                    <button
                      onClick={() => setActiveTab('tcp')}
                      className={`px-4 py-2 text-sm font-medium rounded ${
                        activeTab === 'tcp'
                          ? 'bg-blue-600 text-white'
                          : 'bg-gray-100 text-gray-700 hover:bg-gray-200'
                      }`}
                    >
                      TCP Connections
                    </button>
                  </div>
                </div>

                {activeTab === 'http' ? (
                  <div className="divide-y divide-gray-200 max-h-96 overflow-y-auto">
                    {requests.length === 0 ? (
                      <div className="px-4 py-8 text-center text-gray-500">
                        No HTTP requests captured
                      </div>
                    ) : (
                      requests.map((request) => (
                        <div key={request.id} className="px-4 py-3 hover:bg-gray-50">
                          <div className="flex items-center justify-between mb-1">
                            <div className="flex items-center gap-2">
                              <span className="text-xs font-medium text-gray-900">
                                {request.method}
                              </span>
                              <span className="text-sm text-gray-700">{request.path}</span>
                            </div>
                            <div className="flex items-center gap-3">
                              {request.status_code && (
                                <span className={`text-xs font-medium ${getStatusCodeColor(request.status_code)}`}>
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
                ) : (
                  <div className="divide-y divide-gray-200 max-h-96 overflow-y-auto">
                    {tcpConnections.length === 0 ? (
                      <div className="px-4 py-8 text-center text-gray-500">
                        No TCP connections captured
                      </div>
                    ) : (
                      tcpConnections.map((conn) => (
                        <div key={conn.id} className="px-4 py-3 hover:bg-gray-50">
                          <div className="flex items-center justify-between mb-1">
                            <span className="text-sm text-gray-700">Connection #{conn.id}</span>
                            <div className="flex items-center gap-3 text-xs text-gray-600">
                              <span>⬆ {formatBytes(conn.bytes_sent)}</span>
                              <span>⬇ {formatBytes(conn.bytes_received)}</span>
                            </div>
                          </div>
                          <div className="text-xs text-gray-500">
                            {formatDate(conn.created_at)}
                          </div>
                        </div>
                      ))
                    )}
                  </div>
                )}
              </div>
            ) : (
              <div className="bg-white rounded-lg shadow px-4 py-12 text-center text-gray-500">
                Select a tunnel to view traffic details
              </div>
            )}
          </div>
        </div>
      </main>
    </div>
  );
}

export default App;
