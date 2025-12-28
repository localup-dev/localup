import { useState, useCallback, useEffect } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import {
  getTunnelOptions,
  listRequestsOptions,
  listTcpConnectionsOptions,
} from '../api/client/@tanstack/react-query.gen';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../components/ui/card';
import { Badge } from '../components/ui/badge';
import { Skeleton } from '../components/ui/skeleton';

// Filter types
type StatusFilter = 'all' | '2xx' | '3xx' | '4xx' | '5xx';
type MethodFilter = 'all' | 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH' | 'HEAD' | 'OPTIONS';
type ViewMode = 'list' | 'list-detail';

const PAGE_SIZE_OPTIONS = [10, 20, 50, 100];
const DEFAULT_PAGE_SIZE = 20;
const DEBOUNCE_MS = 500;

// Custom hook for debounced value
function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value);

  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedValue(value);
    }, delay);

    return () => {
      clearTimeout(timer);
    };
  }, [value, delay]);

  return debouncedValue;
}

const getStatusVariant = (status: string): 'default' | 'secondary' | 'destructive' | 'outline' | 'success' => {
  switch (status.toLowerCase()) {
    case 'connected':
      return 'success';
    case 'disconnected':
      return 'destructive';
    case 'connecting':
      return 'secondary';
    default:
      return 'outline';
  }
};

const formatDate = (dateString: string) => {
  return new Date(dateString).toLocaleString();
};

const formatRelativeTime = (dateString: string) => {
  const date = new Date(dateString);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMins / 60);
  const diffDays = Math.floor(diffHours / 24);

  if (diffMins < 1) return 'just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  return `${diffDays}d ago`;
};

const getStatusCodeBadgeVariant = (code?: number): 'default' | 'secondary' | 'destructive' | 'outline' | 'success' => {
  if (!code) return 'outline';
  if (code >= 200 && code < 300) return 'success';
  if (code >= 300 && code < 400) return 'secondary';
  if (code >= 400 && code < 500) return 'secondary';
  return 'destructive';
};

const formatBytes = (bytes: number) => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
};

// Decode base64 to string, returning preview text
const decodeBase64Preview = (base64: string, maxLength: number = 200): string => {
  try {
    const decoded = atob(base64);
    // Check if it's printable text
    const isPrintable = /^[\x20-\x7E\s]*$/.test(decoded.substring(0, 100));
    if (isPrintable) {
      const preview = decoded.substring(0, maxLength);
      return preview + (decoded.length > maxLength ? '...' : '');
    }
    return '[Binary data]';
  } catch {
    // If decoding fails, try showing as-is (might already be decoded)
    return base64.substring(0, maxLength) + (base64.length > maxLength ? '...' : '');
  }
};

const getProtocolBadgeColor = (type: string) => {
  switch (type.toLowerCase()) {
    case 'tcp':
      return 'bg-chart-1/20 text-chart-1 border-chart-1/50';
    case 'http':
      return 'bg-chart-4/20 text-chart-4 border-chart-4/50';
    case 'https':
      return 'bg-chart-2/20 text-chart-2 border-chart-2/50';
    default:
      return 'bg-muted text-muted-foreground border-border';
  }
};

export default function TunnelDetail() {
  const { tunnelId } = useParams<{ tunnelId: string }>();
  const navigate = useNavigate();

  // View mode state
  const [viewMode, setViewMode] = useState<ViewMode>('list-detail');

  // Filter and pagination state
  const [methodFilter, setMethodFilter] = useState<MethodFilter>('all');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');
  const [pathSearch, setPathSearch] = useState('');
  const [pageSize, setPageSize] = useState(DEFAULT_PAGE_SIZE);
  const [currentPage, setCurrentPage] = useState(1);
  const [showFilters, setShowFilters] = useState(false);

  // TCP filter state
  const [clientAddrSearch, setClientAddrSearch] = useState('');
  const [tcpCurrentPage, setTcpCurrentPage] = useState(1);
  const [tcpPageSize, setTcpPageSize] = useState(DEFAULT_PAGE_SIZE);

  // Selected item for detail view (keeps filter/page context)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [selectedRequest, setSelectedRequest] = useState<any | null>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [selectedTcpConnection, setSelectedTcpConnection] = useState<any | null>(null);

  // Debounced search values (only triggers API call after user stops typing)
  const debouncedPathSearch = useDebounce(pathSearch, DEBOUNCE_MS);
  const debouncedClientAddrSearch = useDebounce(clientAddrSearch, DEBOUNCE_MS);

  // Fetch tunnel by ID
  const { data: tunnel, isLoading, error } = useQuery({
    ...getTunnelOptions({
      path: {
        id: tunnelId!,
      },
    }),
    enabled: !!tunnelId,
  });

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const isTcp = tunnel?.endpoints.some((e: any) => e.protocol.type === 'tcp');

  // Build query params for HTTP requests
  const getStatusRange = useCallback(() => {
    switch (statusFilter) {
      case '2xx': return { status_min: 200, status_max: 299 };
      case '3xx': return { status_min: 300, status_max: 399 };
      case '4xx': return { status_min: 400, status_max: 499 };
      case '5xx': return { status_min: 500, status_max: 599 };
      default: return {};
    }
  }, [statusFilter]);

  // Fetch requests with filters and pagination (uses debounced path search)
  const { data: requestsData, isLoading: requestsLoading } = useQuery({
    ...listRequestsOptions({
      query: {
        localup_id: tunnelId || undefined,
        method: methodFilter !== 'all' ? methodFilter : undefined,
        path: debouncedPathSearch || undefined,
        ...getStatusRange(),
        offset: (currentPage - 1) * pageSize,
        limit: pageSize,
      },
    }),
    enabled: !!tunnelId && !isTcp,
    refetchInterval: 5000, // Poll every 5 seconds for real-time feel
  });
  const requests = requestsData?.requests || [];
  const totalRequests = requestsData?.total || 0;
  const totalPages = Math.ceil(totalRequests / pageSize);

  // Fetch TCP connections with filters and pagination (uses debounced client addr search)
  const { data: tcpConnectionsData, isLoading: tcpLoading } = useQuery({
    ...listTcpConnectionsOptions({
      query: {
        localup_id: tunnelId || undefined,
        client_addr: debouncedClientAddrSearch || undefined,
        offset: (tcpCurrentPage - 1) * tcpPageSize,
        limit: tcpPageSize,
      },
    }),
    enabled: !!tunnelId && !!isTcp,
    refetchInterval: 5000, // Poll every 5 seconds for real-time feel
  });
  const tcpConnections = tcpConnectionsData?.connections || [];
  const totalTcpConnections = tcpConnectionsData?.total || 0;
  const totalTcpPages = Math.ceil(totalTcpConnections / tcpPageSize);

  // Check if filters are active
  const hasActiveFilters = methodFilter !== 'all' || statusFilter !== 'all' || pathSearch !== '';
  const hasTcpFilters = clientAddrSearch !== '';

  // Check if search is pending (user is typing)
  const isSearchPending = pathSearch !== debouncedPathSearch;
  const isTcpSearchPending = clientAddrSearch !== debouncedClientAddrSearch;

  // Reset page when debounced search value changes
  useEffect(() => {
    setCurrentPage(1);
  }, [debouncedPathSearch]);

  useEffect(() => {
    setTcpCurrentPage(1);
  }, [debouncedClientAddrSearch]);

  // Clear filters
  const clearFilters = useCallback(() => {
    setMethodFilter('all');
    setStatusFilter('all');
    setPathSearch('');
    setCurrentPage(1);
  }, []);

  const clearTcpFilters = useCallback(() => {
    setClientAddrSearch('');
    setTcpCurrentPage(1);
  }, []);

  // Reset page when filters change
  const handleMethodChange = (method: MethodFilter) => {
    setMethodFilter(method);
    setCurrentPage(1);
  };

  const handleStatusChange = (status: StatusFilter) => {
    setStatusFilter(status);
    setCurrentPage(1);
  };

  const handlePathSearchChange = (value: string) => {
    setPathSearch(value);
    // Page reset is handled by useEffect when debouncedPathSearch changes
  };

  const handlePageSizeChange = (size: number) => {
    setPageSize(size);
    setCurrentPage(1);
  };

  const handleTcpPageSizeChange = (size: number) => {
    setTcpPageSize(size);
    setTcpCurrentPage(1);
  };

  if (isLoading) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="text-muted-foreground">Loading tunnel details...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="text-destructive">Error loading tunnel: {error.message}</div>
      </div>
    );
  }

  if (!tunnel) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="text-center space-y-4">
          <div className="text-6xl opacity-20">🔍</div>
          <div className="text-xl text-muted-foreground">Tunnel not found</div>
          <button
            onClick={() => navigate('/tunnels')}
            className="px-4 py-2 bg-primary text-primary-foreground rounded hover:bg-primary/90 transition"
          >
            Back to Tunnels
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-background">
      {/* Compact Header with Endpoints */}
      <div className="border-b bg-card/50 backdrop-blur-sm sticky top-0 z-10">
        <div className="max-w-7xl mx-auto px-4 py-3">
          {/* Top row: Back button, ID, Status */}
          <div className="flex items-center gap-3 mb-2">
            <button
              onClick={() => navigate('/tunnels')}
              className="text-muted-foreground hover:text-foreground transition"
              aria-label="Back to tunnels"
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
              </svg>
            </button>
            <h1 className="text-lg font-semibold text-foreground font-mono truncate flex-1">{tunnel.id}</h1>
            <Badge variant={getStatusVariant(tunnel.status)} className="shrink-0">
              {tunnel.status}
            </Badge>
          </div>

          {/* Bottom row: Connection time + Endpoints */}
          <div className="flex items-center gap-4 flex-wrap">
            <span
              className="text-xs text-muted-foreground cursor-default"
              title={formatDate(tunnel.connected_at)}
            >
              {formatRelativeTime(tunnel.connected_at)}
            </span>
            <div className="h-3 w-px bg-border" />
            {/* eslint-disable-next-line @typescript-eslint/no-explicit-any */}
            {tunnel.endpoints.map((endpoint: any, i: number) => (
              <div key={i} className="flex items-center gap-2">
                <Badge
                  variant="outline"
                  className={`${getProtocolBadgeColor(endpoint.protocol.type)} text-[10px] px-1.5 py-0`}
                >
                  {endpoint.protocol.type.toUpperCase()}
                </Badge>
                <a
                  href={endpoint.public_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-xs text-primary hover:text-primary/80 font-mono"
                >
                  {endpoint.public_url}
                </a>
                <button
                  onClick={() => navigator.clipboard.writeText(endpoint.public_url)}
                  className="text-muted-foreground hover:text-foreground transition"
                  aria-label="Copy URL"
                >
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
                    />
                  </svg>
                </button>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Main Content - Flex layout for list + detail */}
      <div className="max-w-7xl mx-auto px-4 py-4 flex gap-4">
        {/* Left: Traffic List */}
        <Card className={`${viewMode === 'list-detail' && (selectedRequest || selectedTcpConnection) ? 'flex-1' : 'w-full'} transition-all`}>
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle className="flex items-center gap-2">
                  {isTcp ? (
                    <>
                      <span>TCP Connections</span>
                      <span className="text-sm font-normal text-muted-foreground">
                        (Real-time)
                      </span>
                    </>
                  ) : (
                    <>
                      <span>HTTP Requests</span>
                      <span className="text-sm font-normal text-muted-foreground">
                        (Real-time)
                      </span>
                    </>
                  )}
                </CardTitle>
                <CardDescription>
                  {isTcp
                    ? `${totalTcpConnections} connection${totalTcpConnections !== 1 ? 's' : ''} total${hasTcpFilters ? ` (showing ${tcpConnections.length})` : ''}`
                    : `${totalRequests} request${totalRequests !== 1 ? 's' : ''} total${hasActiveFilters ? ` (showing ${requests.length})` : ''}`}
                </CardDescription>
              </div>
              <div className="flex items-center gap-3">
                {/* View mode toggle */}
                <div className="flex items-center border border-border rounded-md overflow-hidden">
                  <button
                    onClick={() => setViewMode('list')}
                    className={`px-2.5 py-1.5 text-xs font-medium transition-colors ${
                      viewMode === 'list'
                        ? 'bg-primary text-primary-foreground'
                        : 'bg-muted text-muted-foreground hover:text-foreground'
                    }`}
                    title="List only"
                  >
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 10h16M4 14h16M4 18h16" />
                    </svg>
                  </button>
                  <button
                    onClick={() => setViewMode('list-detail')}
                    className={`px-2.5 py-1.5 text-xs font-medium transition-colors ${
                      viewMode === 'list-detail'
                        ? 'bg-primary text-primary-foreground'
                        : 'bg-muted text-muted-foreground hover:text-foreground'
                    }`}
                    title="List + Detail"
                  >
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h7M4 10h7M4 14h7M4 18h7M14 6h6v12h-6z" />
                    </svg>
                  </button>
                </div>

                {/* Filter toggle button */}
                <button
                  onClick={() => setShowFilters(!showFilters)}
                  className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors ${
                    showFilters || (isTcp ? hasTcpFilters : hasActiveFilters)
                      ? 'bg-primary text-primary-foreground'
                      : 'bg-muted text-muted-foreground hover:text-foreground'
                  }`}
                >
                  Filters {(isTcp ? hasTcpFilters : hasActiveFilters) && '•'}
                </button>
                <div className="flex items-center gap-2">
                  <div className="w-2 h-2 bg-chart-2 rounded-full animate-pulse"></div>
                  <span className="text-xs text-muted-foreground">Live</span>
                </div>
              </div>
            </div>

            {/* Filter Panel */}
            {showFilters && (
              <div className="mt-4 pt-4 border-t space-y-4">
                {isTcp ? (
                  /* TCP Filters */
                  <div className="space-y-3">
                    <div>
                      <label className="text-xs text-muted-foreground mb-1.5 block">Client Address</label>
                      <div className="relative">
                        <input
                          type="text"
                          placeholder="Search by client address..."
                          value={clientAddrSearch}
                          onChange={(e) => setClientAddrSearch(e.target.value)}
                          className="w-full px-3 py-2 text-sm bg-muted border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-primary pr-8"
                        />
                        {isTcpSearchPending && (
                          <div className="absolute right-2 top-1/2 -translate-y-1/2">
                            <div className="w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin"></div>
                          </div>
                        )}
                      </div>
                    </div>
                    {hasTcpFilters && (
                      <div className="flex justify-end">
                        <button
                          onClick={clearTcpFilters}
                          className="text-xs text-destructive hover:text-destructive/80 transition-colors"
                        >
                          Clear filters
                        </button>
                      </div>
                    )}
                  </div>
                ) : (
                  /* HTTP Filters */
                  <div className="space-y-3">
                    {/* Path Search */}
                    <div>
                      <label className="text-xs text-muted-foreground mb-1.5 block">Path</label>
                      <div className="relative">
                        <input
                          type="text"
                          placeholder="Search by path..."
                          value={pathSearch}
                          onChange={(e) => handlePathSearchChange(e.target.value)}
                          className="w-full px-3 py-2 text-sm bg-muted border border-border rounded-md focus:outline-none focus:ring-2 focus:ring-primary pr-8"
                        />
                        {isSearchPending && (
                          <div className="absolute right-2 top-1/2 -translate-y-1/2">
                            <div className="w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin"></div>
                          </div>
                        )}
                      </div>
                    </div>

                    {/* Method Filter */}
                    <div>
                      <label className="text-xs text-muted-foreground mb-1.5 block">Method</label>
                      <div className="flex flex-wrap gap-1.5">
                        {(['all', 'GET', 'POST', 'PUT', 'DELETE', 'PATCH'] as MethodFilter[]).map((method) => (
                          <button
                            key={method}
                            onClick={() => handleMethodChange(method)}
                            className={`px-2.5 py-1 text-xs font-medium rounded transition-colors ${
                              methodFilter === method
                                ? 'bg-primary text-primary-foreground'
                                : 'bg-muted text-muted-foreground hover:text-foreground border border-border'
                            }`}
                          >
                            {method === 'all' ? 'All' : method}
                          </button>
                        ))}
                      </div>
                    </div>

                    {/* Status Filter */}
                    <div>
                      <label className="text-xs text-muted-foreground mb-1.5 block">Status Code</label>
                      <div className="flex flex-wrap gap-1.5">
                        {([
                          { value: 'all', label: 'All' },
                          { value: '2xx', label: '2xx Success' },
                          { value: '3xx', label: '3xx Redirect' },
                          { value: '4xx', label: '4xx Client Error' },
                          { value: '5xx', label: '5xx Server Error' },
                        ] as { value: StatusFilter; label: string }[]).map(({ value, label }) => (
                          <button
                            key={value}
                            onClick={() => handleStatusChange(value)}
                            className={`px-2.5 py-1 text-xs font-medium rounded transition-colors ${
                              statusFilter === value
                                ? 'bg-primary text-primary-foreground'
                                : 'bg-muted text-muted-foreground hover:text-foreground border border-border'
                            }`}
                          >
                            {label}
                          </button>
                        ))}
                      </div>
                    </div>

                    {/* Clear Filters */}
                    {hasActiveFilters && (
                      <div className="flex justify-end">
                        <button
                          onClick={clearFilters}
                          className="text-xs text-destructive hover:text-destructive/80 transition-colors"
                        >
                          Clear all filters
                        </button>
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}
          </CardHeader>
          <CardContent>
            <div className="space-y-1 max-h-[calc(100vh-280px)] min-h-[300px] overflow-y-auto">
              {isTcp ? (
                // TCP Connections View
                tcpLoading ? (
                  <div className="space-y-1">
                    {Array.from({ length: 8 }).map((_, i) => (
                      <div key={i} className="px-3 py-2 rounded-md bg-muted/30">
                        <div className="flex items-center gap-2">
                          <Skeleton className="w-1.5 h-1.5 rounded-full" />
                          <Skeleton className="h-3 w-24" />
                          <Skeleton className="h-3 w-3" />
                          <Skeleton className="h-3 w-12" />
                          <Skeleton className="h-3 w-16 ml-auto" />
                          <Skeleton className="h-3 w-16" />
                          <Skeleton className="h-3 w-10" />
                        </div>
                      </div>
                    ))}
                  </div>
                ) : tcpConnections.length === 0 ? (
                  <div className="py-12 text-center">
                    <div className="text-4xl opacity-20 mb-3">🔌</div>
                    <div className="text-muted-foreground">
                      {hasTcpFilters ? 'No connections match your filters' : 'No TCP connections captured yet'}
                    </div>
                    <div className="text-xs text-muted-foreground/60 mt-2">
                      {hasTcpFilters ? (
                        <button onClick={clearTcpFilters} className="text-primary hover:underline">
                          Clear filters
                        </button>
                      ) : (
                        'Connections will appear here in real-time'
                      )}
                    </div>
                  </div>
                ) : (
                  // eslint-disable-next-line @typescript-eslint/no-explicit-any
                  tcpConnections.map((conn: any) => (
                    <button
                      key={conn.id}
                      onClick={() => setSelectedTcpConnection(conn)}
                      className={`w-full text-left px-3 py-2 rounded-md border transition hover:bg-muted/80 ${
                        selectedTcpConnection?.id === conn.id
                          ? 'bg-muted border-primary'
                          : 'bg-muted/30 border-transparent hover:border-border'
                      }`}
                    >
                      <div className="flex items-center gap-2">
                        <div className="w-1.5 h-1.5 bg-chart-1 rounded-full shrink-0"></div>
                        <span className="text-xs font-mono text-foreground truncate">
                          {conn.client_addr}
                        </span>
                        <span className="text-muted-foreground text-xs">→</span>
                        <span className="text-xs font-mono text-primary">
                          :{conn.target_port}
                        </span>
                        <span className="text-[10px] text-chart-2 font-mono ml-auto">
                          ↓{formatBytes(conn.bytes_received)}
                        </span>
                        <span className="text-[10px] text-chart-1 font-mono">
                          ↑{formatBytes(conn.bytes_sent)}
                        </span>
                        {conn.duration_ms && (
                          <span className="text-[10px] text-muted-foreground shrink-0">
                            {conn.duration_ms}ms
                          </span>
                        )}
                      </div>
                    </button>
                  ))
                )
              ) : (
                // HTTP Requests View
                requestsLoading ? (
                  <div className="space-y-1">
                    {Array.from({ length: 10 }).map((_, i) => (
                      <div key={i} className="px-3 py-2 rounded-md bg-muted/30">
                        <div className="flex items-center gap-2">
                          <Skeleton className="h-4 w-10 rounded" />
                          <Skeleton className="h-3 flex-1 max-w-[200px]" />
                          <Skeleton className="h-4 w-8 rounded ml-auto" />
                          <Skeleton className="h-3 w-10" />
                        </div>
                      </div>
                    ))}
                  </div>
                ) : requests.length === 0 ? (
                  <div className="py-12 text-center">
                    <div className="text-4xl opacity-20 mb-3">📡</div>
                    <div className="text-muted-foreground">
                      {hasActiveFilters ? 'No requests match your filters' : 'No HTTP requests captured yet'}
                    </div>
                    <div className="text-xs text-muted-foreground/60 mt-2">
                      {hasActiveFilters ? (
                        <button onClick={clearFilters} className="text-primary hover:underline">
                          Clear filters
                        </button>
                      ) : (
                        'Requests will appear here in real-time'
                      )}
                    </div>
                  </div>
                ) : (
                  // eslint-disable-next-line @typescript-eslint/no-explicit-any
                  requests.map((request: any) => (
                    <button
                      key={request.id}
                      onClick={() => setSelectedRequest(request)}
                      className={`w-full text-left px-3 py-2 rounded-md border transition hover:bg-muted/80 ${
                        selectedRequest?.id === request.id
                          ? 'bg-muted border-primary'
                          : 'bg-muted/30 border-transparent hover:border-border'
                      }`}
                    >
                      <div className="flex items-center gap-2">
                        <Badge
                          variant="outline"
                          className="bg-chart-1/20 text-chart-1 border-chart-1/50 shrink-0 text-[10px] px-1.5 py-0"
                        >
                          {request.method}
                        </Badge>
                        <span className="text-xs font-mono text-foreground truncate flex-1">
                          {request.path}
                        </span>
                        {request.status && (
                          <Badge variant={getStatusCodeBadgeVariant(request.status)} className="text-[10px] px-1.5 py-0">
                            {request.status}
                          </Badge>
                        )}
                        <span className="text-[10px] text-muted-foreground shrink-0">
                          {request.duration_ms !== undefined && request.duration_ms !== null ? `${request.duration_ms}ms` : ''}
                        </span>
                      </div>
                    </button>
                  ))
                )
              )}
            </div>

            {/* Pagination */}
            {((isTcp && totalTcpPages > 0) || (!isTcp && totalPages > 0)) && (
              <div className="mt-4 pt-4 border-t">
                <div className="flex items-center justify-between flex-wrap gap-3">
                  {/* Page Size Selector */}
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-muted-foreground">Show:</span>
                    <select
                      value={isTcp ? tcpPageSize : pageSize}
                      onChange={(e) => isTcp ? handleTcpPageSizeChange(Number(e.target.value)) : handlePageSizeChange(Number(e.target.value))}
                      className="px-2 py-1 text-xs bg-muted border border-border rounded focus:outline-none focus:ring-2 focus:ring-primary cursor-pointer"
                    >
                      {PAGE_SIZE_OPTIONS.map((size) => (
                        <option key={size} value={size}>
                          {size}
                        </option>
                      ))}
                    </select>
                    <span className="text-xs text-muted-foreground">per page</span>
                  </div>

                  {/* Pagination Controls */}
                  <div className="flex items-center gap-2">
                    {/* First Page */}
                    <button
                      onClick={() => isTcp ? setTcpCurrentPage(1) : setCurrentPage(1)}
                      disabled={(isTcp ? tcpCurrentPage : currentPage) === 1}
                      className="px-2 py-1 text-xs bg-muted border border-border rounded text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                      title="First page"
                    >
                      «
                    </button>

                    {/* Previous */}
                    <button
                      onClick={() => isTcp ? setTcpCurrentPage(p => Math.max(1, p - 1)) : setCurrentPage(p => Math.max(1, p - 1))}
                      disabled={(isTcp ? tcpCurrentPage : currentPage) === 1}
                      className="px-3 py-1 text-xs bg-muted border border-border rounded text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                    >
                      Prev
                    </button>

                    {/* Page Info */}
                    <div className="flex items-center gap-1 px-2">
                      <span className="text-xs text-muted-foreground">Page</span>
                      <input
                        type="number"
                        min={1}
                        max={isTcp ? totalTcpPages : totalPages}
                        value={isTcp ? tcpCurrentPage : currentPage}
                        onChange={(e) => {
                          const page = parseInt(e.target.value, 10);
                          const maxPages = isTcp ? totalTcpPages : totalPages;
                          if (!isNaN(page) && page >= 1 && page <= maxPages) {
                            isTcp ? setTcpCurrentPage(page) : setCurrentPage(page);
                          }
                        }}
                        className="w-12 px-2 py-1 text-xs text-center bg-muted border border-border rounded focus:outline-none focus:ring-2 focus:ring-primary"
                      />
                      <span className="text-xs text-muted-foreground">of {isTcp ? totalTcpPages : totalPages}</span>
                    </div>

                    {/* Next */}
                    <button
                      onClick={() => {
                        const maxPages = isTcp ? totalTcpPages : totalPages;
                        isTcp ? setTcpCurrentPage(p => Math.min(maxPages, p + 1)) : setCurrentPage(p => Math.min(maxPages, p + 1));
                      }}
                      disabled={(isTcp ? tcpCurrentPage : currentPage) === (isTcp ? totalTcpPages : totalPages)}
                      className="px-3 py-1 text-xs bg-muted border border-border rounded text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                    >
                      Next
                    </button>

                    {/* Last Page */}
                    <button
                      onClick={() => isTcp ? setTcpCurrentPage(totalTcpPages) : setCurrentPage(totalPages)}
                      disabled={(isTcp ? tcpCurrentPage : currentPage) === (isTcp ? totalTcpPages : totalPages)}
                      className="px-2 py-1 text-xs bg-muted border border-border rounded text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                      title="Last page"
                    >
                      »
                    </button>
                  </div>

                  {/* Items Info */}
                  <div className="text-xs text-muted-foreground">
                    {isTcp ? (
                      totalTcpConnections > 0 ? (
                        <>
                          Showing {(tcpCurrentPage - 1) * tcpPageSize + 1}-{Math.min(tcpCurrentPage * tcpPageSize, totalTcpConnections)} of {totalTcpConnections}
                        </>
                      ) : null
                    ) : (
                      totalRequests > 0 ? (
                        <>
                          Showing {(currentPage - 1) * pageSize + 1}-{Math.min(currentPage * pageSize, totalRequests)} of {totalRequests}
                        </>
                      ) : null
                    )}
                  </div>
                </div>
              </div>
            )}
          </CardContent>
        </Card>

        {/* Right: Detail Panel (inline sidebar) - only shown in list-detail mode */}
        {viewMode === 'list-detail' && selectedRequest && (
          <Card className="w-[400px] shrink-0 flex flex-col max-h-[calc(100vh-120px)]">
            {/* Header */}
            <CardHeader className="py-3 px-4 border-b shrink-0">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <Badge
                    variant="outline"
                    className="bg-chart-1/20 text-chart-1 border-chart-1/50"
                  >
                    {selectedRequest.method}
                  </Badge>
                  {selectedRequest.status && (
                    <Badge variant={getStatusCodeBadgeVariant(selectedRequest.status)}>
                      {selectedRequest.status}
                    </Badge>
                  )}
                  {selectedRequest.duration_ms !== undefined && selectedRequest.duration_ms !== null && (
                    <span className="text-xs text-muted-foreground">
                      {selectedRequest.duration_ms}ms
                    </span>
                  )}
                </div>
                <button
                  onClick={() => setSelectedRequest(null)}
                  className="p-1 hover:bg-muted rounded transition"
                  aria-label="Close panel"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </CardHeader>

            {/* Content */}
            <CardContent className="p-4 space-y-3 overflow-y-auto flex-1">
              {/* Path */}
              <div>
                <label className="text-xs text-muted-foreground mb-1 block">Path</label>
                <div className="text-xs font-mono bg-muted p-2 rounded break-all">
                  {selectedRequest.path}
                </div>
              </div>

              {/* Timestamp */}
              <div>
                <label className="text-xs text-muted-foreground mb-1 block">Timestamp</label>
                <div className="text-xs">
                  {selectedRequest.timestamp ? formatDate(selectedRequest.timestamp) : 'Unknown'}
                </div>
              </div>

              {/* Request Headers */}
              {selectedRequest.headers && selectedRequest.headers.length > 0 && (
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Request Headers</label>
                  <div className="bg-muted rounded p-2 text-[10px] font-mono max-h-32 overflow-auto space-y-0.5">
                    {selectedRequest.headers.map(([key, value]: [string, string], i: number) => (
                      <div key={i}>
                        <span className="text-chart-1">{key}:</span>{' '}
                        <span className="text-foreground break-all">{value}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Request Body */}
              {selectedRequest.body && (
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Request Body</label>
                  <div className="bg-muted rounded p-2 text-[10px] font-mono max-h-32 overflow-auto whitespace-pre-wrap break-all">
                    {decodeBase64Preview(selectedRequest.body, 2000)}
                  </div>
                </div>
              )}

              {/* Response Headers */}
              {selectedRequest.response_headers && selectedRequest.response_headers.length > 0 && (
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Response Headers</label>
                  <div className="bg-muted rounded p-2 text-[10px] font-mono max-h-32 overflow-auto space-y-0.5">
                    {selectedRequest.response_headers.map(([key, value]: [string, string], i: number) => (
                      <div key={i}>
                        <span className="text-chart-2">{key}:</span>{' '}
                        <span className="text-foreground break-all">{value}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Response Body */}
              {selectedRequest.response_body && (
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Response Body</label>
                  <div className="bg-muted rounded p-2 text-[10px] font-mono max-h-48 overflow-auto whitespace-pre-wrap break-all">
                    {decodeBase64Preview(selectedRequest.response_body, 5000)}
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        )}

        {/* Right: TCP Connection Detail Panel (inline sidebar) - only shown in list-detail mode */}
        {viewMode === 'list-detail' && selectedTcpConnection && (
          <Card className="w-[400px] shrink-0 flex flex-col max-h-[calc(100vh-120px)]">
            {/* Header */}
            <CardHeader className="py-3 px-4 border-b shrink-0">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span className="text-xs font-mono">{selectedTcpConnection.client_addr}</span>
                  <span className="text-muted-foreground text-xs">→</span>
                  <span className="text-xs font-mono text-primary">:{selectedTcpConnection.target_port}</span>
                </div>
                <button
                  onClick={() => setSelectedTcpConnection(null)}
                  className="p-1 hover:bg-muted rounded transition"
                  aria-label="Close panel"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </CardHeader>

            {/* Content */}
            <CardContent className="p-4 space-y-3 overflow-y-auto flex-1">
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Bytes Received</label>
                  <div className="text-sm font-mono text-chart-2">
                    {formatBytes(selectedTcpConnection.bytes_received)}
                  </div>
                </div>
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Bytes Sent</label>
                  <div className="text-sm font-mono text-chart-1">
                    {formatBytes(selectedTcpConnection.bytes_sent)}
                  </div>
                </div>
              </div>

              {selectedTcpConnection.duration_ms && (
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Duration</label>
                  <div className="text-xs">{selectedTcpConnection.duration_ms}ms</div>
                </div>
              )}

              <div>
                <label className="text-xs text-muted-foreground mb-1 block">Connected At</label>
                <div className="text-xs">{formatDate(selectedTcpConnection.connected_at)}</div>
              </div>

              {selectedTcpConnection.disconnected_at && (
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Disconnected At</label>
                  <div className="text-xs">{formatDate(selectedTcpConnection.disconnected_at)}</div>
                </div>
              )}

              {selectedTcpConnection.disconnect_reason && (
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Disconnect Reason</label>
                  <div className="text-xs font-mono bg-muted p-2 rounded">
                    {selectedTcpConnection.disconnect_reason}
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}
