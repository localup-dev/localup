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
    <div className="min-h-screen bg-dark-bg">
      {/* Header */}
      <header className="bg-dark-surface border-b border-dark-border">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-6">
          <div className="flex items-center justify-between flex-wrap gap-4">
            <div>
              <h1 className="text-3xl font-bold text-dark-text-primary">Tunnels</h1>
              <p className="text-dark-text-secondary mt-1">Manage and monitor your tunnels</p>
              {tunnelInfo.length > 0 && (
                <div className="mt-3 flex items-center gap-4 text-sm">
                  {tunnelInfo.map((endpoint, i) => (
                    <div key={i} className="flex items-center gap-2">
                      <span className="text-dark-text-secondary">🌐</span>
                      <span className="text-dark-text-primary font-medium">
                        {endpoint.protocol.Tcp && `localhost`}
                        {endpoint.protocol.Http && `localhost`}
                        {endpoint.protocol.Https && `localhost`}
                      </span>
                      <span className="text-dark-text-muted">🛡️</span>
                      <span className="text-dark-text-primary font-medium">
                        {endpoint.protocol.Tcp && `TCP`}
                        {endpoint.protocol.Http && `HTTP`}
                        {endpoint.protocol.Https && `HTTPS`}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>
            <button className="px-6 py-2.5 bg-accent-blue hover:bg-accent-blue-light text-white rounded-lg font-medium transition-all shadow-glow-blue">
              + New Tunnel
            </button>
          </div>
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Tunnel List Section */}
        <div className="mb-8">
          <div className="card-dark p-6">
            <div className="flex items-center justify-between mb-6">
              <h2 className="text-xl font-semibold text-dark-text-primary">All Tunnels</h2>
            </div>
            {tunnelInfo.length > 0 ? (
              <div className="space-y-3">
                {tunnelInfo.map((endpoint, i) => (
                  <div key={i} className="card-dark-hover p-4">
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-4">
                        <div className="w-10 h-10 bg-accent-blue/10 rounded-lg flex items-center justify-center">
                          <span className="text-accent-blue text-xl">📊</span>
                        </div>
                        <div>
                          <div className="flex items-center gap-2">
                            <h3 className="text-lg font-semibold text-dark-text-primary">Test</h3>
                            <span className="status-badge-green">● connected</span>
                          </div>
                          <div className="flex items-center gap-2 mt-1 text-sm text-dark-text-secondary">
                            <span>🌐 localhost</span>
                            <span>•</span>
                            <span>🛡️ {endpoint.protocol.Tcp && 'TCP'}{endpoint.protocol.Http && 'HTTP'}{endpoint.protocol.Https && 'HTTPS'}</span>
                          </div>
                        </div>
                      </div>
                      <div className="flex items-center gap-3">
                        <button className="p-2 hover:bg-dark-surface-light rounded-lg transition-colors">
                          <span className="text-dark-text-secondary">☐</span>
                        </button>
                        <button className="p-2 hover:bg-dark-surface-light rounded-lg transition-colors">
                          <span className="text-dark-text-secondary">🗑️</span>
                        </button>
                      </div>
                    </div>
                    <div className="mt-4 pt-4 border-t border-dark-border">
                      <div className="flex items-center gap-2 text-sm">
                        <span className="text-dark-text-secondary">Public Endpoints:</span>
                      </div>
                      <div className="flex items-center gap-2 mt-2">
                        <span className="text-accent-blue">🔗</span>
                        <code className="text-sm font-mono text-dark-text-primary bg-dark-surface-light px-3 py-1.5 rounded-lg border border-dark-border">
                          {endpoint.public_url}
                        </code>
                        <button className="text-accent-blue hover:text-accent-blue-light transition-colors">
                          <span className="text-sm">🔗</span>
                        </button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-center py-12 text-dark-text-secondary">
                No tunnels configured yet
              </div>
            )}
          </div>
        </div>

        {/* Tabs */}
        <div className="flex gap-2 mb-6 border-b border-dark-border">
          <button
            onClick={() => setViewMode('http')}
            className={`px-6 py-3 font-medium transition-colors relative ${
              viewMode === 'http'
                ? 'text-accent-blue'
                : 'text-dark-text-secondary hover:text-dark-text-primary'
            }`}
          >
            📊 Overview
            {viewMode === 'http' && (
              <div className="absolute bottom-0 left-0 right-0 h-0.5 bg-accent-blue"></div>
            )}
          </button>
          <button
            onClick={() => setViewMode('http')}
            className={`px-6 py-3 font-medium transition-colors relative ${
              viewMode === 'http'
                ? 'text-accent-blue'
                : 'text-dark-text-secondary hover:text-dark-text-primary'
            }`}
          >
            📈 Metrics
            {viewMode === 'http' && (
              <div className="absolute bottom-0 left-0 right-0 h-0.5 bg-accent-blue"></div>
            )}
          </button>
          <button
            onClick={() => setViewMode('tcp')}
            className={`px-6 py-3 font-medium transition-colors relative ${
              viewMode === 'tcp'
                ? 'text-accent-blue'
                : 'text-dark-text-secondary hover:text-dark-text-primary'
            }`}
          >
            📡 Traffic
            {viewMode === 'tcp' && (
              <div className="absolute bottom-0 left-0 right-0 h-0.5 bg-accent-blue"></div>
            )}
          </button>
        </div>

        {/* Stats Section */}
        {viewMode === 'http' && stats ? (
          <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-6">
            <div className="card-dark p-6">
              <div className="flex items-center gap-3 mb-3">
                <div className="w-12 h-12 bg-accent-blue/10 rounded-xl flex items-center justify-center">
                  <span className="text-2xl">📊</span>
                </div>
                <div>
                  <h3 className="text-sm font-medium text-dark-text-secondary">Total Requests</h3>
                  <p className="text-3xl font-bold text-dark-text-primary mt-1">{stats.total_requests}</p>
                </div>
              </div>
            </div>
            <div className="card-dark p-6">
              <div className="flex items-center gap-3 mb-3">
                <div className="w-12 h-12 bg-accent-green/10 rounded-xl flex items-center justify-center">
                  <span className="text-2xl">✓</span>
                </div>
                <div>
                  <h3 className="text-sm font-medium text-dark-text-secondary">Successful</h3>
                  <p className="text-3xl font-bold text-accent-green mt-1">{stats.successful_requests}</p>
                  <p className="text-xs text-dark-text-muted mt-1">
                    {stats.total_requests > 0 ? ((stats.successful_requests / stats.total_requests) * 100).toFixed(1) : '0.0'}% success rate
                  </p>
                </div>
              </div>
            </div>
            <div className="card-dark p-6">
              <div className="flex items-center gap-3 mb-3">
                <div className="w-12 h-12 bg-accent-red/10 rounded-xl flex items-center justify-center">
                  <span className="text-2xl">✗</span>
                </div>
                <div>
                  <h3 className="text-sm font-medium text-dark-text-secondary">Failed</h3>
                  <p className="text-3xl font-bold text-accent-red mt-1">{stats.failed_requests}</p>
                </div>
              </div>
            </div>
            <div className="card-dark p-6">
              <div className="flex items-center gap-3 mb-3">
                <div className="w-12 h-12 bg-accent-purple/10 rounded-xl flex items-center justify-center">
                  <span className="text-2xl">⏱️</span>
                </div>
                <div>
                  <h3 className="text-sm font-medium text-dark-text-secondary">Avg Duration</h3>
                  <p className="text-3xl font-bold text-dark-text-primary mt-1">
                    {stats.avg_duration_ms ? formatDuration(stats.avg_duration_ms) : 'N/A'}
                  </p>
                </div>
              </div>
            </div>
          </div>
        ) : viewMode === 'tcp' && tcpStats ? (
          <div className="grid grid-cols-1 md:grid-cols-5 gap-4 mb-6">
            <div className="card-dark p-6">
              <div className="flex items-center gap-3">
                <div className="w-12 h-12 bg-accent-blue/10 rounded-xl flex items-center justify-center">
                  <span className="text-2xl">🔌</span>
                </div>
                <div>
                  <h3 className="text-sm font-medium text-dark-text-secondary">Total Connections</h3>
                  <p className="text-3xl font-bold text-dark-text-primary mt-1">{tcpStats.total_connections}</p>
                </div>
              </div>
            </div>
            <div className="card-dark p-6">
              <div className="flex items-center gap-3">
                <div className="w-12 h-12 bg-accent-green/10 rounded-xl flex items-center justify-center">
                  <span className="text-2xl">●</span>
                </div>
                <div>
                  <h3 className="text-sm font-medium text-dark-text-secondary">Active</h3>
                  <p className="text-3xl font-bold text-accent-green mt-1">{tcpStats.active_connections}</p>
                </div>
              </div>
            </div>
            <div className="card-dark p-6">
              <div className="flex items-center gap-3">
                <div className="w-12 h-12 bg-dark-text-muted/10 rounded-xl flex items-center justify-center">
                  <span className="text-2xl">○</span>
                </div>
                <div>
                  <h3 className="text-sm font-medium text-dark-text-secondary">Closed</h3>
                  <p className="text-3xl font-bold text-dark-text-muted mt-1">{tcpStats.closed_connections}</p>
                </div>
              </div>
            </div>
            <div className="card-dark p-6">
              <div className="flex items-center gap-3">
                <div className="w-12 h-12 bg-accent-blue/10 rounded-xl flex items-center justify-center">
                  <span className="text-2xl">↑</span>
                </div>
                <div>
                  <h3 className="text-sm font-medium text-dark-text-secondary">Total Sent</h3>
                  <p className="text-3xl font-bold text-accent-blue mt-1">{formatBytes(tcpStats.total_bytes_sent)}</p>
                </div>
              </div>
            </div>
            <div className="card-dark p-6">
              <div className="flex items-center gap-3">
                <div className="w-12 h-12 bg-accent-purple/10 rounded-xl flex items-center justify-center">
                  <span className="text-2xl">↓</span>
                </div>
                <div>
                  <h3 className="text-sm font-medium text-dark-text-secondary">Total Received</h3>
                  <p className="text-3xl font-bold text-accent-purple mt-1">{formatBytes(tcpStats.total_bytes_received)}</p>
                </div>
              </div>
            </div>
          </div>
        ) : null}

        {/* Content Area */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* List Panel */}
          <div className="card-dark">
            <div className="p-4 border-b border-dark-border flex items-center justify-between">
              <h2 className="text-lg font-semibold text-dark-text-primary">
                {viewMode === 'http' ? 'HTTP Requests' : 'TCP Connections'}
              </h2>
              <span className="text-sm text-dark-text-secondary">
                {currentItems.length} total
              </span>
            </div>
            <div className="divide-y divide-dark-border max-h-[600px] overflow-y-auto scrollbar-dark">
              {loading ? (
                <div className="p-8 text-center text-dark-text-secondary">Loading...</div>
              ) : paginatedItems.length === 0 ? (
                <div className="p-8 text-center text-dark-text-secondary">
                  No {viewMode === 'http' ? 'HTTP requests' : 'TCP connections'} yet
                </div>
              ) : viewMode === 'http' ? (
                (paginatedItems as HttpMetric[]).map((metric) => (
                  <button
                    key={metric.id}
                    onClick={() => setSelectedItem(metric)}
                    className={`w-full p-4 hover:bg-dark-surface-light text-left transition ${
                      selectedItem && 'id' in selectedItem && selectedItem.id === metric.id ? 'bg-dark-surface-light border-l-2 border-accent-blue' : ''
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-3">
                        <span className="font-mono text-sm font-semibold text-accent-blue">
                          {metric.method}
                        </span>
                        <span className="text-sm text-dark-text-primary truncate">{metric.uri}</span>
                      </div>
                      <span
                        className={`text-sm font-medium ${
                          metric.response_status && metric.response_status >= 200 && metric.response_status < 300
                            ? 'text-accent-green'
                            : 'text-accent-red'
                        }`}
                      >
                        {metric.response_status || 'ERR'}
                      </span>
                    </div>
                    <div className="mt-1 text-xs text-dark-text-secondary">
                      {formatDuration(metric.duration_ms)} • {new Date(metric.timestamp).toLocaleTimeString()}
                    </div>
                  </button>
                ))
              ) : (
                (paginatedItems as TcpMetric[]).map((metric) => (
                  <button
                    key={metric.id}
                    onClick={() => setSelectedItem(metric)}
                    className={`w-full p-4 hover:bg-dark-surface-light text-left transition ${
                      selectedItem && 'id' in selectedItem && selectedItem.id === metric.id ? 'bg-dark-surface-light border-l-2 border-accent-blue' : ''
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <div>
                        <span className="text-sm font-medium text-dark-text-primary">{metric.remote_addr}</span>
                        <div className="mt-1 text-xs text-dark-text-secondary">
                          {metric.local_addr}
                        </div>
                      </div>
                      <span
                        className={`text-sm font-medium capitalize ${
                          metric.state === 'active' ? 'text-accent-green' : 'text-dark-text-muted'
                        }`}
                      >
                        {metric.state}
                      </span>
                    </div>
                    <div className="mt-1 text-xs text-dark-text-secondary">
                      ↓ {formatBytes(metric.bytes_received)} • ↑ {formatBytes(metric.bytes_sent)}
                      {metric.duration_ms && ` • ${formatDuration(metric.duration_ms)}`}
                    </div>
                  </button>
                ))
              )}
            </div>

            {/* Pagination */}
            {totalPages > 1 && (
              <div className="p-4 border-t border-dark-border flex items-center justify-between">
                <button
                  onClick={() => goToPage(currentPage - 1)}
                  disabled={currentPage === 1}
                  className="px-4 py-2 text-sm bg-dark-surface-light border border-dark-border rounded-lg text-dark-text-primary hover:bg-dark-surface disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  Previous
                </button>
                <span className="text-sm text-dark-text-secondary">
                  Page {currentPage} of {totalPages}
                </span>
                <button
                  onClick={() => goToPage(currentPage + 1)}
                  disabled={currentPage === totalPages}
                  className="px-4 py-2 text-sm bg-dark-surface-light border border-dark-border rounded-lg text-dark-text-primary hover:bg-dark-surface disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  Next
                </button>
              </div>
            )}
          </div>

          {/* Detail Panel */}
          <div className="card-dark">
            <div className="p-4 border-b border-dark-border">
              <h2 className="text-lg font-semibold text-dark-text-primary">Details</h2>
            </div>
            <div className="p-4 max-h-[700px] overflow-y-auto scrollbar-dark">
              {!selectedItem ? (
                <div className="text-center text-dark-text-secondary py-12">
                  Select an item to view details
                </div>
              ) : 'method' in selectedItem ? (
                // HTTP Request Details
                <div className="space-y-4">
                  <div>
                    <h3 className="text-sm font-medium text-dark-text-secondary">Request</h3>
                    <div className="mt-1 flex items-center gap-2">
                      <span className="font-mono text-sm font-semibold text-accent-blue">{selectedItem.method}</span>
                      <span className="text-sm text-dark-text-primary">{selectedItem.uri}</span>
                    </div>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-dark-text-secondary">Status</h3>
                    <p className={`mt-1 text-sm font-medium ${
                      selectedItem.response_status && selectedItem.response_status >= 200 && selectedItem.response_status < 300
                        ? 'text-accent-green'
                        : 'text-accent-red'
                    }`}>
                      {selectedItem.response_status || 'Error'}
                    </p>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-dark-text-secondary">Duration</h3>
                    <p className="mt-1 text-sm text-dark-text-primary">{formatDuration(selectedItem.duration_ms)}</p>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-dark-text-secondary">Request Headers</h3>
                    <div className="mt-1 bg-dark-surface-light rounded-lg p-3 text-xs font-mono max-h-40 overflow-y-auto scrollbar-dark border border-dark-border">
                      {selectedItem.request_headers.map(([key, value]: [string, string], i: number) => (
                        <div key={i} className="py-0.5">
                          <span className="text-accent-blue">{key}:</span> <span className="text-dark-text-primary">{value}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                  {selectedItem.response_headers && (
                    <div>
                      <h3 className="text-sm font-medium text-dark-text-secondary">Response Headers</h3>
                      <div className="mt-1 bg-dark-surface-light rounded-lg p-3 text-xs font-mono max-h-40 overflow-y-auto scrollbar-dark border border-dark-border">
                        {selectedItem.response_headers.map(([key, value]: [string, string], i: number) => (
                          <div key={i} className="py-0.5">
                            <span className="text-accent-green">{key}:</span> <span className="text-dark-text-primary">{value}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              ) : (
                // TCP Connection Details
                <div className="space-y-4">
                  <div>
                    <h3 className="text-sm font-medium text-dark-text-secondary">State</h3>
                    <p className={`mt-1 text-sm capitalize font-medium ${
                      selectedItem.state === 'active' ? 'text-accent-green' : 'text-dark-text-muted'
                    }`}>
                      {selectedItem.state}
                    </p>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-dark-text-secondary">Remote Address</h3>
                    <p className="mt-1 text-sm font-mono text-dark-text-primary bg-dark-surface-light px-3 py-2 rounded-lg border border-dark-border">
                      {selectedItem.remote_addr}
                    </p>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-dark-text-secondary">Local Address</h3>
                    <p className="mt-1 text-sm font-mono text-dark-text-primary bg-dark-surface-light px-3 py-2 rounded-lg border border-dark-border">
                      {selectedItem.local_addr}
                    </p>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-dark-text-secondary">Bytes Received</h3>
                    <p className="mt-1 text-sm text-accent-purple font-medium">{formatBytes(selectedItem.bytes_received)}</p>
                  </div>
                  <div>
                    <h3 className="text-sm font-medium text-dark-text-secondary">Bytes Sent</h3>
                    <p className="mt-1 text-sm text-accent-blue font-medium">{formatBytes(selectedItem.bytes_sent)}</p>
                  </div>
                  {selectedItem.duration_ms && (
                    <div>
                      <h3 className="text-sm font-medium text-dark-text-secondary">Duration</h3>
                      <p className="mt-1 text-sm text-dark-text-primary">{formatDuration(selectedItem.duration_ms)}</p>
                    </div>
                  )}
                  {selectedItem.error && (
                    <div>
                      <h3 className="text-sm font-medium text-dark-text-secondary">Error</h3>
                      <p className="mt-1 text-sm text-accent-red font-medium">{selectedItem.error}</p>
                    </div>
                  )}
                  <div>
                    <h3 className="text-sm font-medium text-dark-text-secondary">Timestamp</h3>
                    <p className="mt-1 text-sm text-dark-text-primary">{new Date(selectedItem.timestamp).toLocaleString()}</p>
                  </div>
                  {selectedItem.closed_at && (
                    <div>
                      <h3 className="text-sm font-medium text-dark-text-secondary">Closed At</h3>
                      <p className="mt-1 text-sm text-dark-text-primary">{new Date(selectedItem.closed_at).toLocaleString()}</p>
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
