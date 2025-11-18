import { useState } from 'react';
import { useTunnels } from '../hooks/useApi';
import TunnelCard from '../components/TunnelCard';

export default function Tunnels() {
  const [includeInactive, setIncludeInactive] = useState(false);

  // Fetch tunnels with automatic polling via React Query
  const { data: tunnelsData, isLoading, error } = useTunnels(includeInactive);
  const tunnels = tunnelsData?.tunnels || [];

  if (isLoading) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="text-muted-foreground">Loading tunnels...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="text-destructive">Error loading tunnels: {error.message}</div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-background">
      {/* Header */}
      <div className="border-b">
        <div className="max-w-7xl mx-auto px-6 py-6">
          <div className="flex items-center justify-between">
            <div>
              <h1 className="text-3xl font-bold text-foreground">
                {includeInactive ? 'All Tunnels' : 'Active Tunnels'}
              </h1>
              <p className="text-muted-foreground mt-2">
                Monitor and manage your {includeInactive ? 'tunnels' : 'running tunnels'} ({tunnels.length} {includeInactive ? 'total' : 'active'})
              </p>
            </div>
            <label className="flex items-center gap-3 cursor-pointer">
              <input
                type="checkbox"
                checked={includeInactive}
                onChange={(e) => setIncludeInactive(e.target.checked)}
                className="w-4 h-4 rounded border-input bg-background text-primary focus:ring-2 focus:ring-ring"
              />
              <span className="text-sm text-foreground">Show inactive tunnels</span>
            </label>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="max-w-7xl mx-auto px-6 py-8">
        {tunnels.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 space-y-4">
            <div className="text-6xl opacity-20">🔌</div>
            <div className="text-xl text-muted-foreground">
              {includeInactive ? 'No tunnels found' : 'No active tunnels'}
            </div>
            <div className="text-sm text-muted-foreground/60">
              {includeInactive
                ? 'No tunnel history available'
                : 'Start a tunnel to see it appear here'}
            </div>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {/* eslint-disable-next-line @typescript-eslint/no-explicit-any */}
            {tunnels.map((tunnel: any) => (
              <TunnelCard key={tunnel.id} tunnel={tunnel} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
