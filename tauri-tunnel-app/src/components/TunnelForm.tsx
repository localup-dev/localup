import { useState, useEffect } from 'react';
import { useCreateTunnel } from '../hooks/useTunnels';
import { X, Plus, Trash2, AlertCircle } from 'lucide-react';
import type { TunnelConfig, ProtocolConfig, ExitNodeConfig } from '../types/tunnel';
import { relayApi, type Relay } from '@/api/database';

interface TunnelFormProps {
  onClose: () => void;
  onSuccess: () => void;
}

export function TunnelForm({ onClose, onSuccess }: TunnelFormProps) {
  const createTunnel = useCreateTunnel();
  const [name, setName] = useState('');
  const [localHost, setLocalHost] = useState('localhost');
  const [authToken, setAuthToken] = useState('');
  const [protocols, setProtocols] = useState<ProtocolConfig[]>([
    { type: 'http', local_port: 3000, subdomain: undefined },
  ]);
  const [exitNode, setExitNode] = useState<ExitNodeConfig>({ type: 'custom', address: '' });
  const [failover, setFailover] = useState(true);
  const [relays, setRelays] = useState<Relay[]>([]);
  const [isLoadingRelays, setIsLoadingRelays] = useState(true);

  // Load relays on mount
  useEffect(() => {
    const loadRelays = async () => {
      try {
        const data = await relayApi.list();
        setRelays(data);
        // If relays exist, default to the first active one
        const activeRelay = data.find((r) => r.status === 'active') || data[0];
        if (activeRelay) {
          setExitNode({ type: 'custom', address: activeRelay.address });
        }
      } catch (error) {
        console.error('Failed to load relays:', error);
      } finally {
        setIsLoadingRelays(false);
      }
    };
    loadRelays();
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    const config: TunnelConfig = {
      name,
      local_host: localHost,
      protocols,
      auth_token: authToken,
      exit_node: exitNode,
      failover,
    };

    console.log('🚀 [TunnelForm] Creating tunnel with config:', config);
    const startTime = performance.now();

    try {
      console.log('📡 [TunnelForm] Calling createTunnel mutation...');
      const result = await createTunnel.mutateAsync(config);
      const createTime = performance.now() - startTime;

      console.log(`✅ [TunnelForm] Tunnel created successfully (${createTime.toFixed(2)}ms):`, result);
      onSuccess();
      onClose();
    } catch (error) {
      const createTime = performance.now() - startTime;
      console.error(`❌ [TunnelForm] Failed to create tunnel (${createTime.toFixed(2)}ms):`, error);

      if (error instanceof Error) {
        console.error('Error message:', error.message);
        console.error('Error stack:', error.stack);
      }
    }
  };

  const addProtocol = () => {
    setProtocols([...protocols, { type: 'http', local_port: 3000, subdomain: undefined }]);
  };

  const removeProtocol = (index: number) => {
    setProtocols(protocols.filter((_, i) => i !== index));
  };

  const updateProtocol = (index: number, updates: Partial<ProtocolConfig>) => {
    setProtocols(
      protocols.map((p, i) =>
        i === index ? ({ ...p, ...updates } as ProtocolConfig) : p
      )
    );
  };

  return (
    <div className="fixed inset-0 bg-background/80 backdrop-blur-sm flex items-center justify-center z-50">
      <div className="bg-card border border-border rounded-lg shadow-xl max-w-2xl w-full max-h-[90vh] overflow-y-auto">
        <div className="sticky top-0 bg-card border-b border-border px-6 py-4 flex items-center justify-between">
          <h2 className="text-xl font-bold text-foreground">Create New Tunnel</h2>
          <button
            onClick={onClose}
            className="p-2 hover:bg-muted rounded-lg transition-colors text-muted-foreground hover:text-foreground"
          >
            <X size={20} />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="p-6 space-y-6">
          {/* Basic Info */}
          <div className="space-y-4">
            <h3 className="font-semibold text-foreground">Basic Information</h3>

            <div>
              <label className="block text-sm font-medium text-foreground mb-1">
                Tunnel Name
              </label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                required
                className="w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
                placeholder="My App Tunnel"
              />
            </div>

            <div>
              <label className="block text-sm font-medium text-foreground mb-1">
                Local Host
              </label>
              <input
                type="text"
                value={localHost}
                onChange={(e) => setLocalHost(e.target.value)}
                required
                className="w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
                placeholder="localhost"
              />
            </div>

            <div>
              <label className="block text-sm font-medium text-foreground mb-1">
                Auth Token
              </label>
              <input
                type="password"
                value={authToken}
                onChange={(e) => setAuthToken(e.target.value)}
                required
                className="w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
                placeholder="Your authentication token"
              />
            </div>
          </div>

          {/* Protocols */}
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="font-semibold text-foreground">Protocols</h3>
              <button
                type="button"
                onClick={addProtocol}
                className="px-3 py-1 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors flex items-center gap-1"
              >
                <Plus size={14} />
                Add Protocol
              </button>
            </div>

            {protocols.map((protocol, index) => (
              <div key={index} className="border border-border rounded-lg p-4 space-y-3 bg-muted/30">
                <div className="flex items-center justify-between">
                  <select
                    value={protocol.type}
                    onChange={(e) =>
                      updateProtocol(index, {
                        type: e.target.value as any,
                        local_port: protocol.local_port,
                      })
                    }
                    className="px-3 py-2 bg-background border border-border rounded-lg text-foreground focus:outline-none focus:ring-2 focus:ring-primary"
                  >
                    <option value="tcp">TCP</option>
                    <option value="tls">TLS</option>
                    <option value="http">HTTP</option>
                    <option value="https">HTTPS</option>
                  </select>

                  {protocols.length > 1 && (
                    <button
                      type="button"
                      onClick={() => removeProtocol(index)}
                      className="p-2 text-destructive hover:bg-destructive/10 rounded-lg transition-colors"
                    >
                      <Trash2 size={16} />
                    </button>
                  )}
                </div>

                <div>
                  <label className="block text-sm font-medium text-foreground mb-1">
                    Local Port
                  </label>
                  <input
                    type="number"
                    value={protocol.local_port}
                    onChange={(e) =>
                      updateProtocol(index, { local_port: parseInt(e.target.value) })
                    }
                    required
                    className="w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground focus:outline-none focus:ring-2 focus:ring-primary"
                    min="1"
                    max="65535"
                  />
                </div>

                {(protocol.type === 'http' || protocol.type === 'https' || protocol.type === 'tls') && (
                  <div>
                    <label className="block text-sm font-medium text-foreground mb-1">
                      Subdomain (optional)
                    </label>
                    <input
                      type="text"
                      value={'subdomain' in protocol ? protocol.subdomain || '' : ''}
                      onChange={(e) =>
                        updateProtocol(index, {
                          subdomain: e.target.value || undefined,
                        })
                      }
                      className="w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
                      placeholder="myapp"
                    />
                  </div>
                )}

                {protocol.type === 'https' && (
                  <div>
                    <label className="block text-sm font-medium text-foreground mb-1">
                      Custom Domain (optional)
                    </label>
                    <input
                      type="text"
                      value={'custom_domain' in protocol ? protocol.custom_domain || '' : ''}
                      onChange={(e) =>
                        updateProtocol(index, {
                          custom_domain: e.target.value || undefined,
                        })
                      }
                      className="w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
                      placeholder="example.com"
                    />
                  </div>
                )}
              </div>
            ))}
          </div>

          {/* Exit Node */}
          <div className="space-y-4">
            <h3 className="font-semibold text-foreground">Relay Server</h3>

            {isLoadingRelays ? (
              <div className="text-sm text-muted-foreground">Loading relays...</div>
            ) : relays.length === 0 ? (
              <div className="p-3 bg-muted/30 border border-border rounded-lg">
                <div className="flex items-start gap-2">
                  <AlertCircle size={16} className="text-destructive mt-0.5 flex-shrink-0" />
                  <div className="text-sm">
                    <p className="text-destructive font-medium">No relays configured</p>
                    <p className="text-muted-foreground mt-1">
                      You need to add a relay server before creating a tunnel. Go to the Relay page to add one.
                    </p>
                  </div>
                </div>
              </div>
            ) : (
              <div>
                <label className="block text-sm font-medium text-foreground mb-1">
                  Select Relay
                </label>
                <select
                  value={exitNode.type === 'custom' && 'address' in exitNode ? exitNode.address : ''}
                  onChange={(e) => setExitNode({ type: 'custom', address: e.target.value })}
                  required
                  className="w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground focus:outline-none focus:ring-2 focus:ring-primary"
                >
                  <option value="">Select a relay...</option>
                  {relays.map((relay) => (
                    <option key={relay.id} value={relay.address}>
                      {relay.name} ({relay.address}) - {relay.region}
                    </option>
                  ))}
                </select>
                {exitNode.type === 'custom' && 'address' in exitNode && exitNode.address && (
                  <p className="text-xs text-muted-foreground mt-1">
                    Will connect to: {exitNode.address}
                  </p>
                )}
              </div>
            )}

            <div className="flex items-center gap-2">
              <input
                type="checkbox"
                id="failover"
                checked={failover}
                onChange={(e) => setFailover(e.target.checked)}
                className="w-4 h-4 accent-primary border-border rounded-sm"
              />
              <label htmlFor="failover" className="text-sm text-foreground">
                Enable automatic failover
              </label>
            </div>
          </div>

          {/* Actions */}
          <div className="flex items-center justify-end gap-3 pt-4 border-t border-border">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 border border-border rounded-lg text-foreground hover:bg-muted transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={createTunnel.isPending || relays.length === 0 || (exitNode.type === 'custom' && !('address' in exitNode && exitNode.address))}
              className="px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {createTunnel.isPending ? 'Creating...' : 'Create Tunnel'}
            </button>
          </div>

          {createTunnel.isError && (
            <div className="p-3 bg-destructive/10 border border-destructive/20 rounded-lg">
              <p className="text-sm text-destructive">
                Failed to create tunnel: {String(createTunnel.error)}
              </p>
            </div>
          )}
        </form>
      </div>
    </div>
  );
}
