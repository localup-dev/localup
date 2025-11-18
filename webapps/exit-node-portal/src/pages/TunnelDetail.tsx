import { useNavigate, useParams } from 'react-router-dom';
import { useTunnels, useTunnelRequests, useTunnelTcpConnections } from '../hooks/useApi';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../components/ui/card';
import { Badge } from '../components/ui/badge';

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

  // Fetch tunnels to get tunnel details
  const { data: tunnelsData, isLoading: tunnelsLoading, error: tunnelsError } = useTunnels();
  const tunnels = tunnelsData?.tunnels || [];

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const tunnel = tunnels.find((t: any) => t.id === tunnelId);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const isTcp = tunnel?.endpoints.some((e: any) => e.protocol.type === 'tcp');

  // Fetch requests or TCP connections based on tunnel type
  const { data: requestsData } = useTunnelRequests(!isTcp && tunnelId ? tunnelId : null);
  const requests = requestsData?.requests || [];

  const { data: tcpConnectionsData } = useTunnelTcpConnections(isTcp && tunnelId ? tunnelId : null);
  const tcpConnections = tcpConnectionsData?.connections || [];

  if (tunnelsLoading) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="text-muted-foreground">Loading tunnel details...</div>
      </div>
    );
  }

  if (tunnelsError) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="text-destructive">Error loading tunnel: {tunnelsError.message}</div>
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
      {/* Header */}
      <div className="border-b bg-card/50 backdrop-blur-sm sticky top-0 z-10">
        <div className="max-w-7xl mx-auto px-6 py-6">
          <div className="flex items-start justify-between">
            <div className="flex items-start gap-4">
              <button
                onClick={() => navigate('/tunnels')}
                className="mt-1 text-muted-foreground hover:text-foreground transition"
                aria-label="Back to tunnels"
              >
                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
                </svg>
              </button>
              <div>
                <h1 className="text-3xl font-bold text-foreground font-mono">{tunnel.id}</h1>
                <p className="text-muted-foreground mt-2">
                  Connected {formatRelativeTime(tunnel.connected_at)} • {formatDate(tunnel.connected_at)}
                </p>
              </div>
            </div>
            <Badge variant={getStatusVariant(tunnel.status)} className="mt-1">
              {tunnel.status}
            </Badge>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="max-w-7xl mx-auto px-6 py-8 space-y-6">
        {/* Endpoints Card */}
        <Card>
          <CardHeader>
            <CardTitle>Endpoints</CardTitle>
            <CardDescription>Public URLs and routing information</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {/* eslint-disable-next-line @typescript-eslint/no-explicit-any */}
            {tunnel.endpoints.map((endpoint: any, i: number) => (
              <div
                key={i}
                className="flex items-center justify-between p-4 bg-muted/50 rounded-lg border hover:border-primary/50 transition"
              >
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-3 mb-2">
                    <Badge
                      variant="outline"
                      className={getProtocolBadgeColor(endpoint.protocol.type)}
                    >
                      {endpoint.protocol.type.toUpperCase()}
                    </Badge>
                    <span className="text-sm text-muted-foreground">
                      Port {endpoint.local_port || endpoint.protocol.port || 'N/A'}
                    </span>
                  </div>
                  <div className="flex items-center gap-2">
                    <a
                      href={endpoint.public_url}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-primary hover:text-primary/80 font-mono text-sm truncate"
                    >
                      {endpoint.public_url}
                    </a>
                    <button
                      onClick={() => navigator.clipboard.writeText(endpoint.public_url)}
                      className="text-muted-foreground hover:text-foreground transition"
                      aria-label="Copy URL"
                    >
                      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          strokeWidth={2}
                          d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
                        />
                      </svg>
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </CardContent>
        </Card>

        {/* Real-time Traffic Card */}
        <Card>
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
                    ? `${tcpConnections.length} connection${tcpConnections.length !== 1 ? 's' : ''} captured`
                    : `${requests.length} request${requests.length !== 1 ? 's' : ''} captured`}
                </CardDescription>
              </div>
              <div className="flex items-center gap-2">
                <div className="w-2 h-2 bg-chart-2 rounded-full animate-pulse"></div>
                <span className="text-xs text-muted-foreground">Live</span>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <div className="space-y-3 max-h-[600px] overflow-y-auto">
              {isTcp ? (
                // TCP Connections View
                tcpConnections.length === 0 ? (
                  <div className="py-12 text-center">
                    <div className="text-4xl opacity-20 mb-3">🔌</div>
                    <div className="text-muted-foreground">No TCP connections captured yet</div>
                    <div className="text-xs text-muted-foreground/60 mt-2">
                      Connections will appear here in real-time
                    </div>
                  </div>
                ) : (
                  // eslint-disable-next-line @typescript-eslint/no-explicit-any
                  tcpConnections.map((conn: any) => (
                    <div
                      key={conn.id}
                      className="p-4 bg-muted/50 rounded-lg border hover:border-primary/50 transition"
                    >
                      <div className="flex items-start justify-between mb-3">
                        <div className="flex items-center gap-3">
                          <div className="w-2 h-2 bg-chart-1 rounded-full"></div>
                          <div>
                            <div className="flex items-center gap-2">
                              <span className="text-sm font-mono text-foreground">
                                {conn.client_addr}
                              </span>
                              <span className="text-muted-foreground">→</span>
                              <span className="text-sm font-mono text-primary">
                                :{conn.target_port}
                              </span>
                            </div>
                            <div className="text-xs text-muted-foreground mt-1">
                              {formatDate(conn.connected_at)}
                            </div>
                          </div>
                        </div>
                        {conn.duration_ms && (
                          <Badge variant="outline">
                            {conn.duration_ms}ms
                          </Badge>
                        )}
                      </div>

                      <div className="grid grid-cols-2 gap-4 mt-3 pt-3 border-t">
                        <div>
                          <div className="text-xs text-muted-foreground mb-1">Received</div>
                          <div className="text-sm text-chart-2 font-mono">
                            📥 {formatBytes(conn.bytes_received)}
                          </div>
                        </div>
                        <div>
                          <div className="text-xs text-muted-foreground mb-1">Sent</div>
                          <div className="text-sm text-chart-1 font-mono">
                            📤 {formatBytes(conn.bytes_sent)}
                          </div>
                        </div>
                      </div>

                      {conn.disconnect_reason && (
                        <div className="mt-3 pt-3 border-t">
                          <div className="text-xs text-muted-foreground mb-1">Disconnect Reason</div>
                          <div className="text-xs text-muted-foreground font-mono">
                            {conn.disconnect_reason}
                          </div>
                        </div>
                      )}
                    </div>
                  ))
                )
              ) : (
                // HTTP Requests View
                requests.length === 0 ? (
                  <div className="py-12 text-center">
                    <div className="text-4xl opacity-20 mb-3">📡</div>
                    <div className="text-muted-foreground">No HTTP requests captured yet</div>
                    <div className="text-xs text-muted-foreground/60 mt-2">
                      Requests will appear here in real-time
                    </div>
                  </div>
                ) : (
                  // eslint-disable-next-line @typescript-eslint/no-explicit-any
                  requests.map((request: any) => (
                    <div
                      key={request.id}
                      className="p-4 bg-muted/50 rounded-lg border hover:border-primary/50 transition"
                    >
                      <div className="flex items-start justify-between mb-3">
                        <div className="flex items-center gap-3 flex-1 min-w-0">
                          <Badge
                            variant="outline"
                            className="bg-chart-1/20 text-chart-1 border-chart-1/50 shrink-0"
                          >
                            {request.method}
                          </Badge>
                          <span className="text-sm font-mono text-foreground truncate">
                            {request.path}
                          </span>
                        </div>
                        <div className="flex items-center gap-2 shrink-0 ml-3">
                          {request.status_code && (
                            <Badge variant={getStatusCodeBadgeVariant(request.status_code)}>
                              {request.status_code}
                            </Badge>
                          )}
                          {request.latency_ms !== undefined && (
                            <Badge variant="outline">
                              {request.latency_ms}ms
                            </Badge>
                          )}
                        </div>
                      </div>

                      <div className="text-xs text-muted-foreground">
                        {formatDate(request.created_at)}
                      </div>

                      {request.response_body && (
                        <div className="mt-3 pt-3 border-t">
                          <div className="text-xs text-muted-foreground mb-2">Response Preview</div>
                          <div className="text-xs text-muted-foreground font-mono bg-muted p-2 rounded max-h-24 overflow-auto">
                            {request.response_body.substring(0, 200)}
                            {request.response_body.length > 200 && '...'}
                          </div>
                        </div>
                      )}
                    </div>
                  ))
                )
              )}
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
