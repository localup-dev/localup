import { useEffect, useState, useCallback, useRef } from "react";
import { useParams, useNavigate } from "react-router-dom";
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
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  ArrowLeft,
  Copy,
  ExternalLink,
  Play,
  Square,
  RefreshCw,
  CheckCircle2,
  AlertCircle,
  Loader2,
  WifiOff,
  Globe,
  Clock,
  Activity,
  ArrowDownToLine,
  ArrowUpFromLine,
  Trash2,
  ChevronLeft,
  ChevronRight,
  RotateCcw,
} from "lucide-react";
import { toast } from "sonner";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  getTunnel,
  startTunnel,
  stopTunnel,
  getTunnelMetrics,
  clearTunnelMetrics,
  subscribeToMetrics,
  replayRequest,
  type Tunnel,
  type HttpMetric,
  type TunnelMetricsPayload,
  type BodyData,
  type ReplayResponse,
} from "@/api/tunnels";

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
          <WifiOff className="h-3 w-3 mr-1" />
          Disconnected
        </Badge>
      );
  }
}

function getMethodBadge(method: string) {
  const colors: Record<string, string> = {
    GET: "bg-blue-500/10 text-blue-500",
    POST: "bg-green-500/10 text-green-500",
    PUT: "bg-yellow-500/10 text-yellow-500",
    PATCH: "bg-orange-500/10 text-orange-500",
    DELETE: "bg-red-500/10 text-red-500",
    OPTIONS: "bg-purple-500/10 text-purple-500",
    HEAD: "bg-gray-500/10 text-gray-500",
  };
  return (
    <Badge className={colors[method.toUpperCase()] || "bg-gray-500/10 text-gray-500"}>
      {method.toUpperCase()}
    </Badge>
  );
}

function getStatusCodeBadge(status: number | null) {
  if (!status) return <Badge variant="secondary">Pending</Badge>;

  if (status >= 200 && status < 300) {
    return <Badge className="bg-green-500/10 text-green-500">{status}</Badge>;
  } else if (status >= 300 && status < 400) {
    return <Badge className="bg-blue-500/10 text-blue-500">{status}</Badge>;
  } else if (status >= 400 && status < 500) {
    return <Badge className="bg-yellow-500/10 text-yellow-500">{status}</Badge>;
  } else if (status >= 500) {
    return <Badge className="bg-red-500/10 text-red-500">{status}</Badge>;
  }
  return <Badge variant="secondary">{status}</Badge>;
}

function formatTimestamp(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString();
}

function formatBodyData(body: BodyData | null): string {
  if (!body) return "";

  switch (body.data.type) {
    case "Json":
      return JSON.stringify(body.data.value, null, 2);
    case "Text":
      return body.data.value;
    case "Binary":
      return `[Binary data: ${body.data.value.size} bytes]`;
    default:
      return "";
  }
}

function headersToObject(headers: [string, string][]): Record<string, string> {
  const obj: Record<string, string> = {};
  for (const [key, value] of headers) {
    obj[key] = value;
  }
  return obj;
}

function RequestDetailDialog({
  request,
  open,
  onOpenChange,
  tunnelId,
}: {
  request: HttpMetric | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  tunnelId: string;
}) {
  const [replaying, setReplaying] = useState(false);
  const [replayResult, setReplayResult] = useState<ReplayResponse | null>(null);

  if (!request) return null;

  const requestHeaders = headersToObject(request.request_headers);
  const responseHeaders = request.response_headers
    ? headersToObject(request.response_headers)
    : {};

  const handleReplay = async () => {
    setReplaying(true);
    setReplayResult(null);
    try {
      // Extract body text from BodyData if present
      let bodyText: string | null = null;
      if (request.request_body) {
        bodyText = formatBodyData(request.request_body);
      }

      const result = await replayRequest(tunnelId, {
        method: request.method,
        uri: request.uri,
        headers: request.request_headers,
        body: bodyText,
      });
      setReplayResult(result);
      toast.success(`Replay completed: ${result.status} (${result.duration_ms}ms)`);
    } catch (error) {
      toast.error("Replay failed", { description: String(error) });
    } finally {
      setReplaying(false);
    }
  };

  const formatJsonBody = (body: string | null | undefined): string => {
    if (!body) return "";
    try {
      const parsed = JSON.parse(body);
      return JSON.stringify(parsed, null, 2);
    } catch {
      return body;
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[90vh]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {getMethodBadge(request.method)}
            <span className="font-mono text-sm truncate flex-1">{request.uri}</span>
            <Button
              variant="outline"
              size="sm"
              onClick={handleReplay}
              disabled={replaying}
            >
              {replaying ? (
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              ) : (
                <RotateCcw className="h-4 w-4 mr-2" />
              )}
              Replay
            </Button>
          </DialogTitle>
          <DialogDescription>
            <span className="font-mono">{requestHeaders["host"] || requestHeaders["Host"] || ""}</span>
            {request.duration_ms && <span className="ml-2">({request.duration_ms}ms)</span>}
          </DialogDescription>
        </DialogHeader>
        <Tabs defaultValue={replayResult ? "replay" : "request"} className="w-full">
          <TabsList className={`grid w-full ${replayResult ? "grid-cols-3" : "grid-cols-2"}`}>
            <TabsTrigger value="request">
              <ArrowUpFromLine className="h-4 w-4 mr-2" />
              Request
            </TabsTrigger>
            <TabsTrigger value="response">
              <ArrowDownToLine className="h-4 w-4 mr-2" />
              Response {request.response_status && getStatusCodeBadge(request.response_status)}
            </TabsTrigger>
            {replayResult && (
              <TabsTrigger value="replay">
                <RotateCcw className="h-4 w-4 mr-2" />
                Replay {getStatusCodeBadge(replayResult.status)}
              </TabsTrigger>
            )}
          </TabsList>
          <TabsContent value="request" className="space-y-4">
            <div>
              <h4 className="text-sm font-medium mb-2">Headers</h4>
              <ScrollArea className="h-40 rounded-md border p-3 bg-muted/30">
                <pre className="text-xs font-mono whitespace-pre-wrap">
                  {JSON.stringify(requestHeaders, null, 2)}
                </pre>
              </ScrollArea>
            </div>
            {request.request_body && (
              <div>
                <h4 className="text-sm font-medium mb-2">
                  Body
                  <span className="text-muted-foreground ml-2">
                    ({request.request_body.content_type}, {request.request_body.size} bytes)
                  </span>
                </h4>
                <ScrollArea className="h-48 rounded-md border p-3 bg-muted/30">
                  <pre className="text-xs font-mono whitespace-pre-wrap">
                    {formatBodyData(request.request_body)}
                  </pre>
                </ScrollArea>
              </div>
            )}
          </TabsContent>
          <TabsContent value="response" className="space-y-4">
            <div>
              <h4 className="text-sm font-medium mb-2">Status</h4>
              <div className="flex items-center gap-2">
                {getStatusCodeBadge(request.response_status)}
                {request.duration_ms && (
                  <span className="text-sm text-muted-foreground">
                    {request.duration_ms}ms
                  </span>
                )}
                {request.error && (
                  <Badge className="bg-red-500/10 text-red-500">{request.error}</Badge>
                )}
              </div>
            </div>
            <div>
              <h4 className="text-sm font-medium mb-2">Headers</h4>
              <ScrollArea className="h-32 rounded-md border p-3 bg-muted/30">
                <pre className="text-xs font-mono whitespace-pre-wrap">
                  {JSON.stringify(responseHeaders, null, 2)}
                </pre>
              </ScrollArea>
            </div>
            {request.response_body && (
              <div>
                <h4 className="text-sm font-medium mb-2">
                  Body
                  <span className="text-muted-foreground ml-2">
                    ({request.response_body.content_type}, {request.response_body.size} bytes)
                  </span>
                </h4>
                <ScrollArea className="h-64 rounded-md border p-3 bg-muted/30">
                  <pre className="text-xs font-mono whitespace-pre-wrap">
                    {formatBodyData(request.response_body)}
                  </pre>
                </ScrollArea>
              </div>
            )}
          </TabsContent>
          {replayResult && (
            <TabsContent value="replay" className="space-y-4">
              <div>
                <h4 className="text-sm font-medium mb-2">Status</h4>
                <div className="flex items-center gap-2">
                  {getStatusCodeBadge(replayResult.status)}
                  <span className="text-sm text-muted-foreground">
                    {replayResult.duration_ms}ms
                  </span>
                </div>
              </div>
              <div>
                <h4 className="text-sm font-medium mb-2">Headers</h4>
                <ScrollArea className="h-32 rounded-md border p-3 bg-muted/30">
                  <pre className="text-xs font-mono whitespace-pre-wrap">
                    {JSON.stringify(headersToObject(replayResult.headers), null, 2)}
                  </pre>
                </ScrollArea>
              </div>
              {replayResult.body && (
                <div>
                  <h4 className="text-sm font-medium mb-2">Body</h4>
                  <ScrollArea className="h-64 rounded-md border p-3 bg-muted/30">
                    <pre className="text-xs font-mono whitespace-pre-wrap">
                      {formatJsonBody(replayResult.body)}
                    </pre>
                  </ScrollArea>
                </div>
              )}
            </TabsContent>
          )}
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}

const ITEMS_PER_PAGE = 20;

export function TunnelDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [tunnel, setTunnel] = useState<Tunnel | null>(null);
  const [metrics, setMetrics] = useState<HttpMetric[]>([]);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [selectedRequest, setSelectedRequest] = useState<HttpMetric | null>(null);
  const [detailDialogOpen, setDetailDialogOpen] = useState(false);
  const [currentPage, setCurrentPage] = useState(1);
  const metricsRef = useRef<Map<string, HttpMetric>>(new Map());

  const loadData = useCallback(async () => {
    if (!id) return;
    try {
      const tunnelData = await getTunnel(id);
      if (!tunnelData) {
        toast.error("Tunnel not found");
        navigate("/tunnels");
        return;
      }
      setTunnel(tunnelData);

      // Load metrics from in-memory store
      const metricsData = await getTunnelMetrics(id);
      metricsData.forEach(m => metricsRef.current.set(m.id, m));
      setMetrics(metricsData);
    } catch (error) {
      toast.error("Failed to load tunnel", {
        description: String(error),
      });
    } finally {
      setLoading(false);
    }
  }, [id, navigate]);

  useEffect(() => {
    loadData();

    // Poll for tunnel status updates every 2 seconds
    const interval = setInterval(async () => {
      if (!id) return;
      try {
        const tunnelData = await getTunnel(id);
        if (tunnelData) {
          setTunnel(tunnelData);
        }
      } catch {
        // Silently ignore polling errors
      }
    }, 2000);

    return () => clearInterval(interval);
  }, [id, loadData]);

  // Subscribe to real-time metrics events
  useEffect(() => {
    if (!id) return;

    let unlisten: (() => void) | null = null;

    const subscribe = async () => {
      unlisten = await subscribeToMetrics((payload: TunnelMetricsPayload) => {
        // Only process events for this tunnel
        if (payload.tunnel_id !== id) return;

        const event = payload.event;
        if (event.type === "request") {
          // New request - add to map and update state
          metricsRef.current.set(event.metric.id, event.metric);
          setMetrics(Array.from(metricsRef.current.values()).sort((a, b) => b.timestamp - a.timestamp));
        } else if (event.type === "response") {
          // Update existing request with response data
          const existing = metricsRef.current.get(event.id);
          if (existing) {
            const updated: HttpMetric = {
              ...existing,
              response_status: event.status,
              response_headers: event.headers,
              response_body: event.body,
              duration_ms: event.duration_ms,
            };
            metricsRef.current.set(event.id, updated);
            setMetrics(Array.from(metricsRef.current.values()).sort((a, b) => b.timestamp - a.timestamp));
          }
        } else if (event.type === "error") {
          // Update existing request with error
          const existing = metricsRef.current.get(event.id);
          if (existing) {
            const updated: HttpMetric = {
              ...existing,
              error: event.error,
              duration_ms: event.duration_ms,
            };
            metricsRef.current.set(event.id, updated);
            setMetrics(Array.from(metricsRef.current.values()).sort((a, b) => b.timestamp - a.timestamp));
          }
        }
      });
    };

    subscribe();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, [id]);

  const handleStartTunnel = async () => {
    if (!tunnel) return;
    setActionLoading("start");
    try {
      const updated = await startTunnel(tunnel.id);
      setTunnel(updated);
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

  const handleStopTunnel = async () => {
    if (!tunnel) return;
    setActionLoading("stop");
    try {
      const updated = await stopTunnel(tunnel.id);
      setTunnel(updated);
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

  const openRequestDetail = (request: HttpMetric) => {
    setSelectedRequest(request);
    setDetailDialogOpen(true);
  };

  const handleClearMetrics = async () => {
    if (!id) return;
    try {
      await clearTunnelMetrics(id);
      metricsRef.current.clear();
      setMetrics([]);
      toast.success("Metrics cleared");
    } catch (error) {
      toast.error("Failed to clear metrics", {
        description: String(error),
      });
    }
  };

  const openInBrowser = async (url: string) => {
    try {
      await openUrl(url);
    } catch (error) {
      toast.error("Failed to open URL", {
        description: String(error),
      });
    }
  };

  if (loading) {
    return (
      <div className="space-y-6">
        <div className="flex items-center gap-4">
          <Skeleton className="h-10 w-10" />
          <div className="space-y-2">
            <Skeleton className="h-8 w-48" />
            <Skeleton className="h-4 w-64" />
          </div>
        </div>
        <div className="grid gap-4 md:grid-cols-3">
          {[1, 2, 3].map((i) => (
            <Card key={i}>
              <CardHeader className="pb-2">
                <Skeleton className="h-4 w-24" />
              </CardHeader>
              <CardContent>
                <Skeleton className="h-8 w-32" />
              </CardContent>
            </Card>
          ))}
        </div>
      </div>
    );
  }

  if (!tunnel) {
    return (
      <div className="flex flex-col items-center justify-center h-64 space-y-4">
        <AlertCircle className="h-12 w-12 text-muted-foreground" />
        <p className="text-lg text-muted-foreground">Tunnel not found</p>
        <Button onClick={() => navigate("/tunnels")}>Back to Tunnels</Button>
      </div>
    );
  }

  const isConnected = tunnel.status.toLowerCase() === "connected";
  const isConnecting = tunnel.status.toLowerCase() === "connecting";
  const isRunning = isConnected || isConnecting;

  // Calculate stats from metrics
  const totalRequests = metrics.length;
  const avgLatency = totalRequests > 0
    ? Math.round(metrics.reduce((sum, r) => sum + (r.duration_ms || 0), 0) / totalRequests)
    : 0;
  const errorRequests = metrics.filter((r) => r.response_status && r.response_status >= 400).length;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-start justify-between">
        <div className="flex items-center gap-4">
          <Button
            variant="ghost"
            size="icon"
            onClick={() => navigate("/tunnels")}
          >
            <ArrowLeft className="h-5 w-5" />
          </Button>
          <div>
            <div className="flex items-center gap-3">
              <h1 className="text-2xl font-bold">{tunnel.name}</h1>
              {getStatusBadge(tunnel.status)}
            </div>
            <p className="text-muted-foreground">
              <span className="font-mono">{tunnel.local_host}:{tunnel.local_port}</span>
              <span className="mx-2">→</span>
              <span className="uppercase text-xs">{tunnel.protocol}</span>
              {tunnel.relay_name && (
                <span className="ml-2">via {tunnel.relay_name}</span>
              )}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="icon" onClick={loadData}>
            <RefreshCw className="h-4 w-4" />
          </Button>
          {isRunning ? (
            <Button
              variant="outline"
              onClick={handleStopTunnel}
              disabled={actionLoading === "stop"}
            >
              {actionLoading === "stop" ? (
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              ) : (
                <Square className="h-4 w-4 mr-2" />
              )}
              Stop
            </Button>
          ) : (
            <Button
              onClick={handleStartTunnel}
              disabled={actionLoading === "start"}
            >
              {actionLoading === "start" ? (
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              ) : (
                <Play className="h-4 w-4 mr-2" />
              )}
              Start
            </Button>
          )}
        </div>
      </div>

      {/* Public URL */}
      {tunnel.public_url && (
        <Card className="bg-green-500/5 border-green-500/20">
          <CardContent className="pt-6">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <Globe className="h-5 w-5 text-green-500" />
                <span className="font-mono text-lg text-primary">
                  {tunnel.public_url}
                </span>
              </div>
              <div className="flex items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => copyToClipboard(tunnel.public_url!)}
                >
                  <Copy className="h-4 w-4 mr-2" />
                  Copy
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => openInBrowser(tunnel.public_url!)}
                >
                  <ExternalLink className="h-4 w-4 mr-2" />
                  Open
                </Button>
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Error Message */}
      {tunnel.error_message && (
        <Card className="bg-red-500/5 border-red-500/20">
          <CardContent className="pt-6">
            <div className="flex items-center gap-3">
              <AlertCircle className="h-5 w-5 text-red-500" />
              <p className="text-red-500">{tunnel.error_message}</p>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Stats */}
      <div className="grid gap-4 md:grid-cols-3">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Total Requests</CardTitle>
            <Activity className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{totalRequests}</div>
            <p className="text-xs text-muted-foreground">
              {totalRequests === 0 ? "No requests captured" : "requests captured"}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Avg. Latency</CardTitle>
            <Clock className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{avgLatency}ms</div>
            <p className="text-xs text-muted-foreground">
              average response time
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Errors</CardTitle>
            <AlertCircle className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className={`text-2xl font-bold ${errorRequests > 0 ? "text-red-500" : ""}`}>
              {errorRequests}
            </div>
            <p className="text-xs text-muted-foreground">
              {errorRequests === 0 ? "no errors" : `${errorRequests} error responses`}
            </p>
          </CardContent>
        </Card>
      </div>

      {/* Request Log */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <div>
            <CardTitle>Request Log</CardTitle>
            <CardDescription>
              {tunnel.protocol === "http" || tunnel.protocol === "https"
                ? "Real-time HTTP requests through this tunnel"
                : "Connection activity for this tunnel"}
            </CardDescription>
          </div>
          {metrics.length > 0 && (
            <Button variant="outline" size="sm" onClick={handleClearMetrics}>
              <Trash2 className="h-4 w-4 mr-2" />
              Clear
            </Button>
          )}
        </CardHeader>
        <CardContent>
          {metrics.length === 0 ? (
            <div className="rounded-lg border border-dashed border-border p-8 text-center">
              <Activity className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
              <h3 className="text-lg font-medium mb-2">No Requests Yet</h3>
              <p className="text-sm text-muted-foreground">
                {isConnected
                  ? "Requests will appear here in real-time when traffic flows through the tunnel"
                  : "Start the tunnel and send some requests to see them here"}
              </p>
            </div>
          ) : (
            <>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="w-[100px]">Time</TableHead>
                    <TableHead className="w-[80px]">Method</TableHead>
                    <TableHead>Path</TableHead>
                    <TableHead className="w-[80px]">Status</TableHead>
                    <TableHead className="w-[80px] text-right">Latency</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {metrics
                    .slice((currentPage - 1) * ITEMS_PER_PAGE, currentPage * ITEMS_PER_PAGE)
                    .map((metric) => (
                    <TableRow
                      key={metric.id}
                      className="cursor-pointer hover:bg-muted/50"
                      onClick={() => openRequestDetail(metric)}
                    >
                      <TableCell className="font-mono text-xs">
                        {formatTimestamp(metric.timestamp)}
                      </TableCell>
                      <TableCell>{getMethodBadge(metric.method)}</TableCell>
                      <TableCell className="font-mono text-sm truncate max-w-xs">
                        {metric.uri}
                      </TableCell>
                      <TableCell>
                        {metric.error ? (
                          <Badge className="bg-red-500/10 text-red-500">Error</Badge>
                        ) : (
                          getStatusCodeBadge(metric.response_status)
                        )}
                      </TableCell>
                      <TableCell className="text-right text-sm text-muted-foreground">
                        {metric.duration_ms ? `${metric.duration_ms}ms` : "-"}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
              {/* Pagination Controls */}
              {metrics.length > ITEMS_PER_PAGE && (
                <div className="flex items-center justify-between mt-4 pt-4 border-t">
                  <div className="text-sm text-muted-foreground">
                    Showing {((currentPage - 1) * ITEMS_PER_PAGE) + 1} - {Math.min(currentPage * ITEMS_PER_PAGE, metrics.length)} of {metrics.length} requests
                  </div>
                  <div className="flex items-center gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setCurrentPage(p => Math.max(1, p - 1))}
                      disabled={currentPage === 1}
                    >
                      <ChevronLeft className="h-4 w-4 mr-1" />
                      Previous
                    </Button>
                    <div className="flex items-center gap-1">
                      {Array.from({ length: Math.min(5, Math.ceil(metrics.length / ITEMS_PER_PAGE)) }, (_, i) => {
                        const totalPages = Math.ceil(metrics.length / ITEMS_PER_PAGE);
                        let pageNum: number;

                        if (totalPages <= 5) {
                          pageNum = i + 1;
                        } else if (currentPage <= 3) {
                          pageNum = i + 1;
                        } else if (currentPage >= totalPages - 2) {
                          pageNum = totalPages - 4 + i;
                        } else {
                          pageNum = currentPage - 2 + i;
                        }

                        return (
                          <Button
                            key={pageNum}
                            variant={currentPage === pageNum ? "default" : "outline"}
                            size="sm"
                            className="w-8 h-8 p-0"
                            onClick={() => setCurrentPage(pageNum)}
                          >
                            {pageNum}
                          </Button>
                        );
                      })}
                    </div>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setCurrentPage(p => Math.min(Math.ceil(metrics.length / ITEMS_PER_PAGE), p + 1))}
                      disabled={currentPage >= Math.ceil(metrics.length / ITEMS_PER_PAGE)}
                    >
                      Next
                      <ChevronRight className="h-4 w-4 ml-1" />
                    </Button>
                  </div>
                </div>
              )}
            </>
          )}
        </CardContent>
      </Card>

      {/* Request Detail Dialog */}
      <RequestDetailDialog
        request={selectedRequest}
        open={detailDialogOpen}
        onOpenChange={setDetailDialogOpen}
        tunnelId={id!}
      />
    </div>
  );
}
