import { useTunnelMetrics } from '../hooks/useTunnels';
import { Activity, CheckCircle, XCircle, Clock, TrendingUp } from 'lucide-react';

interface MetricsDashboardProps {
  tunnelId: string;
}

export function MetricsDashboard({ tunnelId }: MetricsDashboardProps) {
  const { data: metrics } = useTunnelMetrics(tunnelId);

  if (!metrics) {
    return (
      <div className="bg-white rounded-lg shadow-xs p-6">
        <p className="text-gray-500">No metrics available</p>
      </div>
    );
  }

  const successRate = metrics.total_requests > 0
    ? ((metrics.successful_requests / metrics.total_requests) * 100).toFixed(1)
    : '0.0';

  return (
    <div className="space-y-6">
      {/* Overview Stats */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <StatCard
          icon={<Activity className="text-blue-600" size={24} />}
          label="Total Requests"
          value={metrics.total_requests.toString()}
          color="blue"
        />
        <StatCard
          icon={<CheckCircle className="text-green-600" size={24} />}
          label="Successful"
          value={metrics.successful_requests.toString()}
          color="green"
          subtitle={`${successRate}% success rate`}
        />
        <StatCard
          icon={<XCircle className="text-red-600" size={24} />}
          label="Failed"
          value={metrics.failed_requests.toString()}
          color="red"
        />
        <StatCard
          icon={<Clock className="text-purple-600" size={24} />}
          label="Avg Duration"
          value={metrics.avg_duration_ms ? `${metrics.avg_duration_ms}ms` : 'N/A'}
          color="purple"
        />
      </div>

      {/* Percentiles */}
      {metrics.percentiles && (
        <div className="bg-white rounded-lg shadow-xs p-6">
          <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
            <TrendingUp size={20} />
            Response Time Percentiles
          </h3>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <PercentileItem label="p50 (median)" value={metrics.percentiles.p50} />
            <PercentileItem label="p90" value={metrics.percentiles.p90} />
            <PercentileItem label="p95" value={metrics.percentiles.p95} />
            <PercentileItem label="p99" value={metrics.percentiles.p99} />
            <PercentileItem label="Min" value={metrics.percentiles.min} />
            <PercentileItem label="Max" value={metrics.percentiles.max} />
            <PercentileItem label="p99.9" value={metrics.percentiles.p999} />
          </div>
        </div>
      )}

      {/* Methods Breakdown */}
      {Object.keys(metrics.methods).length > 0 && (
        <div className="bg-white rounded-lg shadow-xs p-6">
          <h3 className="text-lg font-semibold mb-4">HTTP Methods</h3>
          <div className="space-y-2">
            {Object.entries(metrics.methods)
              .sort(([, a], [, b]) => b - a)
              .map(([method, count]) => (
                <div key={method} className="flex items-center justify-between">
                  <span className="font-mono text-sm font-medium">{method}</span>
                  <div className="flex items-center gap-3">
                    <div className="w-32 bg-gray-200 rounded-full h-2">
                      <div
                        className="bg-blue-600 h-2 rounded-full"
                        style={{
                          width: `${(count / metrics.total_requests) * 100}%`,
                        }}
                      />
                    </div>
                    <span className="text-sm text-gray-600 w-12 text-right">{count}</span>
                  </div>
                </div>
              ))}
          </div>
        </div>
      )}

      {/* Status Codes */}
      {Object.keys(metrics.status_codes).length > 0 && (
        <div className="bg-white rounded-lg shadow-xs p-6">
          <h3 className="text-lg font-semibold mb-4">HTTP Status Codes</h3>
          <div className="space-y-2">
            {Object.entries(metrics.status_codes)
              .sort(([a], [b]) => parseInt(a) - parseInt(b))
              .map(([status, count]) => {
                const statusNum = parseInt(status);
                const color =
                  statusNum < 300
                    ? 'bg-green-600'
                    : statusNum < 400
                    ? 'bg-blue-600'
                    : statusNum < 500
                    ? 'bg-yellow-600'
                    : 'bg-red-600';

                return (
                  <div key={status} className="flex items-center justify-between">
                    <span className="font-mono text-sm font-medium">{status}</span>
                    <div className="flex items-center gap-3">
                      <div className="w-32 bg-gray-200 rounded-full h-2">
                        <div
                          className={`${color} h-2 rounded-full`}
                          style={{
                            width: `${(count / metrics.total_requests) * 100}%`,
                          }}
                        />
                      </div>
                      <span className="text-sm text-gray-600 w-12 text-right">{count}</span>
                    </div>
                  </div>
                );
              })}
          </div>
        </div>
      )}
    </div>
  );
}

interface StatCardProps {
  icon: React.ReactNode;
  label: string;
  value: string;
  color: 'blue' | 'green' | 'red' | 'purple';
  subtitle?: string;
}

function StatCard({ icon, label, value, color, subtitle }: StatCardProps) {
  const bgColors = {
    blue: 'bg-blue-50',
    green: 'bg-green-50',
    red: 'bg-red-50',
    purple: 'bg-purple-50',
  };

  return (
    <div className={`${bgColors[color]} rounded-lg p-6`}>
      <div className="flex items-start justify-between mb-2">
        {icon}
      </div>
      <div className="text-2xl font-bold mb-1">{value}</div>
      <div className="text-sm text-gray-600">{label}</div>
      {subtitle && <div className="text-xs text-gray-500 mt-1">{subtitle}</div>}
    </div>
  );
}

function PercentileItem({ label, value }: { label: string; value: number }) {
  return (
    <div className="text-center">
      <div className="text-lg font-semibold">{value}ms</div>
      <div className="text-xs text-gray-600">{label}</div>
    </div>
  );
}
