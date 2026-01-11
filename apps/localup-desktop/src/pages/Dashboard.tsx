import { useEffect, useState, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Activity,
  Network,
  Zap,
  Clock,
  Plus,
  Play,
  Square,
  ExternalLink,
  Copy,
  CheckCircle2,
  AlertCircle,
  Loader2,
  ArrowRight,
} from "lucide-react";
import { toast } from "sonner";
import {
  listTunnels,
  startTunnel,
  stopTunnel,
  type Tunnel,
} from "@/api/tunnels";
import { listRelays } from "@/api/relays";

function getStatusBadge(status: string) {
  switch (status.toLowerCase()) {
    case "connected":
      return (
        <Badge className="bg-green-500/10 text-green-500 hover:bg-green-500/20">
          <CheckCircle2 className="h-3 w-3 mr-1" />
          Connected
        </Badge>
      );
    case "connecting":
      return (
        <Badge className="bg-yellow-500/10 text-yellow-500 hover:bg-yellow-500/20">
          <Loader2 className="h-3 w-3 mr-1 animate-spin" />
          Connecting
        </Badge>
      );
    case "error":
      return (
        <Badge className="bg-red-500/10 text-red-500 hover:bg-red-500/20">
          <AlertCircle className="h-3 w-3 mr-1" />
          Error
        </Badge>
      );
    default:
      return (
        <Badge variant="secondary">
          Disconnected
        </Badge>
      );
  }
}

export function Dashboard() {
  const navigate = useNavigate();
  const [tunnels, setTunnels] = useState<Tunnel[]>([]);
  const [hasRelays, setHasRelays] = useState(false);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      const [tunnelData, relayData] = await Promise.all([
        listTunnels(),
        listRelays(),
      ]);
      setTunnels(tunnelData);
      setHasRelays(relayData.length > 0);
    } catch (error) {
      toast.error("Failed to load data", {
        description: String(error),
      });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();

    // Poll for status updates every 2 seconds
    const interval = setInterval(async () => {
      try {
        const tunnelData = await listTunnels();
        setTunnels(tunnelData);
      } catch {
        // Silently ignore polling errors
      }
    }, 2000);

    return () => clearInterval(interval);
  }, [loadData]);

  const handleStartTunnel = async (tunnel: Tunnel) => {
    setActionLoading(`start-${tunnel.id}`);
    try {
      const updated = await startTunnel(tunnel.id);
      setTunnels((prev) => prev.map((t) => (t.id === tunnel.id ? updated : t)));
      toast.success("Tunnel started", {
        description: `Starting tunnel "${tunnel.name}"...`,
      });
    } catch (error) {
      toast.error("Failed to start tunnel", {
        description: String(error),
      });
    } finally {
      setActionLoading(null);
    }
  };

  const handleStopTunnel = async (tunnel: Tunnel) => {
    setActionLoading(`stop-${tunnel.id}`);
    try {
      const updated = await stopTunnel(tunnel.id);
      setTunnels((prev) => prev.map((t) => (t.id === tunnel.id ? updated : t)));
      toast.success("Tunnel stopped", {
        description: `Stopped tunnel "${tunnel.name}"`,
      });
    } catch (error) {
      toast.error("Failed to stop tunnel", {
        description: String(error),
      });
    } finally {
      setActionLoading(null);
    }
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    toast.success("Copied to clipboard");
  };

  // Calculate stats
  const activeTunnels = tunnels.filter(
    (t) => t.status.toLowerCase() === "connected"
  );
  const connectingTunnels = tunnels.filter(
    (t) => t.status.toLowerCase() === "connecting"
  );
  const errorTunnels = tunnels.filter(
    (t) => t.status.toLowerCase() === "error"
  );

  if (loading) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-3xl font-bold">Dashboard</h1>
          <p className="text-muted-foreground">
            Overview of your tunnel activity
          </p>
        </div>
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
          {[1, 2, 3, 4].map((i) => (
            <Card key={i}>
              <CardHeader className="pb-2">
                <Skeleton className="h-4 w-24" />
              </CardHeader>
              <CardContent>
                <Skeleton className="h-8 w-12 mb-1" />
                <Skeleton className="h-3 w-32" />
              </CardContent>
            </Card>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Dashboard</h1>
        <p className="text-muted-foreground">
          Overview of your tunnel activity
        </p>
      </div>

      {/* Stats Grid */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Active Tunnels</CardTitle>
            <Network className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-green-500">
              {activeTunnels.length}
            </div>
            <p className="text-xs text-muted-foreground">
              {activeTunnels.length === 0
                ? "No tunnels running"
                : activeTunnels.length === 1
                ? "1 tunnel connected"
                : `${activeTunnels.length} tunnels connected`}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Total Tunnels</CardTitle>
            <Activity className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{tunnels.length}</div>
            <p className="text-xs text-muted-foreground">
              {tunnels.length === 0
                ? "No tunnels configured"
                : `${tunnels.length} configured`}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Connecting</CardTitle>
            <Zap className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-yellow-500">
              {connectingTunnels.length}
            </div>
            <p className="text-xs text-muted-foreground">
              {connectingTunnels.length === 0
                ? "None connecting"
                : `${connectingTunnels.length} in progress`}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Errors</CardTitle>
            <Clock className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className={`text-2xl font-bold ${errorTunnels.length > 0 ? "text-red-500" : ""}`}>
              {errorTunnels.length}
            </div>
            <p className="text-xs text-muted-foreground">
              {errorTunnels.length === 0
                ? "All healthy"
                : `${errorTunnels.length} need attention`}
            </p>
          </CardContent>
        </Card>
      </div>

      {/* Active Tunnels */}
      {activeTunnels.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle>Active Tunnels</CardTitle>
            <CardDescription>
              Currently running tunnels
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {activeTunnels.map((tunnel) => (
              <div
                key={tunnel.id}
                className="flex items-center justify-between p-3 border rounded-lg bg-green-500/5 border-green-500/20"
              >
                <div className="space-y-1 flex-1">
                  <div className="flex items-center gap-2">
                    <span
                      className="font-medium cursor-pointer hover:underline"
                      onClick={() => navigate(`/tunnels/${tunnel.id}`)}
                    >
                      {tunnel.name}
                    </span>
                    {getStatusBadge(tunnel.status)}
                  </div>
                  {tunnel.public_url && (
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-mono text-primary">
                        {tunnel.public_url}
                      </span>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-6 w-6"
                        onClick={() => copyToClipboard(tunnel.public_url!)}
                      >
                        <Copy className="h-3 w-3" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-6 w-6"
                        onClick={() => window.open(tunnel.public_url!, "_blank")}
                      >
                        <ExternalLink className="h-3 w-3" />
                      </Button>
                    </div>
                  )}
                  <p className="text-xs text-muted-foreground">
                    {tunnel.local_host}:{tunnel.local_port} via {tunnel.relay_name || "relay"}
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => navigate(`/tunnels/${tunnel.id}`)}
                    title="View details"
                  >
                    <ArrowRight className="h-4 w-4" />
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => handleStopTunnel(tunnel)}
                    disabled={actionLoading === `stop-${tunnel.id}`}
                  >
                    {actionLoading === `stop-${tunnel.id}` ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <>
                        <Square className="h-4 w-4 mr-2" />
                        Stop
                      </>
                    )}
                  </Button>
                </div>
              </div>
            ))}
          </CardContent>
        </Card>
      )}

      {/* Quick Actions / Empty State */}
      {tunnels.length === 0 ? (
        <Card>
          <CardHeader>
            <CardTitle>Quick Start</CardTitle>
            <CardDescription>
              Get started by creating a tunnel or adding a relay server
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="rounded-lg border border-dashed border-border p-6 text-center">
              <Network className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
              <h3 className="text-lg font-medium mb-2">No Tunnels Yet</h3>
              <p className="text-sm text-muted-foreground mb-4">
                {hasRelays
                  ? "Create your first tunnel to expose a local server to the internet"
                  : "First add a relay server, then create a tunnel"}
              </p>
              <Button
                onClick={() => navigate(hasRelays ? "/tunnels" : "/relays")}
              >
                <Plus className="h-4 w-4 mr-2" />
                {hasRelays ? "Create Tunnel" : "Add Relay Server"}
              </Button>
            </div>
          </CardContent>
        </Card>
      ) : activeTunnels.length === 0 ? (
        <Card>
          <CardHeader>
            <CardTitle>Quick Actions</CardTitle>
            <CardDescription>
              Start a tunnel or create a new one
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {tunnels.slice(0, 3).map((tunnel) => {
              const isConnecting = tunnel.status.toLowerCase() === "connecting";
              const isLoading =
                actionLoading === `start-${tunnel.id}` ||
                actionLoading === `stop-${tunnel.id}`;

              return (
                <div
                  key={tunnel.id}
                  className="flex items-center justify-between p-3 border rounded-lg"
                >
                  <div className="space-y-1">
                    <div className="flex items-center gap-2">
                      <span className="font-medium">{tunnel.name}</span>
                      {getStatusBadge(tunnel.status)}
                    </div>
                    <p className="text-xs text-muted-foreground">
                      {tunnel.local_host}:{tunnel.local_port} ({tunnel.protocol.toUpperCase()})
                    </p>
                    {tunnel.error_message && (
                      <p className="text-xs text-red-500">{tunnel.error_message}</p>
                    )}
                  </div>
                  {isConnecting ? (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => handleStopTunnel(tunnel)}
                      disabled={isLoading}
                    >
                      {isLoading ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <>
                          <Square className="h-4 w-4 mr-2" />
                          Stop
                        </>
                      )}
                    </Button>
                  ) : (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => handleStartTunnel(tunnel)}
                      disabled={isLoading}
                    >
                      {isLoading ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <>
                          <Play className="h-4 w-4 mr-2" />
                          Start
                        </>
                      )}
                    </Button>
                  )}
                </div>
              );
            })}
            {tunnels.length > 3 && (
              <Button
                variant="ghost"
                className="w-full"
                onClick={() => navigate("/tunnels")}
              >
                View all {tunnels.length} tunnels
              </Button>
            )}
          </CardContent>
        </Card>
      ) : null}
    </div>
  );
}
