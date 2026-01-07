import { useState, useEffect, useCallback, useMemo } from 'react';
import { handleApiMetrics, handleApiStats, handleApiTcpConnections, handleApiReplayById } from './api/generated/sdk.gen';
import type { HttpMetric, MetricsStats, TcpMetric, ReplayResponse, BodyData } from './api/generated/types.gen';

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

// Polling is used instead of SSE for real-time updates

// Filter types
type StatusFilter = 'all' | '2xx' | '3xx' | '4xx' | '5xx' | 'error';
type MethodFilter = 'all' | 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH' | 'HEAD' | 'OPTIONS';

const PAGE_SIZE_OPTIONS = [10, 20, 50, 100];
const DEFAULT_PAGE_SIZE = 20;

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
  // Removed SSE eventSourceRef - using polling instead

  // Filter state
  const [methodFilter, setMethodFilter] = useState<MethodFilter>('all');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');
  const [uriSearch, setUriSearch] = useState('');
  const [pageSize, setPageSize] = useState(DEFAULT_PAGE_SIZE);
  const [showFilters, setShowFilters] = useState(false);

  // Replay state
  const [replayLoading, setReplayLoading] = useState(false);
  const [replayResult, setReplayResult] = useState<ReplayResponse | null>(null);
  const [replayError, setReplayError] = useState<string | null>(null);

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

  // Polling for real-time updates (fetch last 20 metrics every second)
  useEffect(() => {
    const POLL_INTERVAL = 1000; // 1 second
    const POLL_LIMIT = 20;

    const pollMetrics = async () => {
      try {
        const [metricsRes, statsRes, tcpRes] = await Promise.all([
          handleApiMetrics({ query: { limit: POLL_LIMIT.toString(), offset: '0' } }),
          handleApiStats(),
          handleApiTcpConnections()
        ]);

        if (metricsRes.data) {
          setHttpMetrics(metricsRes.data);
        }
        if (statsRes.data) {
          setStats(statsRes.data);
        }
        if (tcpRes.data) {
          setTcpMetrics(tcpRes.data);
        }
        setConnected(true);
      } catch (error) {
        console.error('Polling failed:', error);
        setConnected(false);
      }
    };

    // Start polling
    const intervalId = setInterval(pollMetrics, POLL_INTERVAL);

    // Also poll immediately on mount
    pollMetrics();

    return () => {
      clearInterval(intervalId);
    };
  }, []);

  // Reset to page 1 when switching modes or filters change
  useEffect(() => {
    setCurrentPage(1);
    setSelectedItem(null);
  }, [viewMode]);

  // Reset page when filters change
  useEffect(() => {
    setCurrentPage(1);
  }, [methodFilter, statusFilter, uriSearch, pageSize]);

  // Clear filters function
  const clearFilters = useCallback(() => {
    setMethodFilter('all');
    setStatusFilter('all');
    setUriSearch('');
    setCurrentPage(1);
  }, []);

  // Check if any filters are active
  const hasActiveFilters = methodFilter !== 'all' || statusFilter !== 'all' || uriSearch !== '';

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

  const timeAgo = (timestamp: number | string) => {
    const now = Date.now();
    const time = typeof timestamp === 'string' ? new Date(timestamp).getTime() : timestamp;
    const diff = now - time;

    const seconds = Math.floor(diff / 1000);
    if (seconds < 60) return `${seconds}s ago`;

    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;

    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;

    const days = Math.floor(hours / 24);
    return `${days}d ago`;
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

  // Render body content based on type
  const renderBodyContent = useCallback((bodyData: BodyData | null | undefined) => {
    if (!bodyData) return null;

    const { data, content_type, size } = bodyData;

    if (data.type === 'Json') {
      return (
        <pre className="text-xs font-mono text-dark-text-primary whitespace-pre-wrap break-all">
          {JSON.stringify(data.value, null, 2)}
        </pre>
      );
    }

    if (data.type === 'Text') {
      return (
        <pre className="text-xs font-mono text-dark-text-primary whitespace-pre-wrap break-all">
          {data.value}
        </pre>
      );
    }

    if (data.type === 'Binary') {
      return (
        <div className="text-xs text-dark-text-muted">
          <span className="text-yellow-500">[Binary data]</span> {formatBytes(size)} ({content_type})
        </div>
      );
    }

    return null;
  }, [formatBytes]);

  // Replay a captured HTTP request by ID (backend has the full request data including body)
  const replayRequest = useCallback(async (metric: HttpMetric) => {
    setReplayLoading(true);
    setReplayResult(null);
    setReplayError(null);

    try {
      const result = await handleApiReplayById({
        path: { id: metric.id },
      });

      if (result.data) {
        setReplayResult(result.data);
      } else if (result.error) {
        setReplayError(typeof result.error === 'string' ? result.error : 'Replay failed');
      }
    } catch (error) {
      setReplayError(error instanceof Error ? error.message : 'Replay failed');
    } finally {
      setReplayLoading(false);
    }
  }, []);

  // Clear replay state when selection changes
  useEffect(() => {
    setReplayResult(null);
    setReplayError(null);
  }, [selectedItem]);

  // Filter HTTP metrics
  const filteredHttpMetrics = useMemo(() => {
    return httpMetrics.filter((metric) => {
      // Method filter
      if (methodFilter !== 'all' && metric.method !== methodFilter) {
        return false;
      }

      // Status filter
      if (statusFilter !== 'all') {
        const status = metric.response_status;
        if (statusFilter === 'error') {
          if (status && status >= 100 && status < 600) return false;
        } else if (statusFilter === '2xx') {
          if (!status || status < 200 || status >= 300) return false;
        } else if (statusFilter === '3xx') {
          if (!status || status < 300 || status >= 400) return false;
        } else if (statusFilter === '4xx') {
          if (!status || status < 400 || status >= 500) return false;
        } else if (statusFilter === '5xx') {
          if (!status || status < 500 || status >= 600) return false;
        }
      }

      // URI search filter
      if (uriSearch) {
        const searchLower = uriSearch.toLowerCase();
        if (!metric.uri.toLowerCase().includes(searchLower)) {
          return false;
        }
      }

      return true;
    });
  }, [httpMetrics, methodFilter, statusFilter, uriSearch]);

  // Pagination
  const currentItems = viewMode === 'http' ? filteredHttpMetrics : tcpMetrics;
  const totalItems = viewMode === 'http' ? httpMetrics.length : tcpMetrics.length;
  const filteredCount = currentItems.length;
  const totalPages = Math.ceil(filteredCount / pageSize);
  const startIndex = (currentPage - 1) * pageSize;
  const endIndex = startIndex + pageSize;
  const paginatedItems = currentItems.slice(startIndex, endIndex);

  const goToPage = (page: number) => {
    setCurrentPage(Math.max(1, Math.min(page, totalPages || 1)));
  };

  const tcpStats = viewMode === 'tcp' ? getTcpStats() : null;

  // Get protocol info for display
  const getProtocolInfo = (endpoint: TunnelEndpoint) => {
    if (endpoint.protocol.Tcp) return { type: 'TCP', port: endpoint.protocol.Tcp.port };
    if (endpoint.protocol.Http) return { type: 'HTTP', subdomain: endpoint.protocol.Http.subdomain };
    if (endpoint.protocol.Https) return { type: 'HTTPS', subdomain: endpoint.protocol.Https.subdomain };
    return { type: 'Unknown' };
  };

  // Determine if tunnel is TCP-based or HTTP-based
  const isTcpTunnel = tunnelInfo.length > 0 && tunnelInfo.some(e => e.protocol.Tcp);
  const isHttpTunnel = tunnelInfo.length > 0 && tunnelInfo.some(e => e.protocol.Http || e.protocol.Https);

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
                <span className="text-xs text-dark-text-muted ml-2">v{__APP_VERSION__}</span>
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
              <span className="text-xs text-dark-text-muted ml-2">v{__APP_VERSION__}</span>
            </div>
          )}
        </div>
      </header>

      {/* Main Content */}
      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-6">

        {/* Tabs - only show relevant tab based on tunnel protocol */}
        <div className="flex gap-2 mb-6 border-b border-dark-border">
          {(isHttpTunnel || (!isTcpTunnel && !isHttpTunnel)) && (
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
          )}
          {(isTcpTunnel || (!isTcpTunnel && !isHttpTunnel)) && (
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
          )}
        </div>

        {/* Stats Section - uses server-side computed stats from SSE */}
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
            <div className="p-4 border-b border-dark-border">
              <div className="flex items-center justify-between mb-3">
                <h2 className="text-lg font-semibold text-dark-text-primary">
                  {viewMode === 'http' ? 'HTTP Requests' : 'TCP Connections'}
                </h2>
                <div className="flex items-center gap-2">
                  {viewMode === 'http' && (
                    <button
                      onClick={() => setShowFilters(!showFilters)}
                      className={`px-3 py-1.5 text-xs font-medium rounded-lg transition-colors ${
                        showFilters || hasActiveFilters
                          ? 'bg-accent-blue text-white'
                          : 'bg-dark-surface-light text-dark-text-secondary hover:text-dark-text-primary border border-dark-border'
                      }`}
                    >
                      Filters {hasActiveFilters && `(${filteredCount !== totalItems ? filteredCount : ''})`}
                    </button>
                  )}
                  <span className="text-sm text-dark-text-secondary">
                    {hasActiveFilters && viewMode === 'http' ? `${filteredCount} of ${totalItems}` : `${filteredCount} total`}
                  </span>
                </div>
              </div>

              {/* Filter Bar - HTTP only */}
              {viewMode === 'http' && showFilters && (
                <div className="space-y-3 pt-3 border-t border-dark-border">
                  {/* URI Search */}
                  <div>
                    <input
                      type="text"
                      placeholder="Search URI path..."
                      value={uriSearch}
                      onChange={(e) => setUriSearch(e.target.value)}
                      className="w-full px-3 py-2 text-sm bg-dark-bg border border-dark-border rounded-lg text-dark-text-primary placeholder-dark-text-muted focus:outline-none focus:border-accent-blue transition-colors"
                    />
                  </div>

                  {/* Method Filter */}
                  <div className="flex flex-wrap gap-1.5">
                    <span className="text-xs text-dark-text-muted mr-1 self-center">Method:</span>
                    {(['all', 'GET', 'POST', 'PUT', 'DELETE', 'PATCH'] as MethodFilter[]).map((method) => (
                      <button
                        key={method}
                        onClick={() => setMethodFilter(method)}
                        className={`px-2.5 py-1 text-xs font-medium rounded transition-colors ${
                          methodFilter === method
                            ? 'bg-accent-blue text-white'
                            : 'bg-dark-surface-light text-dark-text-secondary hover:text-dark-text-primary border border-dark-border'
                        }`}
                      >
                        {method === 'all' ? 'All' : method}
                      </button>
                    ))}
                  </div>

                  {/* Status Filter */}
                  <div className="flex flex-wrap gap-1.5">
                    <span className="text-xs text-dark-text-muted mr-1 self-center">Status:</span>
                    {([
                      { value: 'all', label: 'All', color: '' },
                      { value: '2xx', label: '2xx', color: 'text-accent-green' },
                      { value: '3xx', label: '3xx', color: 'text-accent-blue' },
                      { value: '4xx', label: '4xx', color: 'text-yellow-500' },
                      { value: '5xx', label: '5xx', color: 'text-accent-red' },
                      { value: 'error', label: 'Error', color: 'text-accent-red' },
                    ] as { value: StatusFilter; label: string; color: string }[]).map(({ value, label, color }) => (
                      <button
                        key={value}
                        onClick={() => setStatusFilter(value)}
                        className={`px-2.5 py-1 text-xs font-medium rounded transition-colors ${
                          statusFilter === value
                            ? 'bg-accent-blue text-white'
                            : `bg-dark-surface-light ${color || 'text-dark-text-secondary'} hover:text-dark-text-primary border border-dark-border`
                        }`}
                      >
                        {label}
                      </button>
                    ))}
                  </div>

                  {/* Clear Filters */}
                  {hasActiveFilters && (
                    <div className="flex justify-end">
                      <button
                        onClick={clearFilters}
                        className="px-3 py-1 text-xs text-accent-red hover:text-accent-red-light transition-colors"
                      >
                        Clear all filters
                      </button>
                    </div>
                  )}
                </div>
              )}
            </div>
            <div className="divide-y divide-dark-border max-h-[600px] overflow-y-auto scrollbar-dark">
              {loading ? (
                <div className="p-8 text-center text-dark-text-secondary">Loading...</div>
              ) : paginatedItems.length === 0 ? (
                <div className="p-8 text-center">
                  <p className="text-dark-text-secondary">
                    {hasActiveFilters && viewMode === 'http'
                      ? 'No requests match the current filters'
                      : `No ${viewMode === 'http' ? 'HTTP requests' : 'TCP connections'} yet`}
                  </p>
                  {hasActiveFilters && viewMode === 'http' && (
                    <button
                      onClick={clearFilters}
                      className="mt-2 px-4 py-2 text-sm text-accent-blue hover:text-accent-blue-light transition-colors"
                    >
                      Clear filters
                    </button>
                  )}
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
                      {formatDuration(metric.duration_ms)} • {new Date(metric.timestamp).toLocaleTimeString()} ({timeAgo(metric.timestamp)})
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
                      {' • '}{new Date(metric.timestamp).toLocaleTimeString()} ({timeAgo(metric.timestamp)})
                    </div>
                  </button>
                ))
              )}
            </div>

            {/* Enhanced Pagination */}
            <div className="p-4 border-t border-dark-border">
              <div className="flex items-center justify-between flex-wrap gap-3">
                {/* Page Size Selector */}
                <div className="flex items-center gap-2">
                  <span className="text-xs text-dark-text-muted">Show:</span>
                  <select
                    value={pageSize}
                    onChange={(e) => setPageSize(Number(e.target.value))}
                    className="px-2 py-1 text-xs bg-dark-bg border border-dark-border rounded text-dark-text-primary focus:outline-none focus:border-accent-blue cursor-pointer"
                  >
                    {PAGE_SIZE_OPTIONS.map((size) => (
                      <option key={size} value={size}>
                        {size}
                      </option>
                    ))}
                  </select>
                  <span className="text-xs text-dark-text-muted">per page</span>
                </div>

                {/* Pagination Controls */}
                {totalPages > 0 && (
                  <div className="flex items-center gap-2">
                    {/* First Page */}
                    <button
                      onClick={() => goToPage(1)}
                      disabled={currentPage === 1}
                      className="px-2 py-1 text-xs bg-dark-surface-light border border-dark-border rounded text-dark-text-secondary hover:text-dark-text-primary disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                      title="First page"
                    >
                      &laquo;
                    </button>

                    {/* Previous */}
                    <button
                      onClick={() => goToPage(currentPage - 1)}
                      disabled={currentPage === 1}
                      className="px-3 py-1 text-xs bg-dark-surface-light border border-dark-border rounded text-dark-text-secondary hover:text-dark-text-primary disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                    >
                      Prev
                    </button>

                    {/* Page Info */}
                    <div className="flex items-center gap-1 px-2">
                      <span className="text-xs text-dark-text-muted">Page</span>
                      <input
                        type="number"
                        min={1}
                        max={totalPages}
                        value={currentPage}
                        onChange={(e) => {
                          const page = parseInt(e.target.value, 10);
                          if (!isNaN(page)) goToPage(page);
                        }}
                        className="w-12 px-2 py-1 text-xs text-center bg-dark-bg border border-dark-border rounded text-dark-text-primary focus:outline-none focus:border-accent-blue"
                      />
                      <span className="text-xs text-dark-text-muted">of {totalPages}</span>
                    </div>

                    {/* Next */}
                    <button
                      onClick={() => goToPage(currentPage + 1)}
                      disabled={currentPage === totalPages || totalPages === 0}
                      className="px-3 py-1 text-xs bg-dark-surface-light border border-dark-border rounded text-dark-text-secondary hover:text-dark-text-primary disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                    >
                      Next
                    </button>

                    {/* Last Page */}
                    <button
                      onClick={() => goToPage(totalPages)}
                      disabled={currentPage === totalPages || totalPages === 0}
                      className="px-2 py-1 text-xs bg-dark-surface-light border border-dark-border rounded text-dark-text-secondary hover:text-dark-text-primary disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                      title="Last page"
                    >
                      &raquo;
                    </button>
                  </div>
                )}

                {/* Items Info */}
                <div className="text-xs text-dark-text-muted">
                  {filteredCount > 0 ? (
                    <>
                      Showing {startIndex + 1}-{Math.min(endIndex, filteredCount)} of {filteredCount}
                      {hasActiveFilters && viewMode === 'http' && ` (filtered from ${totalItems})`}
                    </>
                  ) : (
                    'No items to display'
                  )}
                </div>
              </div>
            </div>
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
                  {/* Request info with Replay button */}
                  <div className="flex items-start justify-between">
                    <div>
                      <h3 className="text-sm font-medium text-dark-text-secondary">Request</h3>
                      <div className="mt-1 flex items-center gap-2">
                        <span className="font-mono text-sm font-semibold text-accent-blue">{selectedItem.method}</span>
                        <span className="text-sm text-dark-text-primary">{selectedItem.uri}</span>
                      </div>
                    </div>
                    <button
                      onClick={() => replayRequest(selectedItem)}
                      disabled={replayLoading}
                      className={`px-4 py-2 text-sm font-medium rounded-lg transition-colors flex items-center gap-2 ${
                        replayLoading
                          ? 'bg-dark-surface-light text-dark-text-muted cursor-not-allowed'
                          : 'bg-accent-purple hover:bg-accent-purple/80 text-white'
                      }`}
                    >
                      {replayLoading ? (
                        <>
                          <span className="inline-block w-4 h-4 border-2 border-dark-text-muted border-t-transparent rounded-full animate-spin"></span>
                          Replaying...
                        </>
                      ) : (
                        <>
                          <span>↻</span>
                          Replay
                        </>
                      )}
                    </button>
                  </div>

                  {/* Replay Result */}
                  {(replayResult || replayError) && (
                    <div className={`p-4 rounded-lg border ${
                      replayError
                        ? 'bg-accent-red/10 border-accent-red/30'
                        : replayResult?.status && replayResult.status >= 200 && replayResult.status < 300
                          ? 'bg-accent-green/10 border-accent-green/30'
                          : 'bg-yellow-500/10 border-yellow-500/30'
                    }`}>
                      <div className="flex items-center justify-between mb-2">
                        <h3 className="text-sm font-semibold text-dark-text-primary">Replay Result</h3>
                        {replayResult?.status && (
                          <span className={`px-2 py-0.5 text-xs font-medium rounded ${
                            replayResult.status >= 200 && replayResult.status < 300
                              ? 'bg-accent-green/20 text-accent-green'
                              : 'bg-yellow-500/20 text-yellow-500'
                          }`}>
                            {replayResult.status}
                          </span>
                        )}
                      </div>
                      {replayError ? (
                        <p className="text-sm text-accent-red">{replayError}</p>
                      ) : replayResult?.error ? (
                        <p className="text-sm text-accent-red">{replayResult.error}</p>
                      ) : replayResult?.body ? (
                        <div className="bg-dark-bg rounded p-2 max-h-40 overflow-y-auto scrollbar-dark">
                          <pre className="text-xs font-mono text-dark-text-primary whitespace-pre-wrap break-all">
                            {replayResult.body}
                          </pre>
                        </div>
                      ) : (
                        <p className="text-sm text-dark-text-muted">No response body</p>
                      )}
                    </div>
                  )}

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
                  {selectedItem.request_body && (
                    <div>
                      <h3 className="text-sm font-medium text-dark-text-secondary">
                        Request Body
                        <span className="ml-2 text-xs text-dark-text-muted font-normal">
                          ({formatBytes(selectedItem.request_body.size)})
                        </span>
                      </h3>
                      <div className="mt-1 bg-dark-surface-light rounded-lg p-3 max-h-60 overflow-y-auto scrollbar-dark border border-dark-border">
                        {renderBodyContent(selectedItem.request_body)}
                      </div>
                    </div>
                  )}
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
                  {selectedItem.response_body && (
                    <div>
                      <h3 className="text-sm font-medium text-dark-text-secondary">
                        Response Body
                        <span className="ml-2 text-xs text-dark-text-muted font-normal">
                          ({formatBytes(selectedItem.response_body.size)})
                        </span>
                      </h3>
                      <div className="mt-1 bg-dark-surface-light rounded-lg p-3 max-h-60 overflow-y-auto scrollbar-dark border border-dark-border">
                        {renderBodyContent(selectedItem.response_body)}
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
                    <p className="mt-1 text-sm text-dark-text-primary">
                      {new Date(selectedItem.timestamp).toLocaleString()} ({timeAgo(selectedItem.timestamp)})
                    </p>
                  </div>
                  {selectedItem.closed_at && (
                    <div>
                      <h3 className="text-sm font-medium text-dark-text-secondary">Closed At</h3>
                      <p className="mt-1 text-sm text-dark-text-primary">
                        {new Date(selectedItem.closed_at).toLocaleString()} ({timeAgo(selectedItem.closed_at)})
                      </p>
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
