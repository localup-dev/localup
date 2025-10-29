import { useState } from 'react';
import { useTunnels, useStopTunnel } from '../hooks/useTunnels';
import { Square, Activity, Server, Trash2 } from 'lucide-react';
import type { TunnelInfo } from '../types/tunnel';

interface TunnelListProps {
  onSelectTunnel: (tunnelId: string) => void;
  onCreateTunnel: () => void;
}

export function TunnelList({ onSelectTunnel, onCreateTunnel }: TunnelListProps) {
  const { data: tunnels = [], isLoading } = useTunnels();
  const stopTunnel = useStopTunnel();
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const handleStopTunnel = async (e: React.MouseEvent, tunnelId: string) => {
    e.stopPropagation();
    console.log('🛑 [TunnelList] Stop button clicked for tunnel:', tunnelId);
    await stopTunnel.mutateAsync(tunnelId);
  };

  const handleSelectTunnel = (tunnelId: string) => {
    setSelectedId(tunnelId);
    onSelectTunnel(tunnelId);
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-muted-foreground">Loading tunnels...</div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {tunnels.length === 0 ? (
        <div className="bg-card border border-border rounded-lg p-8 text-center">
          <Server className="mx-auto text-muted-foreground mb-4" size={48} />
          <p className="text-muted-foreground mb-4">No tunnels created yet</p>
          <button
            onClick={onCreateTunnel}
            className="px-6 py-3 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
          >
            Create Your First Tunnel
          </button>
        </div>
      ) : (
        <div className="space-y-3">
          {tunnels.map((tunnel) => (
            <TunnelCard
              key={tunnel.id}
              tunnel={tunnel}
              isSelected={selectedId === tunnel.id}
              onSelect={() => handleSelectTunnel(tunnel.id)}
              onStop={(e) => handleStopTunnel(e, tunnel.id)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

interface TunnelCardProps {
  tunnel: TunnelInfo;
  isSelected: boolean;
  onSelect: () => void;
  onStop: (e: React.MouseEvent) => void;
}

function TunnelCard({ tunnel, isSelected, onSelect, onStop }: TunnelCardProps) {
  const statusColors = {
    connecting: 'bg-yellow-500',
    connected: 'bg-green-500',
    disconnected: 'bg-muted-foreground',
    error: 'bg-destructive',
  };

  return (
    <div
      onClick={onSelect}
      className={`bg-card rounded-lg border-2 p-4 cursor-pointer transition-all hover:border-primary/50 ${
        isSelected ? 'border-primary' : 'border-border'
      }`}
    >
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3 flex-1 min-w-0">
          <Activity className="text-primary flex-shrink-0" size={20} />
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <h3 className="font-semibold text-foreground truncate">{tunnel.name}</h3>
              <span className={`w-2 h-2 rounded-full flex-shrink-0 ${statusColors[tunnel.status]}`} />
            </div>
            <p className="text-sm text-muted-foreground truncate">
              {tunnel.config.protocols.map((p) => p.type.toUpperCase()).join(', ')} • {tunnel.config.local_host}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-1 flex-shrink-0 ml-2">
          {tunnel.status === 'connected' && (
            <button
              onClick={onStop}
              className="p-2 text-muted-foreground hover:text-destructive transition-colors"
              title="Stop tunnel"
            >
              <Square size={16} />
            </button>
          )}
          <button
            onClick={(e) => {
              e.stopPropagation();
              // TODO: Implement delete
              console.log('Delete tunnel:', tunnel.id);
            }}
            className="p-2 text-muted-foreground hover:text-destructive transition-colors"
            title="Delete tunnel"
          >
            <Trash2 size={16} />
          </button>
        </div>
      </div>
    </div>
  );
}
