import { useState, useEffect } from "react";
import {
  Plus,
  Server,
  MapPin,
  Zap,
  X,
  Trash2,
  CheckCircle,
  XCircle,
  Loader2,
} from "lucide-react";
import { relayApi, type Relay as RelayType } from "@/api/database";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";

export function Relay() {
  const [relays, setRelays] = useState<RelayType[]>([]);
  const [isAddModalOpen, setIsAddModalOpen] = useState(false);

  // Load relays on component mount
  useEffect(() => {
    loadRelays();
  }, []);

  const loadRelays = async () => {
    console.log("🔄 [Relay] Loading relays...");
    try {
      const data = await relayApi.list();
      console.log(`✅ [Relay] Loaded ${data.length} relay(s):`, data);
      setRelays(data);
    } catch (error) {
      console.error("❌ [Relay] Failed to load relays:", error);
    }
  };

  const handleAddRelay = () => {
    console.log("➕ [Relay] Opening add relay modal");
    setIsAddModalOpen(true);
  };

  const handleDeleteRelay = async (id: string) => {
    console.log(`🗑️ [Relay] Delete requested for relay ID: ${id}`);

    const relay = relays.find((r) => r.id === id);
    console.log(`🔍 [Relay] Found relay to delete:`, relay);

    console.log(`🚀 [Relay] Starting delete for relay ID: ${id}`);
    const startTime = performance.now();

    try {
      console.log(`📡 [Relay] Calling relayApi.delete(${id})...`);
      await relayApi.delete(id);
      const deleteTime = performance.now() - startTime;
      console.log(
        `✅ [Relay] Delete API call succeeded (${deleteTime.toFixed(2)}ms)`
      );

      console.log("🔄 [Relay] Refreshing relay list...");
      await loadRelays();

      const totalTime = performance.now() - startTime;
      console.log(
        `✅ [Relay] Delete operation completed (${totalTime.toFixed(
          2
        )}ms total)`
      );
    } catch (error) {
      console.error("❌ [Relay] Failed to delete relay:", error);
      alert(`Failed to delete relay: ${error}`);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold text-foreground">
            Relay Management
          </h1>
          <p className="text-muted-foreground mt-1">
            Configure and monitor your relay servers
          </p>
        </div>
        <Button onClick={handleAddRelay}>
          <Plus size={16} />
          Add Relay
        </Button>
      </div>

      {/* Relay List */}
      <div className="bg-card border border-border rounded-lg p-6">
        <h2 className="text-lg font-semibold text-foreground mb-4">
          Configured Relays
        </h2>
        {relays.length === 0 ? (
          <div className="text-center py-12">
            <Server className="mx-auto text-muted-foreground mb-4" size={48} />
            <p className="text-muted-foreground">No relays configured</p>
            <p className="text-sm text-muted-foreground mt-2">
              Add a relay server to get started
            </p>
          </div>
        ) : (
          <div className="space-y-3">
            {relays.map((relay) => (
              <RelayCard
                key={relay.id}
                relay={relay}
                onDelete={handleDeleteRelay}
              />
            ))}
          </div>
        )}
      </div>

      {/* Quick Connect */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <QuickConnectCard
          icon={<MapPin className="text-primary" size={20} />}
          region="US East"
          status="Available"
          latency="12ms"
        />
        <QuickConnectCard
          icon={<MapPin className="text-primary" size={20} />}
          region="EU West"
          status="Available"
          latency="45ms"
        />
        <QuickConnectCard
          icon={<MapPin className="text-primary" size={20} />}
          region="Asia Pacific"
          status="Available"
          latency="89ms"
        />
      </div>

      {/* Add Relay Modal */}
      {isAddModalOpen && (
        <AddRelayModal
          onClose={() => setIsAddModalOpen(false)}
          onSuccess={() => {
            loadRelays();
            setIsAddModalOpen(false);
          }}
        />
      )}
    </div>
  );
}

interface RelayCardProps {
  relay: RelayType;
  onDelete: (id: string) => void;
}

function RelayCard({ relay, onDelete }: RelayCardProps) {
  const statusColors = {
    active: "bg-green-500",
    inactive: "bg-gray-500",
    error: "bg-red-500",
  };

  return (
    <div className="flex items-center justify-between p-4 border border-border rounded-lg hover:border-primary/50 transition-colors">
      <div className="flex items-center gap-4">
        <Server className="text-primary" size={20} />
        <div>
          <div className="flex items-center gap-2">
            <h3 className="font-semibold text-foreground">{relay.name}</h3>
            <span
              className={`w-2 h-2 rounded-full ${
                statusColors[relay.status as keyof typeof statusColors] ||
                statusColors.inactive
              }`}
            />
          </div>
          <p className="text-sm text-muted-foreground">{relay.address}</p>
          {relay.description && (
            <p className="text-xs text-muted-foreground mt-1">
              {relay.description}
            </p>
          )}
        </div>
      </div>
      <div className="flex items-center gap-2">
        <span className="text-sm text-muted-foreground px-3 py-1 bg-muted rounded">
          {relay.region}
        </span>
        <Button
          onClick={() => onDelete(relay.id)}
          variant="ghost"
          size="icon"
          className="hover:text-destructive"
        >
          <Trash2 size={16} />
        </Button>
      </div>
    </div>
  );
}

interface AddRelayModalProps {
  onClose: () => void;
  onSuccess: () => void;
}

function AddRelayModal({ onClose, onSuccess }: AddRelayModalProps) {
  const [formData, setFormData] = useState({
    name: "",
    address: "",
    region: "UsEast",
    description: "",
  });
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [isVerifying, setIsVerifying] = useState(false);
  const [verificationStatus, setVerificationStatus] = useState<
    "idle" | "success" | "error"
  >("idle");
  const [error, setError] = useState("");

  const handleVerify = async () => {
    if (!formData.address) {
      console.warn("⚠️ [AddRelay] No address provided for verification");
      setError("Please enter a relay address first");
      return;
    }

    console.log(`🔍 [AddRelay] Verifying relay address: ${formData.address}`);
    setError("");
    setIsVerifying(true);
    setVerificationStatus("idle");

    const startTime = performance.now();
    try {
      await invoke<boolean>("verify_relay", { address: formData.address });
      const verifyTime = performance.now() - startTime;
      console.log(
        `✅ [AddRelay] Verification succeeded (${verifyTime.toFixed(2)}ms)`
      );
      setVerificationStatus("success");
    } catch (err) {
      const verifyTime = performance.now() - startTime;
      console.error(
        `❌ [AddRelay] Verification failed (${verifyTime.toFixed(2)}ms):`,
        err
      );
      setVerificationStatus("error");
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsVerifying(false);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    console.log("📝 [AddRelay] Form submitted:", formData);
    setError("");

    // Check if this is a QUIC port (common ports: 4443, 5443, 443)
    const isQuicPort = formData.address.match(/:(?:4443|5443|443)$/);

    // Require verification for TCP relays, but allow QUIC relays without verification
    if (!isQuicPort && verificationStatus !== "success") {
      console.warn("⚠️ [AddRelay] TCP relay address not verified");
      setError("Please verify the relay address before creating");
      return;
    }

    if (isQuicPort && verificationStatus !== "success") {
      console.log("ℹ️ [AddRelay] Skipping verification for QUIC port (UDP-based)");
    }

    console.log("🚀 [AddRelay] Creating relay...");
    setIsSubmitting(true);

    const startTime = performance.now();
    try {
      const params = {
        name: formData.name,
        address: formData.address,
        region: formData.region,
        description: formData.description || undefined,
      };
      console.log("📡 [AddRelay] Calling relayApi.create with:", params);

      const result = await relayApi.create(params);
      const createTime = performance.now() - startTime;

      console.log(
        `✅ [AddRelay] Relay created successfully (${createTime.toFixed(
          2
        )}ms):`,
        result
      );
      onSuccess();
    } catch (err) {
      const createTime = performance.now() - startTime;
      console.error(
        `❌ [AddRelay] Failed to create relay (${createTime.toFixed(2)}ms):`,
        err
      );
      setError(err instanceof Error ? err.message : "Failed to create relay");
      setIsSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-background/80 backdrop-blur-sm flex items-center justify-center z-50">
      <div className="bg-card border border-border rounded-lg shadow-lg w-full max-w-md p-6">
        <div className="flex items-center justify-between mb-6">
          <h2 className="text-xl font-bold text-foreground">Add New Relay</h2>
          <Button onClick={onClose} variant="ghost" size="icon">
            <X size={20} />
          </Button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-foreground mb-2">
              Relay Name *
            </label>
            <input
              type="text"
              required
              value={formData.name}
              onChange={(e) =>
                setFormData({ ...formData, name: e.target.value })
              }
              placeholder="e.g., US East Production"
              className="w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-foreground mb-2">
              Address *
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                required
                value={formData.address}
                onChange={(e) => {
                  setFormData({ ...formData, address: e.target.value });
                  setVerificationStatus("idle"); // Reset verification when address changes
                }}
                placeholder="relay.example.com:8080"
                className="flex-1 px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
              />
              <Button
                type="button"
                onClick={handleVerify}
                disabled={isVerifying || !formData.address}
                variant="secondary"
              >
                {isVerifying ? (
                  <>
                    <Loader2 size={16} className="animate-spin" />
                    Verifying...
                  </>
                ) : (
                  <>
                    {verificationStatus === "success" && (
                      <CheckCircle size={16} className="text-green-500" />
                    )}
                    {verificationStatus === "error" && (
                      <XCircle size={16} className="text-destructive" />
                    )}
                    Verify
                  </>
                )}
              </Button>
            </div>
            {verificationStatus === "success" && (
              <p className="text-sm text-green-600 dark:text-green-400 mt-1 flex items-center gap-1">
                <CheckCircle size={14} />
                Connection verified successfully
              </p>
            )}
            {formData.address.match(/:(?:4443|5443|443)$/) && (
              <p className="text-xs text-muted-foreground mt-1">
                ℹ️ QUIC/UDP port detected - verification is optional
              </p>
            )}
          </div>

          <div>
            <label className="block text-sm font-medium text-foreground mb-2">
              Region *
            </label>
            <select
              required
              value={formData.region}
              onChange={(e) =>
                setFormData({ ...formData, region: e.target.value })
              }
              className="w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground focus:outline-none focus:ring-2 focus:ring-primary"
            >
              <option value="UsEast">US East</option>
              <option value="UsWest">US West</option>
              <option value="EuWest">EU West</option>
              <option value="EuCentral">EU Central</option>
              <option value="AsiaPacific">Asia Pacific</option>
              <option value="SouthAmerica">South America</option>
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-foreground mb-2">
              Description
            </label>
            <textarea
              value={formData.description}
              onChange={(e) =>
                setFormData({ ...formData, description: e.target.value })
              }
              placeholder="Optional description..."
              rows={3}
              className="w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary resize-none"
            />
          </div>

          {error && (
            <div className="text-sm text-destructive bg-destructive/10 px-3 py-2 rounded-lg">
              {error}
            </div>
          )}

          <div className="flex gap-3 pt-2">
            <Button
              type="button"
              onClick={onClose}
              variant="outline"
              className="flex-1"
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isSubmitting} className="flex-1">
              {isSubmitting ? "Creating..." : "Create Relay"}
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
}

interface QuickConnectCardProps {
  icon: React.ReactNode;
  region: string;
  status: string;
  latency: string;
}

function QuickConnectCard({
  icon,
  region,
  status,
  latency,
}: QuickConnectCardProps) {
  return (
    <div className="bg-card border border-border rounded-lg p-4 hover:border-primary/50 transition-colors cursor-pointer">
      <div className="flex items-center gap-3 mb-3">
        {icon}
        <span className="font-semibold text-foreground">{region}</span>
      </div>
      <div className="flex items-center justify-between text-sm">
        <span className="text-muted-foreground">{status}</span>
        <div className="flex items-center gap-1 text-muted-foreground">
          <Zap size={14} />
          <span>{latency}</span>
        </div>
      </div>
    </div>
  );
}
