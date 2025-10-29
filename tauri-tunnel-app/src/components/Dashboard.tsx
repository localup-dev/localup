import { Activity, Server, TrendingUp, Zap } from 'lucide-react';

export function Dashboard() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold text-foreground">Dashboard</h1>
        <p className="text-muted-foreground mt-1">Overview of your tunnel system</p>
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <StatCard
          icon={<Activity className="text-primary" size={24} />}
          label="Active Tunnels"
          value="0"
          change="+0%"
        />
        <StatCard
          icon={<Server className="text-primary" size={24} />}
          label="Connected Relays"
          value="0"
          change="+0%"
        />
        <StatCard
          icon={<TrendingUp className="text-primary" size={24} />}
          label="Total Requests"
          value="0"
          change="+0%"
        />
        <StatCard
          icon={<Zap className="text-primary" size={24} />}
          label="Avg Response Time"
          value="0ms"
          change="-0%"
        />
      </div>

      {/* Recent Activity */}
      <div className="bg-card border border-border rounded-lg p-6">
        <h2 className="text-lg font-semibold text-foreground mb-4">Recent Activity</h2>
        <div className="text-center py-12">
          <p className="text-muted-foreground">No recent activity</p>
          <p className="text-sm text-muted-foreground mt-2">
            Start a tunnel to see activity here
          </p>
        </div>
      </div>
    </div>
  );
}

interface StatCardProps {
  icon: React.ReactNode;
  label: string;
  value: string;
  change: string;
}

function StatCard({ icon, label, value, change }: StatCardProps) {
  return (
    <div className="bg-card border border-border rounded-lg p-6">
      <div className="flex items-start justify-between mb-4">
        {icon}
        <span className="text-xs text-primary font-medium">{change}</span>
      </div>
      <div className="text-3xl font-bold text-foreground mb-1">{value}</div>
      <div className="text-sm text-muted-foreground">{label}</div>
    </div>
  );
}
