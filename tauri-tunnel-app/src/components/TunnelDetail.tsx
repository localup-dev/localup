import { useTunnels } from '../hooks/useTunnels';
import { MetricsDashboard } from './MetricsDashboard';
import { TrafficInspector } from './TrafficInspector';
import { BarChart3, Activity, ExternalLink, Globe, MapPin, Shield } from 'lucide-react';
import { useState } from 'react';

interface TunnelDetailProps {
  tunnelId: string;
}

export function TunnelDetail({ tunnelId }: TunnelDetailProps) {
  const { data: tunnels = [] } = useTunnels();
  const tunnel = tunnels.find((t) => t.id === tunnelId);
  const [activeTab, setActiveTab] = useState<'overview' | 'metrics' | 'traffic'>('overview');

  if (!tunnel) {
    return (
      <div className="bg-card border border-border rounded-lg p-12 text-center">
        <Activity size={48} className="mx-auto text-muted-foreground mb-4" />
        <h3 className="text-lg font-semibold text-foreground mb-2">Tunnel Not Found</h3>
        <p className="text-muted-foreground">The selected tunnel could not be found.</p>
      </div>
    );
  }

  const statusColors = {
    connecting: 'bg-yellow-500/10 text-yellow-600 dark:text-yellow-400 border-yellow-500/20',
    connected: 'bg-green-500/10 text-green-600 dark:text-green-400 border-green-500/20',
    disconnected: 'bg-muted text-muted-foreground border-border',
    error: 'bg-destructive/10 text-destructive border-destructive/20',
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="bg-card border border-border rounded-lg p-6">
        <div className="flex items-start justify-between mb-4">
          <div>
            <h1 className="text-2xl font-bold text-foreground mb-2">{tunnel.name}</h1>
            <div className="flex items-center gap-4 text-sm text-muted-foreground">
              <div className="flex items-center gap-2">
                <Globe size={16} />
                <span>{tunnel.config.local_host}</span>
              </div>
              <div className="flex items-center gap-2">
                <Shield size={16} />
                <span>{tunnel.config.protocols.map((p) => p.type.toUpperCase()).join(', ')}</span>
              </div>
            </div>
          </div>
          <span
            className={`px-3 py-1.5 rounded-lg text-sm font-medium border ${
              statusColors[tunnel.status]
            }`}
          >
            {tunnel.status}
          </span>
        </div>

        {/* Endpoints */}
        {tunnel.endpoints.length > 0 && (
          <div className="border-t border-border pt-4 space-y-2">
            <p className="text-sm font-medium text-foreground mb-2">Public Endpoints:</p>
            {tunnel.endpoints.map((endpoint, idx) => (
              <div
                key={idx}
                className="flex items-center gap-2 p-2 bg-muted/30 rounded-lg"
              >
                <MapPin size={14} className="text-primary flex-shrink-0" />
                <code className="text-sm text-foreground flex-1 truncate">
                  {endpoint.public_url}
                </code>
                <button
                  onClick={() => window.open(endpoint.public_url, '_blank')}
                  className="p-1.5 text-primary hover:bg-primary/10 rounded transition-colors flex-shrink-0"
                  title="Open in browser"
                >
                  <ExternalLink size={14} />
                </button>
              </div>
            ))}
          </div>
        )}

        {tunnel.error && (
          <div className="mt-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg">
            <p className="text-sm text-destructive">
              <strong>Error:</strong> {tunnel.error}
            </p>
          </div>
        )}
      </div>

      {/* Tabs */}
      <div className="bg-card border border-border rounded-lg">
        <div className="flex border-b border-border">
          <button
            onClick={() => setActiveTab('overview')}
            className={`flex items-center gap-2 px-6 py-4 font-medium transition-colors ${
              activeTab === 'overview'
                ? 'border-b-2 border-primary text-primary'
                : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            <Activity size={18} />
            Overview
          </button>
          <button
            onClick={() => setActiveTab('metrics')}
            className={`flex items-center gap-2 px-6 py-4 font-medium transition-colors ${
              activeTab === 'metrics'
                ? 'border-b-2 border-primary text-primary'
                : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            <BarChart3 size={18} />
            Metrics
          </button>
          <button
            onClick={() => setActiveTab('traffic')}
            className={`flex items-center gap-2 px-6 py-4 font-medium transition-colors ${
              activeTab === 'traffic'
                ? 'border-b-2 border-primary text-primary'
                : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            <Activity size={18} />
            Traffic
          </button>
        </div>

        <div className="p-6">
          {activeTab === 'overview' && (
            <div className="space-y-6">
              <div>
                <h3 className="text-sm font-medium text-foreground mb-3">Configuration</h3>
                <div className="space-y-2 text-sm">
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">Local Host:</span>
                    <span className="text-foreground font-medium">{tunnel.config.local_host}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">Failover:</span>
                    <span className="text-foreground font-medium">
                      {tunnel.config.failover ? 'Enabled' : 'Disabled'}
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-muted-foreground">Exit Node:</span>
                    <span className="text-foreground font-medium">
                      {tunnel.config.exit_node.type}
                    </span>
                  </div>
                </div>
              </div>

              <div>
                <h3 className="text-sm font-medium text-foreground mb-3">Protocols</h3>
                <div className="space-y-2">
                  {tunnel.config.protocols.map((protocol, idx) => (
                    <div
                      key={idx}
                      className="flex items-center justify-between p-3 bg-muted/30 rounded-lg"
                    >
                      <div>
                        <span className="text-sm font-medium text-foreground">
                          {protocol.type.toUpperCase()}
                        </span>
                        {('subdomain' in protocol && protocol.subdomain) && (
                          <span className="text-sm text-muted-foreground ml-2">
                            ({protocol.subdomain})
                          </span>
                        )}
                      </div>
                      <span className="text-sm text-muted-foreground">
                        Port {protocol.local_port}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}

          {activeTab === 'metrics' && <MetricsDashboard tunnelId={tunnelId} />}
          {activeTab === 'traffic' && <TrafficInspector tunnelId={tunnelId} />}
        </div>
      </div>
    </div>
  );
}
