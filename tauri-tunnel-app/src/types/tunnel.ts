// Type definitions for tunnel management

export type TunnelStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

export interface TunnelInfo {
  id: string;
  name: string;
  status: TunnelStatus;
  config: TunnelConfig;
  endpoints: Endpoint[];
  created_at: number;
  connected_at?: number;
  error?: string;
}

export interface TunnelConfig {
  name: string;
  local_host: string;
  protocols: ProtocolConfig[];
  auth_token: string;
  exit_node: ExitNodeConfig;
  failover: boolean;
}

export type ProtocolConfig =
  | { type: 'tcp'; local_port: number; remote_port?: number }
  | { type: 'tls'; local_port: number; subdomain?: string; remote_port?: number }
  | { type: 'http'; local_port: number; subdomain?: string }
  | { type: 'https'; local_port: number; subdomain?: string; custom_domain?: string };

export type ExitNodeConfig =
  | { type: 'auto' }
  | { type: 'nearest' }
  | { type: 'specific'; region: Region }
  | { type: 'multi_region'; regions: Region[] }
  | { type: 'custom'; address: string };

export type Region =
  | 'us-east'
  | 'us-west'
  | 'eu-west'
  | 'eu-central'
  | 'asia-pacific'
  | 'south-america';

export interface Endpoint {
  protocol: string;
  public_url: string;
  port?: number;
}

export interface MetricsStats {
  total_requests: number;
  successful_requests: number;
  failed_requests: number;
  avg_duration_ms?: number;
  percentiles?: DurationPercentiles;
  methods: Record<string, number>;
  status_codes: Record<number, number>;
}

export interface DurationPercentiles {
  min: number;
  p50: number;
  p90: number;
  p95: number;
  p99: number;
  p999: number;
  max: number;
}

export interface HttpMetric {
  id: string;
  stream_id: string;
  timestamp: number;
  method: string;
  uri: string;
  request_headers: [string, string][];
  request_body?: BodyData;
  response_status?: number;
  response_headers?: [string, string][];
  response_body?: BodyData;
  duration_ms?: number;
  error?: string;
}

export interface TcpMetric {
  id: string;
  stream_id: string;
  timestamp: number;
  remote_addr: string;
  local_addr: string;
  state: 'active' | 'closed' | 'error';
  bytes_received: number;
  bytes_sent: number;
  duration_ms?: number;
  closed_at?: number;
  error?: string;
}

export interface BodyData {
  content_type: string;
  size: number;
  data: BodyContent;
}

export type BodyContent =
  | { type: 'Json'; value: any }
  | { type: 'Text'; value: string }
  | { type: 'Binary'; value: { size: number } };
