import { useState } from 'react';
import { Cable, Shield } from 'lucide-react';
import { useTunnels } from '../hooks/useApi';
import { useCurrentUser } from '../components/Layout';
import TunnelCard from '../components/TunnelCard';
import { Switch } from '../components/ui/switch';
import { Label } from '../components/ui/label';
import { Skeleton } from '../components/ui/skeleton';
import type { Tunnel } from '../api/client/types.gen';

function TunnelCardSkeleton() {
  return (
    <div className="bg-card rounded-lg border border-border p-6 space-y-4">
      <div className="flex items-start justify-between">
        <div className="space-y-2 flex-1">
          <Skeleton className="h-5 w-48" />
          <Skeleton className="h-3 w-24" />
        </div>
        <Skeleton className="h-6 w-20 rounded-full" />
      </div>
      <div className="flex gap-2">
        <Skeleton className="h-5 w-14 rounded-full" />
        <Skeleton className="h-5 w-16 rounded-full" />
      </div>
      <div className="space-y-2">
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-3/4" />
      </div>
      <div className="flex gap-4 pt-2 border-t border-border">
        <Skeleton className="h-4 w-24" />
        <Skeleton className="h-4 w-28" />
      </div>
    </div>
  );
}

export default function Tunnels() {
  const [includeInactive, setIncludeInactive] = useState(false);
  const [viewAll, setViewAll] = useState(true);
  const { isAdmin } = useCurrentUser();

  // Admin: default to viewing all tunnels; toggle switches to 'mine'
  // Non-admin: scope is always undefined (backend defaults to 'mine')
  const scope = isAdmin ? (viewAll ? 'all' : 'mine') : undefined;

  const { data: tunnelsData, isLoading, error } = useTunnels(includeInactive, scope);
  const tunnels = tunnelsData?.tunnels || [];

  return (
    <div className="min-h-screen bg-background">
      {/* Header */}
      <div className="border-b border-border">
        <div className="max-w-7xl mx-auto px-6 py-6">
          <div className="flex items-center justify-between">
            <div>
              <h1 className="text-3xl font-bold text-foreground">
                {isAdmin && viewAll ? 'All Users\' ' : ''}
                {includeInactive ? 'Tunnels' : 'Active Tunnels'}
              </h1>
              <p className="text-muted-foreground mt-2">
                {isAdmin && viewAll ? 'Viewing all tunnels across users' : 'Monitor and manage your ' + (includeInactive ? 'tunnels' : 'running tunnels')} ({tunnels.length} {includeInactive ? 'total' : 'active'})
              </p>
            </div>
            <div className="flex items-center gap-6">
              {isAdmin && (
                <div className="flex items-center gap-2">
                  <Shield className="h-4 w-4 text-amber-500" />
                  <Switch
                    id="view-all"
                    checked={viewAll}
                    onCheckedChange={setViewAll}
                  />
                  <Label htmlFor="view-all" className="text-sm cursor-pointer text-amber-600 dark:text-amber-400">
                    View all tunnels
                  </Label>
                </div>
              )}
              <div className="flex items-center gap-3">
                <Switch
                  id="show-inactive"
                  checked={includeInactive}
                  onCheckedChange={setIncludeInactive}
                />
                <Label htmlFor="show-inactive" className="text-sm cursor-pointer">
                  Show inactive tunnels
                </Label>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="max-w-7xl mx-auto px-6 py-8">
        {error && (
          <div className="mb-6 bg-destructive/10 border border-destructive/50 text-destructive px-4 py-3 rounded-lg">
            Error loading tunnels: {error.message}
          </div>
        )}

        {isLoading ? (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {[1, 2, 3, 4, 5, 6].map((i) => (
              <TunnelCardSkeleton key={i} />
            ))}
          </div>
        ) : tunnels.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 space-y-4">
            <div className="w-16 h-16 rounded-full bg-muted flex items-center justify-center">
              <Cable className="h-8 w-8 text-muted-foreground" />
            </div>
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
            {tunnels.map((tunnel: Tunnel) => (
              <TunnelCard key={tunnel.id} tunnel={tunnel} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
