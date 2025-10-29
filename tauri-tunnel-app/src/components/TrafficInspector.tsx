import { useState } from 'react';
import { useTunnelRequests, useTunnelTcpConnections } from '../hooks/useTunnels';
import { ChevronRight, ChevronDown, Copy, Activity } from 'lucide-react';
import type { HttpMetric, TcpMetric } from '../types/tunnel';

interface TrafficInspectorProps {
  tunnelId: string;
}

export function TrafficInspector({ tunnelId }: TrafficInspectorProps) {
  const [tab, setTab] = useState<'http' | 'tcp'>('http');
  const [selectedRequest, setSelectedRequest] = useState<string | null>(null);

  return (
    <div className="space-y-4">
      {/* Tabs */}
      <div className="flex gap-2 border-b">
        <button
          onClick={() => setTab('http')}
          className={`px-4 py-2 font-medium transition-colors ${
            tab === 'http'
              ? 'border-b-2 border-blue-600 text-blue-600'
              : 'text-gray-600 hover:text-gray-900'
          }`}
        >
          HTTP Requests
        </button>
        <button
          onClick={() => setTab('tcp')}
          className={`px-4 py-2 font-medium transition-colors ${
            tab === 'tcp'
              ? 'border-b-2 border-blue-600 text-blue-600'
              : 'text-gray-600 hover:text-gray-900'
          }`}
        >
          TCP Connections
        </button>
      </div>

      {/* Content */}
      {tab === 'http' ? (
        <HttpRequestsTab
          tunnelId={tunnelId}
          selectedRequest={selectedRequest}
          onSelectRequest={setSelectedRequest}
        />
      ) : (
        <TcpConnectionsTab tunnelId={tunnelId} />
      )}
    </div>
  );
}

interface HttpRequestsTabProps {
  tunnelId: string;
  selectedRequest: string | null;
  onSelectRequest: (id: string | null) => void;
}

function HttpRequestsTab({ tunnelId, selectedRequest, onSelectRequest }: HttpRequestsTabProps) {
  const { data: requests = [] } = useTunnelRequests(tunnelId, 0, 100);

  if (requests.length === 0) {
    return (
      <div className="bg-gray-50 rounded-lg p-8 text-center">
        <Activity className="mx-auto text-gray-400 mb-2" size={48} />
        <p className="text-gray-600">No HTTP requests captured yet</p>
      </div>
    );
  }

  const selected = requests.find((r) => r.id === selectedRequest);

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
      {/* Request List */}
      <div className="bg-white rounded-lg shadow-xs overflow-hidden">
        <div className="p-4 border-b bg-gray-50">
          <h3 className="font-semibold">Requests</h3>
        </div>
        <div className="overflow-y-auto max-h-[600px]">
          {requests.map((request) => (
            <RequestItem
              key={request.id}
              request={request}
              isSelected={selectedRequest === request.id}
              onSelect={() => onSelectRequest(request.id)}
            />
          ))}
        </div>
      </div>

      {/* Request Details */}
      <div className="bg-white rounded-lg shadow-xs overflow-hidden">
        <div className="p-4 border-b bg-gray-50">
          <h3 className="font-semibold">Details</h3>
        </div>
        <div className="overflow-y-auto max-h-[600px] p-4">
          {selected ? <RequestDetails request={selected} /> : <p className="text-gray-500">Select a request to view details</p>}
        </div>
      </div>
    </div>
  );
}

function RequestItem({
  request,
  isSelected,
  onSelect,
}: {
  request: HttpMetric;
  isSelected: boolean;
  onSelect: () => void;
}) {
  const statusColor = request.response_status
    ? request.response_status < 300
      ? 'text-green-600'
      : request.response_status < 400
      ? 'text-blue-600'
      : request.response_status < 500
      ? 'text-yellow-600'
      : 'text-red-600'
    : 'text-gray-600';

  return (
    <div
      onClick={onSelect}
      className={`p-4 border-b cursor-pointer hover:bg-gray-50 transition-colors ${
        isSelected ? 'bg-blue-50 border-blue-200' : ''
      }`}
    >
      <div className="flex items-start justify-between mb-2">
        <div className="flex items-center gap-2">
          <span className="font-mono text-xs font-bold px-2 py-1 bg-gray-200 rounded-sm">
            {request.method}
          </span>
          <span className={`font-mono text-xs font-bold ${statusColor}`}>
            {request.response_status || '...'}
          </span>
        </div>
        <span className="text-xs text-gray-500">
          {request.duration_ms ? `${request.duration_ms}ms` : 'pending'}
        </span>
      </div>
      <div className="text-sm text-gray-800 truncate">{request.uri}</div>
      <div className="text-xs text-gray-500 mt-1">
        {new Date(request.timestamp).toLocaleTimeString()}
      </div>
    </div>
  );
}

function RequestDetails({ request }: { request: HttpMetric }) {
  const [expandedSections, setExpandedSections] = useState<Set<string>>(
    new Set(['request-headers'])
  );

  const toggleSection = (section: string) => {
    const newSet = new Set(expandedSections);
    if (newSet.has(section)) {
      newSet.delete(section);
    } else {
      newSet.add(section);
    }
    setExpandedSections(newSet);
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
  };

  return (
    <div className="space-y-4">
      {/* Overview */}
      <div>
        <div className="flex items-center gap-2 mb-2">
          <span className="font-mono text-sm font-bold px-2 py-1 bg-gray-200 rounded-sm">
            {request.method}
          </span>
          <span className="font-mono text-sm">{request.uri}</span>
        </div>
        {request.response_status && (
          <div className="text-sm text-gray-600">
            Status: <span className="font-bold">{request.response_status}</span>
            {request.duration_ms && ` • ${request.duration_ms}ms`}
          </div>
        )}
      </div>

      {/* Request Headers */}
      <Section
        title="Request Headers"
        expanded={expandedSections.has('request-headers')}
        onToggle={() => toggleSection('request-headers')}
      >
        <div className="space-y-1">
          {request.request_headers.map(([key, value], idx) => (
            <div key={idx} className="flex items-start text-sm">
              <span className="font-medium text-gray-700 min-w-[120px]">{key}:</span>
              <span className="text-gray-600 break-all">{value}</span>
            </div>
          ))}
        </div>
      </Section>

      {/* Request Body */}
      {request.request_body && (
        <Section
          title="Request Body"
          expanded={expandedSections.has('request-body')}
          onToggle={() => toggleSection('request-body')}
        >
          <BodyContent body={request.request_body} onCopy={copyToClipboard} />
        </Section>
      )}

      {/* Response Headers */}
      {request.response_headers && (
        <Section
          title="Response Headers"
          expanded={expandedSections.has('response-headers')}
          onToggle={() => toggleSection('response-headers')}
        >
          <div className="space-y-1">
            {request.response_headers.map(([key, value], idx) => (
              <div key={idx} className="flex items-start text-sm">
                <span className="font-medium text-gray-700 min-w-[120px]">{key}:</span>
                <span className="text-gray-600 break-all">{value}</span>
              </div>
            ))}
          </div>
        </Section>
      )}

      {/* Response Body */}
      {request.response_body && (
        <Section
          title="Response Body"
          expanded={expandedSections.has('response-body')}
          onToggle={() => toggleSection('response-body')}
        >
          <BodyContent body={request.response_body} onCopy={copyToClipboard} />
        </Section>
      )}

      {/* Error */}
      {request.error && (
        <div className="p-3 bg-red-50 border border-red-200 rounded-lg">
          <p className="text-sm font-medium text-red-800">Error:</p>
          <p className="text-sm text-red-700 mt-1">{request.error}</p>
        </div>
      )}
    </div>
  );
}

function Section({
  title,
  expanded,
  onToggle,
  children,
}: {
  title: string;
  expanded: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="border rounded-lg">
      <button
        onClick={onToggle}
        className="w-full px-4 py-2 flex items-center justify-between bg-gray-50 hover:bg-gray-100 transition-colors"
      >
        <span className="font-medium text-sm">{title}</span>
        {expanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
      </button>
      {expanded && <div className="p-4">{children}</div>}
    </div>
  );
}

function BodyContent({
  body,
  onCopy,
}: {
  body: any;
  onCopy: (text: string) => void;
}) {
  const content =
    body.data.type === 'Json'
      ? JSON.stringify(body.data.value, null, 2)
      : body.data.type === 'Text'
      ? body.data.value
      : `[Binary data: ${body.size} bytes]`;

  return (
    <div className="relative">
      <button
        onClick={() => onCopy(content)}
        className="absolute top-2 right-2 p-2 bg-gray-100 hover:bg-gray-200 rounded-lg transition-colors"
        title="Copy to clipboard"
      >
        <Copy size={14} />
      </button>
      <pre className="bg-gray-50 p-4 rounded-lg text-xs overflow-x-auto">
        {content}
      </pre>
    </div>
  );
}

function TcpConnectionsTab({ tunnelId }: { tunnelId: string }) {
  const { data: connections = [] } = useTunnelTcpConnections(tunnelId, 0, 100);

  if (connections.length === 0) {
    return (
      <div className="bg-gray-50 rounded-lg p-8 text-center">
        <Activity className="mx-auto text-gray-400 mb-2" size={48} />
        <p className="text-gray-600">No TCP connections captured yet</p>
      </div>
    );
  }

  return (
    <div className="bg-white rounded-lg shadow-xs overflow-hidden">
      <div className="overflow-x-auto">
        <table className="w-full">
          <thead className="bg-gray-50 border-b">
            <tr>
              <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">
                Remote Address
              </th>
              <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">
                State
              </th>
              <th className="px-4 py-3 text-right text-xs font-medium text-gray-500 uppercase">
                Received
              </th>
              <th className="px-4 py-3 text-right text-xs font-medium text-gray-500 uppercase">
                Sent
              </th>
              <th className="px-4 py-3 text-right text-xs font-medium text-gray-500 uppercase">
                Duration
              </th>
            </tr>
          </thead>
          <tbody className="divide-y">
            {connections.map((conn) => (
              <TcpConnectionRow key={conn.id} connection={conn} />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function TcpConnectionRow({ connection }: { connection: TcpMetric }) {
  const stateColors = {
    active: 'bg-green-100 text-green-800',
    closed: 'bg-gray-100 text-gray-800',
    error: 'bg-red-100 text-red-800',
  };

  const formatBytes = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  return (
    <tr className="hover:bg-gray-50">
      <td className="px-4 py-3 text-sm text-gray-900">{connection.remote_addr}</td>
      <td className="px-4 py-3">
        <span
          className={`px-2 py-1 text-xs font-medium rounded-full ${
            stateColors[connection.state]
          }`}
        >
          {connection.state}
        </span>
      </td>
      <td className="px-4 py-3 text-sm text-gray-600 text-right">
        {formatBytes(connection.bytes_received)}
      </td>
      <td className="px-4 py-3 text-sm text-gray-600 text-right">
        {formatBytes(connection.bytes_sent)}
      </td>
      <td className="px-4 py-3 text-sm text-gray-600 text-right">
        {connection.duration_ms ? `${connection.duration_ms}ms` : '—'}
      </td>
    </tr>
  );
}
