import { useState, useEffect } from 'react';
import { handleApiMetrics, handleApiStats, handleApiTcpConnections } from './api/generated/sdk.gen';
import type { HttpMetric, MetricsStats, TcpMetric } from './api/generated/types.gen';

type ViewMode = 'http' | 'tcp';

interface TunnelEndpoint {
  protocol: { Tcp?: { port: number }; Http?: { subdomain: string }; Https?: { subdomain: string } };
  public_url: string;
  port: number;
}

interface TcpStats {
  total_connections: number;
  active_connections: number;
  closed_connections: number;
  total_bytes_sent: number;
  total_bytes_received: number;
}

const ITEMS_PER_PAGE = 20;

function App() {
  const [viewMode, setViewMode] = useState<ViewMode | null>(null); // null until we detect protocol
  const [httpMetrics, setHttpMetrics] = useState<HttpMetric[]>([]);
  const [tcpMetrics, setTcpMetrics] = useState<TcpMetric[]>([]);
  const [stats, setStats] = useState<MetricsStats | null>(null);
  const [selectedItem, setSelectedItem] = useState<HttpMetric | TcpMetric | null>(null);
  const [loading, setLoading] = useState(true);
  const [tunnelInfo, setTunnelInfo] = useState<TunnelEndpoint[]>([]);
  const [currentPage, setCurrentPage] = useState(1);

  useEffect(() => {
    const fetchData = async () => {
      try {
        setLoading(true);

        // Fetch tunnel info
        const infoRes = await fetch('/api/info');
        if (infoRes.ok) {
          const info = await infoRes.json();
          setTunnelInfo(info);

          // Auto-detect initial view mode based on tunnel protocol
          if (viewMode === null && info.length > 0) {
            const firstEndpoint = info[0];
            if (firstEndpoint.protocol.Tcp) {
              setViewMode('tcp');
            } else if (firstEndpoint.protocol.Http || firstEndpoint.protocol.Https) {
              setViewMode('http');
            }
          }
        }

        // Only fetch metrics if viewMode is set
        if (viewMode === 'http') {
          const [metricsRes, statsRes] = await Promise.all([
            handleApiMetrics(),
            handleApiStats()
          ]);
          if (metricsRes.data) setHttpMetrics(metricsRes.data);
          if (statsRes.data) setStats(statsRes.data);
        } else if (viewMode === 'tcp') {
          const tcpRes = await handleApiTcpConnections();
          if (tcpRes.data) setTcpMetrics(tcpRes.data);
        }
      } catch (error) {
        console.error('Failed to fetch data:', error);
      } finally {
        setLoading(false);
      }
    };

    fetchData();
    const interval = setInterval(fetchData, 2000);
    return () => clearInterval(interval);
  }, [viewMode]);

  // Reset to page 1 when switching modes
  useEffect(() => {
    setCurrentPage(1);
    setSelectedItem(null);
  }, [viewMode]);

  const formatBytes = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  };

  const formatDuration = (ms?: number | null) => {
    if (!ms) return 'N/A';
    if (ms < 1000) return `${ms.toFixed(0)}ms`;
    return `${(ms / 1000).toFixed(2)}s`;
  };

  const getTcpStats = (): TcpStats => {
    const active = tcpMetrics.filter(m => m.state === 'active').length;
    const closed = tcpMetrics.filter(m => m.state === 'closed').length;
    const totalSent = tcpMetrics.reduce((sum, m) => sum + m.bytes_sent, 0);
    const totalReceived = tcpMetrics.reduce((sum, m) => sum + m.bytes_received, 0);

    return {
      total_connections: tcpMetrics.length,
      active_connections: active,
      closed_connections: closed,
      total_bytes_sent: totalSent,
      total_bytes_received: totalReceived,
    };
  };

  // Pagination
  const currentItems = viewMode === 'http' ? httpMetrics : tcpMetrics;
  const totalPages = Math.ceil(currentItems.length / ITEMS_PER_PAGE);
  const startIndex = (currentPage - 1) * ITEMS_PER_PAGE;
  const endIndex = startIndex + ITEMS_PER_PAGE;
  const paginatedItems = currentItems.slice(startIndex, endIndex);

  const goToPage = (page: number) => {
    setCurrentPage(Math.max(1, Math.min(page, totalPages)));
  };

  const tcpStats = viewMode === 'tcp' ? getTcpStats() : null;

  return (
    <div className="min-h-screen bg-gray-50">
      {/* Header */}
      <header className="bg-white shadow">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-4">
          <div className="flex items-center justify-between flex-wrap gap-4">
            <div>
              <h1 className="text-2xl font-bold text-gray-900">Tunnel Metrics Dashboard</h1>
              {tunnelInfo.length > 0 && (
                <div className="mt-1 flex items-center gap-4 text-sm text-gray-600">
                  {tunnelInfo.map((endpoint, i) => (
                    <div key={i} className="flex items-center gap-2">
                      <span className="font-medium">
                        {endpoint.protocol.Tcp && `TCP:${endpoint.protocol.Tcp.port}`}
                        {endpoint.protocol.Http && `HTTP`}
                        {endpoint.protocol.Https && `HTTPS`}
                      </span>
                      <span className="text-gray-400">→</span>
                      <code className="bg-blue-50 text-blue-700 px-2 py-0.5 rounded font-mono text-xs">
                        {endpoint.public_url}
                      </code>
                    </div>
                  ))}
                </div>
              )}
            </div>
            <div className="flex gap-2 bg-gray-100 p-1 rounded-lg">
              <button
                onClick={() => setViewMode('http')}
                className={`px-4 py-2 rounded-md font-medium transition ${
                  viewMode === 'http'
                    ? 'bg-white text-blue-600 shadow'
                    : 'text-gray-600 hover:text-gray-900'
                }`}
              >
                HTTP Requests
              </button>
              <button
                onClick={() => setViewMode('tcp')}
                className={`px-4 py-2 rounded-md font-medium transition ${
                  viewMode === 'tcp'
                    ? 'bg-white text-blue-600 shadow'
                    : 'text-gray-600 hover:text-gray-900'
                }`}
              >
                TCP Connections
              </button>
            </div>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Stats Section */}
        {viewMode === 'http' && stats ? (
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-6">
            <div className="bg-white rounded-lg shadow p-4">
              <h3 className="text-sm font-medium text-gray-500">Total Requests</h3>
              <p className="text-2xl font-bold text-gray-900 mt-1">{stats.total_requests}</p>
            </div>
            <div className="bg-white rounded-lg shadow p-4">
              <h3 className="text-sm font-medium text-gray-500">Successful</h3>
              <p className="text-2xl font-bold text-green-600 mt-1">{stats.successful_requests}</p>
            </div>
            <div className="bg-white rounded-lg shadow p-4">
              <h3 className="text-sm font-medium text-gray-500">Failed</h3>
              <p className="text-2xl font-bold text-red-600 mt-1">{stats.failed_requests}</p>
            </div>
            <div className="bg-white rounded-lg shadow p-4">
              <h3 className="text-sm font-medium text-gray-500">Avg Duration</h3>
              <p className="text-2xl font-bold text-gray-900 mt-1">
                {stats.avg_duration_ms ? formatDuration(stats.avg_duration_ms) : 'N/A'}
              </p>
            </div>
          </div>
        ) : viewMode === 'tcp' && tcpStats ? (
          <div className="grid grid-cols-1 md:grid-cols-5 gap-4 mb-6">
            <div className="bg-white rounded-lg shadow p-4">
              <h3 className="text-sm font-medium text-gray-500">Total Connections</h3>
              <p className="text-2xl font-bold text-gray-900 mt-1">{tcpStats.total_connections}</p>
            </div>
            <div className="bg-white rounded-lg shadow p-4">
              <h3 className="text-sm font-medium text-gray-500">Active</h3>
              <p className="text-2xl font-bold text-green-600 mt-1">{tcpStats.active_connections}</p>
            </div>
            <div className="bg-white rounded-lg shadow p-4">
              <h3 className="text-sm font-medium text-gray-500">Closed</h3>
              <p className="text-2xl font-bold text-gray-600 mt-1">{tcpStats.closed_connections}</p>
            </div>
            <div className="bg-white rounded-lg shadow p-4">
              <h3 className="text-sm font-medium text-gray-500">Total Sent</h3>
              <p className="text-2xl font-bold text-blue-600 mt-1">{formatBytes(tcpStats.total_bytes_sent)}</p>
            </div>
            <div className="bg-white rounded-lg shadow p-4">
              <h3 className="text-sm font-medium text-gray-500">Total Received</h3>
              <p className="text-2xl font-bold text-purple-600 mt-1">{formatBytes(tcpStats.total_bytes_received)}</p>
            </div>
          </div>
        ) : null}

        {/* Content Area */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* List Panel */}
          <div className="bg-white rounded-lg shadow">
            <div className="p-4 border-b border-gray-200 flex items-center justify-between">
              <h2 className="text-lg font-semibold">
                {viewMode === 'http' ? 'HTTP Requests' : 'TCP Connections'}
              </h2>
              <span className="text-sm text-gray-500">
                {currentItems.length} total
              </span>
            </div>
            <div className="divide-y divide-gray-200 max-h-[600px] overflow-y-auto">
              {loading ? (
                <div className="p-8 text-center text-gray-500">Loading...</div>
              ) : paginatedItems.length === 0 ? (
                <div className="p-8 text-center text-gray-500">
                  No {viewMode === 'http' ? 'HTTP requests' : 'TCP connections'} yet
                </div>
              ) : viewMode === 'http' ? (
                (paginatedItems as HttpMetric[]).map((metric) => (
                  <button
                    key={metric.id}
                    onClick={() => setSelectedItem(metric)}
                    className={`w-full p-4 hover:bg-gray-50 text-left transition ${
                      selectedItem && 'id' in selectedItem && selectedItem.id === metric.id ? 'bg-blue-50' : ''
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-3">
                        <span className="font-mono text-sm font-semibold text-blue-600">
                          {metric.method}
                        </span>
                        <span className="text-sm text-gray-900 truncate">{metric.uri}</span>
                      </div>
                      <span
                        className={`text-sm font-medium ${
                          metric.response_status && metric.response_status >= 200 && metric.response_status < 300
                            ? 'text-green-600'
                            : 'text-red-600'
                        }`}
                      >
                        {metric.response_status || 'ERR'}
                      </span>
                    </div>
                    <div className="mt-1 text-xs text-gray-500">
                      {formatDuration(metric.duration_ms)} • {new Date(metric.timestamp).toLocaleTimeString()}
                    </div>
                  </button>
                ))
              ) : (
                (paginatedItems as TcpMetric[]).map((metric) => (
                  <button
                    key={metric.id}
                    onClick={() => setSelectedItem(metric)}
                    className={`w-full p-4 hover:bg-gray-50 text-left transition ${
                      selectedItem && 'id' in selectedItem && selectedItem.id === metric.id ? 'bg-blue-50' : ''
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <div>
                        <span className="text-sm font-medium text-gray-900">{metric.remote_addr}</span>
                        <div className="mt-1 text-xs text-gray-500">
                          {metric.local_addr}
                        </div>
                      </div>
                      <span
                        className={`text-sm font-medium capitalize ${
                          metric.state === 'active' ? 'text-green-600' : 'text-gray-600'
                        }`}
                      >
                        {metric.state}
                      </span>
                    </div>
                    <div className="mt-1 text-xs text-gray-500">
                      ↓ {formatBytes(metric.bytes_received)} • ↑ {formatBytes(metric.bytes_sent)}
                      {metric.duration_ms && ` • ${formatDuration(metric.duration_ms)}`}
                    </div>
                  </button>
                ))
              )}
            </div>

            {/* Pagination */}
            {totalPages > 1 && (
              <div className="p-4 border-t border-gray-200 flex items-center justify-between">
                <button
                  onClick={() => goToPage(currentPage - 1)}
                  disabled={currentPage === 1}
                  className="px-3 py-1 text-sm border rounded hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Previous
                </button>
                <span className="text-sm text-gray-600">
                  Page {currentPage} of {totalPages}
                </span>
                <button
                  onClick={() => goToPage(currentPage + 1)}
                  disabled={currentPage === totalPages}
                  className="px-3 py-1 text-sm border rounded hover:bg-gray-50 disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Next
                </button>
              </div>
            )}
          </div>

          {/* Detail Panel */}
          <div className="bg-white rounded-lg shadow">
            <div className="p-4 border-b border-gray-200">
              <h2 className="text-lg font-semibold">Details</h2>
            </div>
            <div className="p-4 max-h-[700px] overflow-y-auto">
              {!selectedItem ? (
                <div className="text-center text-gray-500 py-12">
                  Select an item to view details
                </div>
              ) : 'method' in selectedItem ? (
                // HTTP Request Details
                <div className="space-y-4">
                  <div>
                    <h3 className="text-sm font-medium text-gray-500">Request</h3>
                    <div className="mt-1 flex items-center gap-2">
                      <span className="font-mono text-sm font-semibold">{selectedItem.method}</span>
                      <span className="text-sm">{selectedItem.uri}</span>
                    </div>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-gray-500">Status</h3>
                    <p className="mt-1 text-sm">{selectedItem.response_status || 'Error'}</p>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-gray-500">Duration</h3>
                    <p className="mt-1 text-sm">{formatDuration(selectedItem.duration_ms)}</p>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-gray-500">Request Headers</h3>
                    <div className="mt-1 bg-gray-50 rounded p-2 text-xs font-mono max-h-40 overflow-y-auto">
                      {selectedItem.request_headers.map(([key, value]: [string, string], i: number) => (
                        <div key={i}><span className="text-gray-600">{key}:</span> {value}</div>
                      ))}
                    </div>
                  </div>
                  {selectedItem.response_headers && (
                    <div>
                      <h3 className="text-sm font-medium text-gray-500">Response Headers</h3>
                      <div className="mt-1 bg-gray-50 rounded p-2 text-xs font-mono max-h-40 overflow-y-auto">
                        {selectedItem.response_headers.map(([key, value]: [string, string], i: number) => (
                          <div key={i}><span className="text-gray-600">{key}:</span> {value}</div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              ) : (
                // TCP Connection Details
                <div className="space-y-4">
                  <div>
                    <h3 className="text-sm font-medium text-gray-500">State</h3>
                    <p className="mt-1 text-sm capitalize">{selectedItem.state}</p>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-gray-500">Remote Address</h3>
                    <p className="mt-1 text-sm font-mono">{selectedItem.remote_addr}</p>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-gray-500">Local Address</h3>
                    <p className="mt-1 text-sm font-mono">{selectedItem.local_addr}</p>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-gray-500">Bytes Received</h3>
                    <p className="mt-1 text-sm">{formatBytes(selectedItem.bytes_received)}</p>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-gray-500">Bytes Sent</h3>
                    <p className="mt-1 text-sm">{formatBytes(selectedItem.bytes_sent)}</p>
                  </div>
                  {selectedItem.duration_ms && (
                    <div>
                      <h3 className="text-sm font-medium text-gray-500">Duration</h3>
                      <p className="mt-1 text-sm">{formatDuration(selectedItem.duration_ms)}</p>
                    </div>
                  )}
                  {selectedItem.error && (
                    <div>
                      <h3 className="text-sm font-medium text-gray-500">Error</h3>
                      <p className="mt-1 text-sm text-red-600">{selectedItem.error}</p>
                    </div>
                  )}
                  <div>
                    <h3 className="text-sm font-medium text-gray-500">Timestamp</h3>
                    <p className="mt-1 text-sm">{new Date(selectedItem.timestamp).toLocaleString()}</p>
                  </div>
                  {selectedItem.closed_at && (
                    <div>
                      <h3 className="text-sm font-medium text-gray-500">Closed At</h3>
                      <p className="mt-1 text-sm">{new Date(selectedItem.closed_at).toLocaleString()}</p>
                    </div>
                  )}
                </div>
              )}
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}

export default App;
