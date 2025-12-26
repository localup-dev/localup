import { useState, useEffect, useCallback, useRef } from 'react';
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

// SSE Event types from the backend
type MetricsEvent =
  | { type: 'request'; metric: HttpMetric }
  | { type: 'response'; id: string; status: number; headers: [string, string][]; body: unknown; duration_ms: number }
  | { type: 'error'; id: string; error: string; duration_ms: number }
  | { type: 'tcp_connection'; metric: TcpMetric }
  | { type: 'tcp_update'; id: string; bytes_received: number; bytes_sent: number }
  | { type: 'tcp_closed'; id: string; bytes_received: number; bytes_sent: number; duration_ms: number; error?: string }
  | { type: 'stats'; stats: MetricsStats };

const ITEMS_PER_PAGE = 20;

function App() {
  const [viewMode, setViewMode] = useState<ViewMode | null>(null);
  const [httpMetrics, setHttpMetrics] = useState<HttpMetric[]>([]);
  const [tcpMetrics, setTcpMetrics] = useState<TcpMetric[]>([]);
  const [stats, setStats] = useState<MetricsStats | null>(null);
  const [selectedItem, setSelectedItem] = useState<HttpMetric | TcpMetric | null>(null);
  const [loading, setLoading] = useState(true);
  const [tunnelInfo, setTunnelInfo] = useState<TunnelEndpoint[]>([]);
  const [currentPage, setCurrentPage] = useState(1);
  const [connected, setConnected] = useState(false);
  const eventSourceRef = useRef<EventSource | null>(null);

  // Initial data fetch
  useEffect(() => {
    const fetchInitialData = async () => {
      try {
        setLoading(true);

        // Fetch tunnel info
        const infoRes = await fetch('/api/info');
        if (infoRes.ok) {
          const info = await infoRes.json();
          setTunnelInfo(info);

          // Auto-detect initial view mode based on tunnel protocol
          if (info.length > 0) {
            const firstEndpoint = info[0];
            if (firstEndpoint.protocol.Tcp) {
              setViewMode('tcp');
            } else if (firstEndpoint.protocol.Http || firstEndpoint.protocol.Https) {
              setViewMode('http');
            }
          }
        }

        // Fetch initial metrics data
        const [metricsRes, statsRes, tcpRes] = await Promise.all([
          handleApiMetrics(),
          handleApiStats(),
          handleApiTcpConnections()
        ]);
        if (metricsRes.data) setHttpMetrics(metricsRes.data);
        if (statsRes.data) setStats(statsRes.data);
        if (tcpRes.data) setTcpMetrics(tcpRes.data);
      } catch (error) {
        console.error('Failed to fetch initial data:', error);
      } finally {
        setLoading(false);
      }
    };

    fetchInitialData();
  }, []);

  // SSE connection for real-time updates
  useEffect(() => {
    const connectSSE = () => {
      const eventSource = new EventSource('/api/metrics/stream');
      eventSourceRef.current = eventSource;

      eventSource.onopen = () => {
        setConnected(true);
        console.log('SSE connected');
      };

      eventSource.onmessage = (event) => {
        try {
          const data: MetricsEvent = JSON.parse(event.data);

          switch (data.type) {
            case 'request':
              setHttpMetrics(prev => [data.metric, ...prev]);
              break;

            case 'response':
              setHttpMetrics(prev =>
                prev.map(m =>
                  m.id === data.id
                    ? {
                        ...m,
                        response_status: data.status,
                        response_headers: data.headers,
                        response_body: data.body as HttpMetric['response_body'],
                        duration_ms: data.duration_ms,
                      }
                    : m
                )
              );
              break;

            case 'error':
              setHttpMetrics(prev =>
                prev.map(m =>
                  m.id === data.id
                    ? { ...m, error: data.error, duration_ms: data.duration_ms }
                    : m
                )
              );
              break;

            case 'tcp_connection':
              setTcpMetrics(prev => [data.metric, ...prev]);
              break;

            case 'tcp_update':
              setTcpMetrics(prev =>
                prev.map(m =>
                  m.id === data.id
                    ? { ...m, bytes_received: data.bytes_received, bytes_sent: data.bytes_sent }
                    : m
                )
              );
              break;

            case 'tcp_closed':
              setTcpMetrics(prev =>
                prev.map(m =>
                  m.id === data.id
                    ? {
                        ...m,
                        state: data.error ? 'error' : 'closed',
                        bytes_received: data.bytes_received,
                        bytes_sent: data.bytes_sent,
                        duration_ms: data.duration_ms,
                        error: data.error,
                        closed_at: Date.now(),
                      }
                    : m
                )
              );
              break;

            case 'stats':
              setStats(data.stats);
              break;
          }
        } catch (error) {
          console.error('Failed to parse SSE event:', error);
        }
      };

      eventSource.onerror = () => {
        setConnected(false);
        eventSource.close();
        // Reconnect after 2 seconds
        setTimeout(connectSSE, 2000);
      };
    };

    connectSSE();

    return () => {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
      }
    };
  }, []);

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

  const copyToClipboard = useCallback((text: string) => {
    navigator.clipboard.writeText(text);
  }, []);

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

  // Get protocol info for display
  const getProtocolInfo = (endpoint: TunnelEndpoint) => {
    if (endpoint.protocol.Tcp) return { type: 'TCP', port: endpoint.protocol.Tcp.port };
    if (endpoint.protocol.Http) return { type: 'HTTP', subdomain: endpoint.protocol.Http.subdomain };
    if (endpoint.protocol.Https) return { type: 'HTTPS', subdomain: endpoint.protocol.Https.subdomain };
    return { type: 'Unknown' };
  };

  return (
    <div className="min-h-screen bg-dark-bg">
      {/* Compact Header with Tunnel Info */}
      <header className="bg-dark-surface border-b border-dark-border">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-4">
          {tunnelInfo.length > 0 ? (
            <div className="flex items-center gap-4 flex-wrap">
              {/* Status indicator */}
              <div className="flex items-center gap-2">
                <span className={`w-2 h-2 rounded-full ${connected ? 'bg-accent-green animate-pulse' : 'bg-yellow-500'}`}></span>
                <span className={`font-medium text-sm ${connected ? 'text-accent-green' : 'text-yellow-500'}`}>
                  {connected ? 'Live' : 'Reconnecting...'}
                </span>
              </div>

              {/* Protocol badge */}
              {tunnelInfo.map((endpoint, i) => {
                const protocolInfo = getProtocolInfo(endpoint);
                return (
                  <div key={i} className="flex items-center gap-3 flex-1">
                    <span className="px-2 py-1 bg-accent-blue/10 text-accent-blue text-xs font-medium rounded">
                      {protocolInfo.type}
                    </span>
                    <span className="text-xs text-dark-text-muted">:{endpoint.port}</span>

                    {/* URL with actions */}
                    <div className="flex items-center gap-2 flex-1">
                      <code className="flex-1 text-sm font-mono text-dark-text-primary bg-dark-bg px-3 py-1.5 rounded border border-dark-border truncate">
                        {endpoint.public_url}
                      </code>
                      <button
                        onClick={() => copyToClipboard(endpoint.public_url)}
                        className="px-2 py-1.5 bg-dark-bg hover:bg-dark-border text-dark-text-secondary hover:text-dark-text-primary rounded text-xs transition-colors"
                        title="Copy URL"
                      >
                        Copy
                      </button>
                      <a
                        href={endpoint.public_url}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="px-2 py-1.5 bg-accent-blue hover:bg-accent-blue-light text-white rounded text-xs transition-colors"
                      >
                        Open
                      </a>
                    </div>
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="flex items-center gap-2">
              <span className="w-2 h-2 bg-dark-text-muted rounded-full"></span>
              <span className="text-dark-text-muted font-medium text-sm">Connecting...</span>
            </div>
          )}
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-6">

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
            HTTP Traffic
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
            TCP Connections
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
