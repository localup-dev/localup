import { useEffect, useState, useCallback } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import {
  Plus,
  Network,
  Play,
  Square,
  Trash2,
  Copy,
  ExternalLink,
  Loader2,
  RefreshCw,
  AlertCircle,
  CheckCircle2,
  WifiOff,
  Pencil,
} from "lucide-react";
import { toast } from "sonner";
import {
  listTunnels,
  createTunnel,
  updateTunnel,
  deleteTunnel,
  startTunnel,
  stopTunnel,
  type Tunnel,
  type CreateTunnelRequest,
  type UpdateTunnelRequest,
} from "@/api/tunnels";
import { listRelays, type RelayServer } from "@/api/relays";

type TunnelStatus = "disconnected" | "connecting" | "connected" | "error";

function getStatusBadge(status: string) {
  const statusLower = status.toLowerCase() as TunnelStatus;
  switch (statusLower) {
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
          <WifiOff className="h-3 w-3 mr-1" />
          Disconnected
        </Badge>
      );
  }
}

function TunnelSkeleton() {
  return (
    <div className="flex items-center justify-between p-4 border rounded-lg">
      <div className="space-y-2">
        <Skeleton className="h-5 w-32" />
        <Skeleton className="h-4 w-48" />
      </div>
      <div className="flex items-center gap-2">
        <Skeleton className="h-6 w-20" />
        <Skeleton className="h-9 w-9" />
      </div>
    </div>
  );
}

export function Tunnels() {
  const [tunnels, setTunnels] = useState<Tunnel[]>([]);
  const [relays, setRelays] = useState<RelayServer[]>([]);
  const [loading, setLoading] = useState(true);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [editDialogOpen, setEditDialogOpen] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [tunnelToEdit, setTunnelToEdit] = useState<Tunnel | null>(null);
  const [tunnelToDelete, setTunnelToDelete] = useState<Tunnel | null>(null);
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  // Form state for editing tunnel
  const [editTunnel, setEditTunnel] = useState<UpdateTunnelRequest>({});

  // Form state for creating tunnel
  const [newTunnel, setNewTunnel] = useState<CreateTunnelRequest>({
    name: "",
    relay_id: "",
    local_port: 3000,
    protocol: "http",
    subdomain: "",
    auto_start: false,
  });

  const loadData = useCallback(async () => {
    try {
      const [tunnelData, relayData] = await Promise.all([
        listTunnels(),
        listRelays(),
      ]);
      setTunnels(tunnelData);
      setRelays(relayData);
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

  const handleCreateTunnel = async () => {
    if (!newTunnel.name.trim()) {
      toast.error("Name is required");
      return;
    }
    if (!newTunnel.relay_id) {
      toast.error("Please select a relay server");
      return;
    }
    if (newTunnel.local_port < 1 || newTunnel.local_port > 65535) {
      toast.error("Invalid port number");
      return;
    }

    setActionLoading("create");
    try {
      const created = await createTunnel({
        ...newTunnel,
        subdomain: newTunnel.subdomain ? newTunnel.subdomain.toLowerCase() : undefined,
      });
      setTunnels((prev) => [...prev, created]);
      setCreateDialogOpen(false);
      setNewTunnel({
        name: "",
        relay_id: "",
        local_port: 3000,
        protocol: "http",
        subdomain: "",
        auto_start: false,
      });
      toast.success("Tunnel created", {
        description: `Created tunnel "${created.name}"`,
      });
    } catch (error) {
      toast.error("Failed to create tunnel", {
        description: String(error),
      });
    } finally {
      setActionLoading(null);
    }
  };

  const handleEditTunnel = async () => {
    if (!tunnelToEdit) return;

    if (editTunnel.local_port !== undefined && (editTunnel.local_port < 1 || editTunnel.local_port > 65535)) {
      toast.error("Invalid port number");
      return;
    }

    setActionLoading("edit");
    try {
      const updated = await updateTunnel(tunnelToEdit.id, {
        ...editTunnel,
        subdomain: editTunnel.subdomain?.toLowerCase(),
      });
      setTunnels((prev) => prev.map((t) => (t.id === tunnelToEdit.id ? updated : t)));
      setEditDialogOpen(false);
      setTunnelToEdit(null);
      setEditTunnel({});
      toast.success("Tunnel updated", {
        description: `Updated tunnel "${updated.name}"`,
      });
    } catch (error) {
      toast.error("Failed to update tunnel", {
        description: String(error),
      });
    } finally {
      setActionLoading(null);
    }
  };

  const openEditDialog = (tunnel: Tunnel) => {
    setTunnelToEdit(tunnel);
    setEditTunnel({
      name: tunnel.name,
      local_host: tunnel.local_host,
      local_port: tunnel.local_port,
      subdomain: tunnel.subdomain || "",
      auto_start: tunnel.auto_start,
    });
    setEditDialogOpen(true);
  };

  const handleDeleteTunnel = async () => {
    if (!tunnelToDelete) return;

    setActionLoading(`delete-${tunnelToDelete.id}`);
    try {
      await deleteTunnel(tunnelToDelete.id);
      setTunnels((prev) => prev.filter((t) => t.id !== tunnelToDelete.id));
      toast.success("Tunnel deleted", {
        description: `Deleted tunnel "${tunnelToDelete.name}"`,
      });
    } catch (error) {
      toast.error("Failed to delete tunnel", {
        description: String(error),
      });
    } finally {
      setActionLoading(null);
      setDeleteDialogOpen(false);
      setTunnelToDelete(null);
    }
  };

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

  const openInBrowser = (url: string) => {
    window.open(url, "_blank");
  };

  if (loading) {
    return (
      <div className="space-y-6">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-3xl font-bold">Tunnels</h1>
            <p className="text-muted-foreground">
              Manage your tunnel configurations
            </p>
          </div>
          <Skeleton className="h-10 w-32" />
        </div>
        <Card>
          <CardHeader>
            <CardTitle>Your Tunnels</CardTitle>
            <CardDescription>
              Tunnels let you expose local servers to the internet
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <TunnelSkeleton />
            <TunnelSkeleton />
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">Tunnels</h1>
          <p className="text-muted-foreground">
            Manage your tunnel configurations
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="icon" onClick={loadData}>
            <RefreshCw className="h-4 w-4" />
          </Button>
          <Dialog open={createDialogOpen} onOpenChange={setCreateDialogOpen}>
            <DialogTrigger asChild>
              <Button disabled={relays.length === 0}>
                <Plus className="h-4 w-4 mr-2" />
                New Tunnel
              </Button>
            </DialogTrigger>
            <DialogContent className="sm:max-w-[425px]">
              <DialogHeader>
                <DialogTitle>Create New Tunnel</DialogTitle>
                <DialogDescription>
                  Configure a new tunnel to expose a local service
                </DialogDescription>
              </DialogHeader>
              <div className="grid gap-4 py-4">
                <div className="grid gap-2">
                  <Label htmlFor="name">Name</Label>
                  <Input
                    id="name"
                    placeholder="My Local Server"
                    value={newTunnel.name}
                    onChange={(e) =>
                      setNewTunnel({ ...newTunnel, name: e.target.value })
                    }
                  />
                </div>
                <div className="grid gap-2">
                  <Label htmlFor="relay">Relay Server</Label>
                  <Select
                    value={newTunnel.relay_id}
                    onValueChange={(value) =>
                      setNewTunnel({ ...newTunnel, relay_id: value })
                    }
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select a relay" />
                    </SelectTrigger>
                    <SelectContent>
                      {relays.map((relay) => (
                        <SelectItem key={relay.id} value={relay.id}>
                          {relay.name}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="grid grid-cols-2 gap-4">
                  <div className="grid gap-2">
                    <Label htmlFor="port">Local Port</Label>
                    <Input
                      id="port"
                      type="number"
                      min={1}
                      max={65535}
                      value={newTunnel.local_port}
                      onChange={(e) =>
                        setNewTunnel({
                          ...newTunnel,
                          local_port: parseInt(e.target.value) || 3000,
                        })
                      }
                    />
                  </div>
                  <div className="grid gap-2">
                    <Label htmlFor="protocol">Protocol</Label>
                    <Select
                      value={newTunnel.protocol}
                      onValueChange={(value) =>
                        setNewTunnel({ ...newTunnel, protocol: value })
                      }
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="http">HTTP</SelectItem>
                        <SelectItem value="https">HTTPS</SelectItem>
                        <SelectItem value="tcp">TCP</SelectItem>
                        <SelectItem value="tls">TLS</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                </div>
                {(newTunnel.protocol === "http" ||
                  newTunnel.protocol === "https") && (
                  <div className="grid gap-2">
                    <Label htmlFor="subdomain">Subdomain (optional)</Label>
                    <Input
                      id="subdomain"
                      placeholder="myapp"
                      value={newTunnel.subdomain}
                      onChange={(e) =>
                        setNewTunnel({ ...newTunnel, subdomain: e.target.value })
                      }
                    />
                    <p className="text-xs text-muted-foreground">
                      Leave empty for a random subdomain
                    </p>
                  </div>
                )}
                <div className="flex items-center justify-between">
                  <div className="space-y-0.5">
                    <Label htmlFor="auto-start">Auto Start</Label>
                    <p className="text-xs text-muted-foreground">
                      Start tunnel when app launches
                    </p>
                  </div>
                  <Switch
                    id="auto-start"
                    checked={newTunnel.auto_start}
                    onCheckedChange={(checked) =>
                      setNewTunnel({ ...newTunnel, auto_start: checked })
                    }
                  />
                </div>
              </div>
              <DialogFooter>
                <Button
                  variant="outline"
                  onClick={() => setCreateDialogOpen(false)}
                >
                  Cancel
                </Button>
                <Button
                  onClick={handleCreateTunnel}
                  disabled={actionLoading === "create"}
                >
                  {actionLoading === "create" && (
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  )}
                  Create Tunnel
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        </div>
      </div>

      {relays.length === 0 && (
        <Card className="border-yellow-500/50 bg-yellow-500/5">
          <CardContent className="pt-6">
            <div className="flex items-center gap-3">
              <AlertCircle className="h-5 w-5 text-yellow-500" />
              <div>
                <p className="font-medium">No Relay Servers Configured</p>
                <p className="text-sm text-muted-foreground">
                  Add a relay server in the Relays tab before creating tunnels
                </p>
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <CardTitle>Your Tunnels</CardTitle>
          <CardDescription>
            Tunnels let you expose local servers to the internet
          </CardDescription>
        </CardHeader>
        <CardContent>
          {tunnels.length === 0 ? (
            <div className="rounded-lg border border-dashed border-border p-12 text-center">
              <Network className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
              <h3 className="text-lg font-medium mb-2">No Tunnels Configured</h3>
              <p className="text-sm text-muted-foreground mb-4 max-w-md mx-auto">
                Create a tunnel to expose your local development server.
                {relays.length === 0 && " You'll need to add a relay server first."}
              </p>
              <Button
                variant="outline"
                onClick={() => setCreateDialogOpen(true)}
                disabled={relays.length === 0}
              >
                <Plus className="h-4 w-4 mr-2" />
                Create Your First Tunnel
              </Button>
            </div>
          ) : (
            <div className="space-y-4">
              {tunnels.map((tunnel) => {
                const isConnected = tunnel.status.toLowerCase() === "connected";
                const isConnecting = tunnel.status.toLowerCase() === "connecting";
                const isLoading =
                  actionLoading === `start-${tunnel.id}` ||
                  actionLoading === `stop-${tunnel.id}`;

                return (
                  <div
                    key={tunnel.id}
                    className="flex items-center justify-between p-4 border rounded-lg hover:bg-muted/50 transition-colors"
                  >
                    <div className="space-y-1">
                      <div className="flex items-center gap-3">
                        <span className="font-medium">{tunnel.name}</span>
                        {getStatusBadge(tunnel.status)}
                      </div>
                      <div className="text-sm text-muted-foreground">
                        <span className="font-mono">
                          {tunnel.local_host}:{tunnel.local_port}
                        </span>
                        <span className="mx-2">→</span>
                        <span className="uppercase text-xs">{tunnel.protocol}</span>
                        {tunnel.relay_name && (
                          <span className="ml-2 text-xs">
                            via {tunnel.relay_name}
                          </span>
                        )}
                      </div>
                      {tunnel.public_url && (
                        <div className="flex items-center gap-2 mt-2">
                          <span className="text-sm font-mono text-primary truncate max-w-xs">
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
                            onClick={() => openInBrowser(tunnel.public_url!)}
                          >
                            <ExternalLink className="h-3 w-3" />
                          </Button>
                        </div>
                      )}
                      {tunnel.error_message && (
                        <p className="text-sm text-red-500 mt-1">
                          {tunnel.error_message}
                        </p>
                      )}
                    </div>
                    <div className="flex items-center gap-2">
                      {isConnected || isConnecting ? (
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
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => openEditDialog(tunnel)}
                        disabled={isConnected || isConnecting}
                        title={isConnected || isConnecting ? "Stop tunnel to edit" : "Edit tunnel"}
                      >
                        <Pencil className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="text-destructive hover:text-destructive"
                        onClick={() => {
                          setTunnelToDelete(tunnel);
                          setDeleteDialogOpen(true);
                        }}
                        disabled={actionLoading === `delete-${tunnel.id}`}
                      >
                        {actionLoading === `delete-${tunnel.id}` ? (
                          <Loader2 className="h-4 w-4 animate-spin" />
                        ) : (
                          <Trash2 className="h-4 w-4" />
                        )}
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      <AlertDialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Tunnel</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete "{tunnelToDelete?.name}"? This
              action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDeleteTunnel}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <Dialog open={editDialogOpen} onOpenChange={(open) => {
        setEditDialogOpen(open);
        if (!open) {
          setTunnelToEdit(null);
          setEditTunnel({});
        }
      }}>
        <DialogContent className="sm:max-w-[425px]">
          <DialogHeader>
            <DialogTitle>Edit Tunnel</DialogTitle>
            <DialogDescription>
              Update tunnel configuration. Changes take effect on next start.
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="edit-name">Name</Label>
              <Input
                id="edit-name"
                value={editTunnel.name || ""}
                onChange={(e) =>
                  setEditTunnel({ ...editTunnel, name: e.target.value })
                }
              />
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div className="grid gap-2">
                <Label htmlFor="edit-host">Local Host</Label>
                <Input
                  id="edit-host"
                  placeholder="localhost"
                  value={editTunnel.local_host || ""}
                  onChange={(e) =>
                    setEditTunnel({ ...editTunnel, local_host: e.target.value })
                  }
                />
              </div>
              <div className="grid gap-2">
                <Label htmlFor="edit-port">Local Port</Label>
                <Input
                  id="edit-port"
                  type="number"
                  min={1}
                  max={65535}
                  value={editTunnel.local_port || ""}
                  onChange={(e) =>
                    setEditTunnel({
                      ...editTunnel,
                      local_port: parseInt(e.target.value) || undefined,
                    })
                  }
                />
              </div>
            </div>
            {(tunnelToEdit?.protocol === "http" ||
              tunnelToEdit?.protocol === "https") && (
              <div className="grid gap-2">
                <Label htmlFor="edit-subdomain">Subdomain</Label>
                <Input
                  id="edit-subdomain"
                  placeholder="myapp"
                  value={editTunnel.subdomain || ""}
                  onChange={(e) =>
                    setEditTunnel({ ...editTunnel, subdomain: e.target.value.toLowerCase() })
                  }
                />
                <p className="text-xs text-muted-foreground">
                  Leave empty for a random subdomain
                </p>
              </div>
            )}
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <Label htmlFor="edit-auto-start">Auto Start</Label>
                <p className="text-xs text-muted-foreground">
                  Start tunnel when app launches
                </p>
              </div>
              <Switch
                id="edit-auto-start"
                checked={editTunnel.auto_start || false}
                onCheckedChange={(checked) =>
                  setEditTunnel({ ...editTunnel, auto_start: checked })
                }
              />
            </div>
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setEditDialogOpen(false);
                setTunnelToEdit(null);
                setEditTunnel({});
              }}
            >
              Cancel
            </Button>
            <Button
              onClick={handleEditTunnel}
              disabled={actionLoading === "edit"}
            >
              {actionLoading === "edit" && (
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              )}
              Save Changes
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
