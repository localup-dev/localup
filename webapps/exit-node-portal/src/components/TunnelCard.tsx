import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './ui/card';
import { Badge } from './ui/badge';

interface TunnelEndpoint {
  protocol: {
    type: string;
  };
  public_url: string;
  local_port?: number;
}

interface Tunnel {
  id: string;
  status: string;
  connected_at: string;
  endpoints: TunnelEndpoint[];
}

interface TunnelCardProps {
  tunnel: Tunnel;
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

const getProtocolBadgeColor = (protocol: string) => {
  switch (protocol.toLowerCase()) {
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

export default function TunnelCard({ tunnel }: TunnelCardProps) {
  const navigate = useNavigate();

  // Group endpoints by protocol type
  const protocolGroups = tunnel.endpoints.reduce((acc, endpoint) => {
    const type = endpoint.protocol.type;
    if (!acc[type]) acc[type] = [];
    acc[type].push(endpoint);
    return acc;
  }, {} as Record<string, TunnelEndpoint[]>);

  return (
    <Card
      className="hover:border-primary/50 transition-all cursor-pointer hover:shadow-lg hover:shadow-primary/10"
      onClick={() => navigate(`/tunnels/${tunnel.id}`)}
    >
      <CardHeader className="pb-3">
        <div className="flex items-start justify-between">
          <div className="space-y-1 flex-1 min-w-0">
            <CardTitle className="text-lg font-mono truncate">
              {tunnel.id}
            </CardTitle>
            <CardDescription className="text-xs">
              Connected {formatRelativeTime(tunnel.connected_at)}
            </CardDescription>
          </div>
          <Badge variant={getStatusVariant(tunnel.status)} className="ml-2 shrink-0">
            {tunnel.status}
          </Badge>
        </div>
      </CardHeader>

      <CardContent className="space-y-3">
        {/* Protocol badges */}
        <div className="flex flex-wrap gap-2">
          {Object.keys(protocolGroups).map((protocol) => (
            <Badge
              key={protocol}
              variant="outline"
              className={getProtocolBadgeColor(protocol)}
            >
              {protocol.toUpperCase()}
              {protocolGroups[protocol].length > 1 && (
                <span className="ml-1 opacity-70">×{protocolGroups[protocol].length}</span>
              )}
            </Badge>
          ))}
        </div>

        {/* Endpoints */}
        <div className="space-y-1.5">
          {tunnel.endpoints.slice(0, 3).map((endpoint, i) => (
            <div key={i} className="text-xs">
              <div className="flex items-center gap-2 text-muted-foreground">
                <span className="font-mono">→</span>
                <span className="truncate text-primary hover:text-primary/80 font-mono">
                  {endpoint.public_url}
                </span>
              </div>
              {endpoint.local_port && (
                <div className="ml-5 text-muted-foreground">
                  :{endpoint.local_port}
                </div>
              )}
            </div>
          ))}
          {tunnel.endpoints.length > 3 && (
            <div className="text-xs text-muted-foreground ml-5">
              +{tunnel.endpoints.length - 3} more
            </div>
          )}
        </div>

        {/* Stats preview */}
        <div className="flex items-center gap-4 pt-2 text-xs text-muted-foreground border-t">
          <div className="flex items-center gap-1">
            <span>📊</span>
            <span>View traffic</span>
          </div>
          <div className="flex items-center gap-1">
            <span>🔗</span>
            <span>{tunnel.endpoints.length} endpoint{tunnel.endpoints.length !== 1 ? 's' : ''}</span>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
