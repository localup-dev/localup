import { useState, useEffect } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
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
  Server,
  Pencil,
  Trash2,
  CheckCircle,
  XCircle,
  Loader2,
} from "lucide-react";
import { Checkbox } from "@/components/ui/checkbox";
import {
  listRelays,
  addRelay,
  updateRelay,
  deleteRelay,
  testRelay,
  type RelayServer,
  type CreateRelayRequest,
  type TunnelProtocol,
} from "@/api/relays";

const ALL_PROTOCOLS: TunnelProtocol[] = ["http", "https", "tcp", "tls"];

const PROTOCOL_LABELS: Record<TunnelProtocol, string> = {
  http: "HTTP",
  https: "HTTPS",
  tcp: "TCP",
  tls: "TLS/SNI",
};

export function Relays() {
  const [relays, setRelays] = useState<RelayServer[]>([]);
  const [loading, setLoading] = useState(true);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [selectedRelay, setSelectedRelay] = useState<RelayServer | null>(null);
  const [testingId, setTestingId] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, boolean | null>>({});

  // Form state
  const [formData, setFormData] = useState<CreateRelayRequest>({
    name: "",
    address: "",
    jwt_token: "",
    protocol: "quic",
    insecure: false,
    is_default: false,
    supported_protocols: [...ALL_PROTOCOLS],
  });
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    loadRelays();
  }, []);

  async function loadRelays() {
    try {
      setLoading(true);
      const data = await listRelays();
      setRelays(data);
    } catch (error) {
      console.error("Failed to load relays:", error);
    } finally {
      setLoading(false);
    }
  }

  function openAddDialog() {
    setSelectedRelay(null);
    setFormData({
      name: "",
      address: "",
      jwt_token: "",
      protocol: "quic",
      insecure: false,
      is_default: relays.length === 0, // First relay is default
      supported_protocols: [...ALL_PROTOCOLS],
    });
    setDialogOpen(true);
  }

  function openEditDialog(relay: RelayServer) {
    setSelectedRelay(relay);
    setFormData({
      name: relay.name,
      address: relay.address,
      jwt_token: relay.jwt_token || "",
      protocol: relay.protocol,
      insecure: relay.insecure,
      is_default: relay.is_default,
      supported_protocols: relay.supported_protocols || [...ALL_PROTOCOLS],
    });
    setDialogOpen(true);
  }

  function toggleProtocol(protocol: TunnelProtocol) {
    const current = formData.supported_protocols || [];
    if (current.includes(protocol)) {
      // Don't allow removing all protocols
      if (current.length > 1) {
        setFormData({
          ...formData,
          supported_protocols: current.filter((p) => p !== protocol),
        });
      }
    } else {
      setFormData({
        ...formData,
        supported_protocols: [...current, protocol],
      });
    }
  }

  function openDeleteDialog(relay: RelayServer) {
    setSelectedRelay(relay);
    setDeleteDialogOpen(true);
  }

  async function handleSave() {
    try {
      setSaving(true);
      if (selectedRelay) {
        await updateRelay(selectedRelay.id, formData);
      } else {
        await addRelay(formData);
      }
      setDialogOpen(false);
      await loadRelays();
    } catch (error) {
      console.error("Failed to save relay:", error);
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!selectedRelay) return;
    try {
      await deleteRelay(selectedRelay.id);
      setDeleteDialogOpen(false);
      setSelectedRelay(null);
      await loadRelays();
    } catch (error) {
      console.error("Failed to delete relay:", error);
    }
  }

  async function handleTest(id: string) {
    try {
      setTestingId(id);
      setTestResults((prev) => ({ ...prev, [id]: null }));
      const result = await testRelay(id);
      setTestResults((prev) => ({ ...prev, [id]: result.success }));
    } catch {
      setTestResults((prev) => ({ ...prev, [id]: false }));
    } finally {
      setTestingId(null);
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">Relay Servers</h1>
          <p className="text-muted-foreground">
            Configure relay servers for your tunnels
          </p>
        </div>
        <Button onClick={openAddDialog}>
          <Plus className="h-4 w-4 mr-2" />
          Add Relay
        </Button>
      </div>

      {relays.length === 0 ? (
        <Card>
          <CardHeader>
            <CardTitle>Your Relays</CardTitle>
            <CardDescription>
              Relay servers route traffic to your local tunnels
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="rounded-lg border border-dashed border-border p-12 text-center">
              <Server className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
              <h3 className="text-lg font-medium mb-2">No Relay Servers</h3>
              <p className="text-sm text-muted-foreground mb-4 max-w-md mx-auto">
                Add a relay server to connect your tunnels to the internet. Each
                relay requires an address and JWT token.
              </p>
              <Button variant="outline" onClick={openAddDialog}>
                <Plus className="h-4 w-4 mr-2" />
                Add Your First Relay
              </Button>
            </div>
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-4">
          {relays.map((relay) => (
            <Card key={relay.id}>
              <CardHeader className="pb-2">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <Server className="h-5 w-5 text-muted-foreground" />
                    <div>
                      <CardTitle className="text-lg flex items-center gap-2">
                        {relay.name}
                        {relay.is_default && (
                          <Badge variant="secondary">Default</Badge>
                        )}
                      </CardTitle>
                      <CardDescription className="font-mono">
                        {relay.address}
                      </CardDescription>
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    {testResults[relay.id] === true && (
                      <CheckCircle className="h-5 w-5 text-green-500" />
                    )}
                    {testResults[relay.id] === false && (
                      <XCircle className="h-5 w-5 text-red-500" />
                    )}
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => handleTest(relay.id)}
                      disabled={testingId === relay.id}
                    >
                      {testingId === relay.id ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        "Test"
                      )}
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => openEditDialog(relay)}
                    >
                      <Pencil className="h-4 w-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => openDeleteDialog(relay)}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              </CardHeader>
              <CardContent>
                <div className="flex flex-wrap gap-4 text-sm text-muted-foreground">
                  <span>Connection: {relay.protocol.toUpperCase()}</span>
                  {relay.insecure && <Badge variant="destructive">Insecure</Badge>}
                  {relay.jwt_token && <span>Token configured</span>}
                </div>
                <div className="flex gap-2 mt-2">
                  <span className="text-sm text-muted-foreground">Tunnels:</span>
                  {relay.supported_protocols?.map((protocol) => (
                    <Badge key={protocol} variant="outline">
                      {PROTOCOL_LABELS[protocol]}
                    </Badge>
                  ))}
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      {/* Add/Edit Dialog */}
      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {selectedRelay ? "Edit Relay" : "Add Relay Server"}
            </DialogTitle>
            <DialogDescription>
              {selectedRelay
                ? "Update the relay server configuration"
                : "Add a new relay server to connect your tunnels"}
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="name">Name</Label>
              <Input
                id="name"
                placeholder="My Relay"
                value={formData.name}
                onChange={(e) =>
                  setFormData({ ...formData, name: e.target.value })
                }
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="address">Address</Label>
              <Input
                id="address"
                placeholder="relay.localup.dev:4443"
                value={formData.address}
                onChange={(e) =>
                  setFormData({ ...formData, address: e.target.value })
                }
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="jwt_token">JWT Token</Label>
              <Input
                id="jwt_token"
                type="password"
                placeholder="eyJhbGciOiJIUzI1NiIs..."
                value={formData.jwt_token || ""}
                onChange={(e) =>
                  setFormData({ ...formData, jwt_token: e.target.value || null })
                }
              />
            </div>
            <div className="flex items-center justify-between">
              <Label htmlFor="insecure">Skip TLS Verification</Label>
              <Switch
                id="insecure"
                checked={formData.insecure}
                onCheckedChange={(checked) =>
                  setFormData({ ...formData, insecure: checked })
                }
              />
            </div>
            <div className="flex items-center justify-between">
              <Label htmlFor="is_default">Set as Default</Label>
              <Switch
                id="is_default"
                checked={formData.is_default}
                onCheckedChange={(checked) =>
                  setFormData({ ...formData, is_default: checked })
                }
              />
            </div>
            <div className="grid gap-2">
              <Label>Supported Tunnel Protocols</Label>
              <div className="flex flex-wrap gap-4">
                {ALL_PROTOCOLS.map((protocol) => (
                  <div key={protocol} className="flex items-center space-x-2">
                    <Checkbox
                      id={`protocol-${protocol}`}
                      checked={formData.supported_protocols?.includes(protocol)}
                      onCheckedChange={() => toggleProtocol(protocol)}
                    />
                    <label
                      htmlFor={`protocol-${protocol}`}
                      className="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
                    >
                      {PROTOCOL_LABELS[protocol]}
                    </label>
                  </div>
                ))}
              </div>
              <p className="text-xs text-muted-foreground">
                Select which tunnel types this relay supports
              </p>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDialogOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleSave} disabled={saving}>
              {saving && <Loader2 className="h-4 w-4 mr-2 animate-spin" />}
              {selectedRelay ? "Save Changes" : "Add Relay"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation */}
      <AlertDialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Relay Server?</AlertDialogTitle>
            <AlertDialogDescription>
              This will delete the relay server "{selectedRelay?.name}" and all
              associated tunnels. This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleDelete}>Delete</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
