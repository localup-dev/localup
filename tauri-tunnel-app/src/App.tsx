import { useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Sidebar } from './components/Sidebar';
import { Dashboard } from './components/Dashboard';
import { Relay } from './components/Relay';
import { TunnelList } from './components/TunnelList';
import { TunnelDetail } from './components/TunnelDetail';
import { TunnelForm } from './components/TunnelForm';
import { Activity, Plus } from 'lucide-react';
import './App.css';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <AppContent />
    </QueryClientProvider>
  );
}

function AppContent() {
  const [activeView, setActiveView] = useState<'dashboard' | 'relay' | 'tunnels'>('dashboard');
  const [showTunnelForm, setShowTunnelForm] = useState(false);
  const [selectedTunnelId, setSelectedTunnelId] = useState<string | null>(null);

  return (
    <div className="flex h-screen bg-background">
      {/* Sidebar */}
      <Sidebar activeView={activeView} onViewChange={setActiveView} />

      {/* Main Content */}
      <main className="flex-1 overflow-y-auto">
        <div className="max-w-7xl mx-auto p-8">
          {activeView === 'dashboard' && <Dashboard />}
          {activeView === 'relay' && <Relay />}
          {activeView === 'tunnels' && (
            <div className="space-y-6">
              <div className="flex items-center justify-between">
                <div>
                  <h1 className="text-3xl font-bold text-foreground">Tunnels</h1>
                  <p className="text-muted-foreground mt-1">Manage and monitor your tunnels</p>
                </div>
                <button
                  onClick={() => setShowTunnelForm(true)}
                  className="px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors flex items-center gap-2"
                >
                  <Plus size={16} />
                  New Tunnel
                </button>
              </div>

              <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
                {/* Left - Tunnel List */}
                <div className="lg:col-span-1">
                  <div className="bg-card border border-border rounded-lg p-6">
                    <h2 className="text-lg font-semibold text-foreground mb-4">All Tunnels</h2>
                    <TunnelList
                      onSelectTunnel={setSelectedTunnelId}
                      onCreateTunnel={() => setShowTunnelForm(true)}
                    />
                  </div>
                </div>

                {/* Right - Tunnel Details */}
                <div className="lg:col-span-2">
                  {selectedTunnelId ? (
                    <TunnelDetail tunnelId={selectedTunnelId} />
                  ) : (
                    <div className="bg-card border border-border rounded-lg p-12 text-center">
                      <div className="w-16 h-16 bg-muted rounded-full flex items-center justify-center mx-auto mb-4">
                        <Activity size={32} className="text-muted-foreground" />
                      </div>
                      <h3 className="text-lg font-semibold text-foreground mb-2">
                        No Tunnel Selected
                      </h3>
                      <p className="text-muted-foreground">
                        Select a tunnel from the list to view details, metrics and traffic
                      </p>
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}
        </div>
      </main>

      {/* Tunnel Creation Modal */}
      {showTunnelForm && (
        <TunnelForm
          onClose={() => setShowTunnelForm(false)}
          onSuccess={() => {
            setShowTunnelForm(false);
          }}
        />
      )}
    </div>
  );
}

export default App;
