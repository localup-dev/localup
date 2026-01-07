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
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Separator } from "@/components/ui/separator";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
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
  ChevronDown,
  ChevronUp,
  RotateCcw,
  Search,
  X,
  FileJson,
  FileText,
  Binary,
  Check,
  Download,
} from "lucide-react";
import { toast } from "sonner";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  getTunnel,
  startTunnel,
  stopTunnel,
  getTunnelMetrics,
  clearTunnelMetrics,
  replayRequest,
  getTcpConnections,
  type Tunnel,
  type HttpMetric,
  type BodyData,
  type ReplayResponse,
  type TcpConnection,
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

function formatTimestampString(timestamp: string): string {
  return new Date(timestamp).toLocaleTimeString();
}

function getTcpStateBadge(state: string) {
  switch (state.toLowerCase()) {
    case "connected":
    case "active":
      return (
        <Badge className="bg-green-500/10 text-green-500">
          <CheckCircle2 className="h-3 w-3 mr-1" />
          Active
        </Badge>
      );
    case "closed":
      return (
        <Badge variant="secondary">
          <WifiOff className="h-3 w-3 mr-1" />
          Closed
        </Badge>
      );
    case "error":
      return (
        <Badge className="bg-red-500/10 text-red-500">
          <AlertCircle className="h-3 w-3 mr-1" />
          Error
        </Badge>
      );
    default:
      return <Badge variant="secondary">{state}</Badge>;
  }
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

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

function getBodyTypeIcon(contentType: string) {
  if (contentType.includes("json")) {
    return <FileJson className="h-4 w-4" />;
  } else if (contentType.includes("text") || contentType.includes("html") || contentType.includes("xml")) {
    return <FileText className="h-4 w-4" />;
  } else {
    return <Binary className="h-4 w-4" />;
  }
}

function CopyButton({ text, label }: { text: string; label?: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async (e: React.MouseEvent) => {
    e.stopPropagation();
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <Button
      variant="ghost"
      size="sm"
      className="h-7 px-2"
      onClick={handleCopy}
    >
      {copied ? (
        <Check className="h-3 w-3 text-green-500" />
      ) : (
        <Copy className="h-3 w-3" />
      )}
      {label && <span className="ml-1 text-xs">{label}</span>}
    </Button>
  );
}

function HeadersSection({
  headers,
  title,
  defaultOpen = true
}: {
  headers: [string, string][];
  title: string;
  defaultOpen?: boolean;
}) {
  const [isOpen, setIsOpen] = useState(defaultOpen);
  const headersText = headers.map(([k, v]) => `${k}: ${v}`).join("\n");

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <div className="flex items-center justify-between">
        <CollapsibleTrigger className="flex items-center gap-2 text-sm font-medium hover:underline">
          {isOpen ? <ChevronDown className="h-4 w-4" /> : <ChevronUp className="h-4 w-4" />}
          {title}
          <Badge variant="secondary" className="text-xs">
            {headers.length}
          </Badge>
        </CollapsibleTrigger>
        <CopyButton text={headersText} />
      </div>
      <CollapsibleContent className="mt-2">
        <div className="rounded-md border bg-muted/30 overflow-hidden">
          <ScrollArea className="h-[180px]">
            <table className="w-full text-xs">
              <tbody>
                {headers.map(([name, value], idx) => (
                  <tr key={idx} className="border-b last:border-0 hover:bg-muted/50">
                    <td className="px-3 py-1.5 font-mono font-medium text-muted-foreground whitespace-nowrap align-top w-1/3">
                      {name}
                    </td>
                    <td className="px-3 py-1.5 font-mono break-all">
                      {value}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </ScrollArea>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

function DownloadButton({ content, filename, contentType }: { content: string; filename: string; contentType: string }) {
  const handleDownload = () => {
    const blob = new Blob([content], { type: contentType });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    toast.success(`Downloaded ${filename}`);
  };

  return (
    <Button variant="ghost" size="sm" className="h-7 px-2" onClick={handleDownload}>
      <Download className="h-3 w-3" />
      <span className="ml-1 text-xs">Download</span>
    </Button>
  );
}

function BodySection({
  body,
  title = "Body",
  defaultOpen = true,
  filename = "response"
}: {
  body: BodyData | null;
  title?: string;
  defaultOpen?: boolean;
  filename?: string;
}) {
  const [isOpen, setIsOpen] = useState(defaultOpen);

  if (!body) {
    return (
      <div className="text-sm text-muted-foreground italic">
        No body
      </div>
    );
  }

  const formattedBody = formatBodyData(body);
  const isJson = body.content_type.includes("json") && body.data.type === "Json";
  const isLarge = body.size > 10 * 1024; // Show download button for bodies > 10KB

  // Determine file extension based on content type
  const getFileExtension = (ct: string): string => {
    if (ct.includes("json")) return ".json";
    if (ct.includes("html")) return ".html";
    if (ct.includes("xml")) return ".xml";
    if (ct.includes("javascript")) return ".js";
    if (ct.includes("css")) return ".css";
    return ".txt";
  };

  const downloadFilename = `${filename}${getFileExtension(body.content_type)}`;

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <div className="flex items-center justify-between">
        <CollapsibleTrigger className="flex items-center gap-2 text-sm font-medium hover:underline">
          {isOpen ? <ChevronDown className="h-4 w-4" /> : <ChevronUp className="h-4 w-4" />}
          {title}
        </CollapsibleTrigger>
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
            {getBodyTypeIcon(body.content_type)}
            <span>{body.content_type.split(";")[0]}</span>
            <Separator orientation="vertical" className="h-3" />
            <span>{formatBytes(body.size)}</span>
          </div>
          {body.data.type !== "Binary" && (
            <>
              {isLarge && (
                <DownloadButton
                  content={formattedBody}
                  filename={downloadFilename}
                  contentType={body.content_type}
                />
              )}
              <CopyButton text={formattedBody} />
            </>
          )}
        </div>
      </div>
      <CollapsibleContent className="mt-2">
        <div className="rounded-md border bg-muted/30 overflow-hidden">
          <ScrollArea className="h-[280px]">
            <pre className={`p-3 text-xs font-mono whitespace-pre-wrap break-all ${isJson ? "text-emerald-600 dark:text-emerald-400" : ""}`}>
              {formattedBody}
            </pre>
          </ScrollArea>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

function RequestDetailSidebar({
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
  const [activeTab, setActiveTab] = useState<string>("request");

  // Reset state when sidebar opens with a new request
  useEffect(() => {
    if (open && request) {
      setReplayResult(null);
      setActiveTab("request");
    }
  }, [open, request?.id]);

  if (!request) return null;

  const host = request.request_headers.find(([k]) => k.toLowerCase() === "host")?.[1] || "";

  const handleReplay = async () => {
    setReplaying(true);
    setReplayResult(null);
    try {
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
      setActiveTab("replay");
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

  // Generate cURL command
  const generateCurl = () => {
    const headers = request.request_headers
      .map(([k, v]) => `-H '${k}: ${v}'`)
      .join(" \\\n  ");
    const body = request.request_body ? formatBodyData(request.request_body) : null;
    const bodyFlag = body ? ` \\\n  -d '${body.replace(/'/g, "'\\''")}'` : "";
    return `curl -X ${request.method} '${host}${request.uri}' \\\n  ${headers}${bodyFlag}`;
  };

  if (!open) return null;

  return (
    <div className="w-[500px] flex-shrink-0 border-l bg-background flex flex-col h-full">
      {/* Header */}
      <div className="px-6 py-4 border-b flex-shrink-0 space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2 min-w-0 flex-1">
            {getMethodBadge(request.method)}
            <code className="font-mono text-sm bg-muted px-2 py-1 rounded truncate">
              {request.uri}
            </code>
          </div>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8 flex-shrink-0"
            onClick={() => onOpenChange(false)}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
        {/* Meta info row */}
        <div className="flex flex-wrap items-center gap-3 text-sm text-muted-foreground">
          <span className="font-mono text-xs">{host}</span>
          <Separator orientation="vertical" className="h-4" />
          <span className="flex items-center gap-1">
            <Clock className="h-3 w-3" />
            {formatTimestamp(request.timestamp)}
          </span>
          {request.duration_ms != null && (
            <>
              <Separator orientation="vertical" className="h-4" />
              <span>{request.duration_ms}ms</span>
            </>
          )}
          {request.response_status && (
            <>
              <Separator orientation="vertical" className="h-4" />
              {getStatusCodeBadge(request.response_status)}
            </>
          )}
          {request.error && (
            <Badge className="bg-red-500/10 text-red-500">{request.error}</Badge>
          )}
        </div>
        {/* Action buttons */}
        <div className="flex items-center gap-2">
          <CopyButton text={generateCurl()} label="cURL" />
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
        </div>
      </div>

        <Tabs value={activeTab} onValueChange={setActiveTab} className="flex-1 flex flex-col min-h-0">
          <TabsList className={`grid w-full flex-shrink-0 mx-6 mt-4 ${replayResult ? "grid-cols-3" : "grid-cols-2"}`} style={{ width: 'calc(100% - 48px)' }}>
            <TabsTrigger value="request" className="gap-1 text-xs">
              <ArrowUpFromLine className="h-3 w-3" />
              Request
              {request.request_body && (
                <Badge variant="secondary" className="text-[10px] px-1">
                  {formatBytes(request.request_body.size)}
                </Badge>
              )}
            </TabsTrigger>
            <TabsTrigger value="response" className="gap-1 text-xs">
              <ArrowDownToLine className="h-3 w-3" />
              Response
              {request.response_body && (
                <Badge variant="secondary" className="text-[10px] px-1">
                  {formatBytes(request.response_body.size)}
                </Badge>
              )}
            </TabsTrigger>
            {replayResult && (
              <TabsTrigger value="replay" className="gap-1 text-xs">
                <RotateCcw className="h-3 w-3" />
                Replay
                {getStatusCodeBadge(replayResult.status)}
              </TabsTrigger>
            )}
          </TabsList>

          <ScrollArea className="flex-1 px-6 py-4">
            <TabsContent value="request" className="space-y-4 m-0">
              <HeadersSection
                headers={request.request_headers}
                title="Request Headers"
              />
              <Separator />
              <BodySection
                body={request.request_body}
                title="Request Body"
                filename={`request-${request.id}`}
              />
            </TabsContent>

            <TabsContent value="response" className="space-y-4 m-0">
              {/* Response Status Summary */}
              <div className="flex flex-wrap items-center gap-3 p-3 rounded-lg bg-muted/50 text-sm">
                <div className="flex items-center gap-2">
                  <span className="font-medium">Status:</span>
                  {getStatusCodeBadge(request.response_status)}
                </div>
                {request.duration_ms != null && (
                  <div className="flex items-center gap-2">
                    <span className="font-medium">Time:</span>
                    <span>{request.duration_ms}ms</span>
                  </div>
                )}
                {request.response_body && (
                  <div className="flex items-center gap-2">
                    <span className="font-medium">Size:</span>
                    <span>{formatBytes(request.response_body.size)}</span>
                  </div>
                )}
              </div>

              <HeadersSection
                headers={request.response_headers || []}
                title="Response Headers"
              />
              <Separator />
              <BodySection
                body={request.response_body}
                title="Response Body"
                filename={`response-${request.id}`}
              />
            </TabsContent>

            {replayResult && (
              <TabsContent value="replay" className="space-y-4 m-0">
                {/* Replay Status Summary */}
                <div className="flex flex-wrap items-center gap-3 p-3 rounded-lg bg-muted/50 text-sm">
                  <div className="flex items-center gap-2">
                    <span className="font-medium">Status:</span>
                    {getStatusCodeBadge(replayResult.status)}
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="font-medium">Time:</span>
                    <span>{replayResult.duration_ms}ms</span>
                  </div>
                </div>

                <HeadersSection
                  headers={replayResult.headers}
                  title="Response Headers"
                />
                <Separator />
                {replayResult.body ? (
                  <Collapsible defaultOpen>
                    <div className="flex items-center justify-between">
                      <CollapsibleTrigger className="flex items-center gap-2 text-sm font-medium hover:underline">
                        <ChevronDown className="h-4 w-4" />
                        Response Body
                      </CollapsibleTrigger>
                      <CopyButton text={replayResult.body} />
                    </div>
                    <CollapsibleContent className="mt-2">
                      <div className="rounded-md border bg-muted/30 overflow-hidden">
                        <ScrollArea className="h-[200px]">
                          <pre className="p-3 text-xs font-mono whitespace-pre-wrap break-all text-emerald-600 dark:text-emerald-400">
                            {formatJsonBody(replayResult.body)}
                          </pre>
                        </ScrollArea>
                      </div>
                    </CollapsibleContent>
                  </Collapsible>
                ) : (
                  <div className="text-sm text-muted-foreground italic">
                    No body
                  </div>
                )}
              </TabsContent>
            )}
          </ScrollArea>
        </Tabs>
    </div>
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

  // TCP connections state
  const [tcpConnections, setTcpConnections] = useState<TcpConnection[]>([]);
  const tcpConnectionsRef = useRef<Map<string, TcpConnection>>(new Map());

  // Filter state
  const [methodFilter, setMethodFilter] = useState<string>("");
  const [pathFilter, setPathFilter] = useState<string>("");
  const [statusFilter, setStatusFilter] = useState<string>("");

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

      // Load metrics based on protocol type
      const isTcpProtocol = tunnelData.protocol === "tcp" || tunnelData.protocol === "tls";

      if (isTcpProtocol) {
        // Load TCP connections for TCP/TLS tunnels
        const tcpResponse = await getTcpConnections(id, 0, 100);
        tcpResponse.items.forEach(c => tcpConnectionsRef.current.set(c.id, c));
        setTcpConnections(tcpResponse.items);
      } else {
        // Load HTTP metrics for HTTP/HTTPS tunnels
        const metricsResponse = await getTunnelMetrics(id, 0, 100);
        metricsResponse.items.forEach(m => metricsRef.current.set(m.id, m));
        setMetrics(metricsResponse.items);
      }
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

    // Poll for tunnel status and metrics updates every 1 second
    // This is simpler and more reliable than streaming subscriptions
    const interval = setInterval(async () => {
      if (!id) return;
      try {
        // Poll tunnel status
        const tunnelData = await getTunnel(id);
        if (tunnelData) {
          setTunnel(tunnelData);

          // Poll metrics based on protocol type
          const isTcpProtocol = tunnelData.protocol === "tcp" || tunnelData.protocol === "tls";

          if (isTcpProtocol) {
            // Poll TCP connections for TCP/TLS tunnels
            const tcpResponse = await getTcpConnections(id, 0, 100);
            tcpResponse.items.forEach(c => tcpConnectionsRef.current.set(c.id, c));
            // Update state with all connections sorted by timestamp (newest first)
            setTcpConnections(
              Array.from(tcpConnectionsRef.current.values()).sort(
                (a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
              )
            );
          } else {
            // Poll HTTP metrics for HTTP/HTTPS tunnels
            const metricsResponse = await getTunnelMetrics(id, 0, 100);
            metricsResponse.items.forEach(m => metricsRef.current.set(m.id, m));
            setMetrics(Array.from(metricsRef.current.values()).sort((a, b) => b.timestamp - a.timestamp));
          }
        }
      } catch {
        // Silently ignore polling errors
      }
    }, 1000);

    return () => clearInterval(interval);
  }, [id, loadData]);

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
  const isTcpProtocol = tunnel.protocol === "tcp" || tunnel.protocol === "tls";

  // Filter metrics based on filter state (HTTP only)
  const filteredMetrics = metrics.filter((m) => {
    // Method filter
    if (methodFilter && m.method.toUpperCase() !== methodFilter.toUpperCase()) {
      return false;
    }
    // Path filter (case-insensitive contains)
    if (pathFilter && !m.uri.toLowerCase().includes(pathFilter.toLowerCase())) {
      return false;
    }
    // Status filter
    if (statusFilter) {
      const status = m.response_status;
      if (statusFilter === "2xx" && (status === null || status < 200 || status >= 300)) return false;
      if (statusFilter === "3xx" && (status === null || status < 300 || status >= 400)) return false;
      if (statusFilter === "4xx" && (status === null || status < 400 || status >= 500)) return false;
      if (statusFilter === "5xx" && (status === null || status < 500)) return false;
      if (statusFilter === "error" && !m.error) return false;
      if (statusFilter === "pending" && (status !== null || m.error)) return false;
    }
    return true;
  });

  // Calculate stats based on protocol type
  const totalRequests = isTcpProtocol ? tcpConnections.length : metrics.length;
  const avgLatency = isTcpProtocol
    ? (tcpConnections.length > 0
        ? Math.round(tcpConnections.reduce((sum, c) => sum + (c.duration_ms || 0), 0) / tcpConnections.length)
        : 0)
    : (metrics.length > 0
        ? Math.round(metrics.reduce((sum, r) => sum + (r.duration_ms || 0), 0) / metrics.length)
        : 0);
  const errorCount = isTcpProtocol
    ? tcpConnections.filter((c) => c.error).length
    : metrics.filter((r) => r.response_status && r.response_status >= 400).length;

  // TCP-specific stats
  const totalBytesIn = tcpConnections.reduce((sum, c) => sum + c.bytes_received, 0);
  const totalBytesOut = tcpConnections.reduce((sum, c) => sum + c.bytes_sent, 0);
  const activeConnections = tcpConnections.filter((c) => c.state.toLowerCase() === "active" || c.state.toLowerCase() === "connected").length;

  // Reset to page 1 when filters change
  const hasActiveFilter = methodFilter || pathFilter || statusFilter;

  return (
    <div className="flex h-[calc(100vh-4rem)]">
      {/* Main Content */}
      <div className="flex-1 overflow-auto p-6 space-y-6">
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
      {isTcpProtocol ? (
        /* TCP-specific stats */
        <div className="grid gap-4 md:grid-cols-4">
          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Connections</CardTitle>
              <Activity className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{totalRequests}</div>
              <p className="text-xs text-muted-foreground">
                {activeConnections > 0 ? `${activeConnections} active` : "total connections"}
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Data In</CardTitle>
              <ArrowDownToLine className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{formatBytes(totalBytesIn)}</div>
              <p className="text-xs text-muted-foreground">bytes received</p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Data Out</CardTitle>
              <ArrowUpFromLine className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{formatBytes(totalBytesOut)}</div>
              <p className="text-xs text-muted-foreground">bytes sent</p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium">Errors</CardTitle>
              <AlertCircle className="h-4 w-4 text-muted-foreground" />
            </CardHeader>
            <CardContent>
              <div className={`text-2xl font-bold ${errorCount > 0 ? "text-red-500" : ""}`}>
                {errorCount}
              </div>
              <p className="text-xs text-muted-foreground">
                {errorCount === 0 ? "no errors" : "connection errors"}
              </p>
            </CardContent>
          </Card>
        </div>
      ) : (
        /* HTTP-specific stats */
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
              <div className={`text-2xl font-bold ${errorCount > 0 ? "text-red-500" : ""}`}>
                {errorCount}
              </div>
              <p className="text-xs text-muted-foreground">
                {errorCount === 0 ? "no errors" : `${errorCount} error responses`}
              </p>
            </CardContent>
          </Card>
        </div>
      )}

      {/* Connection/Request Log */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <div>
            <CardTitle>{isTcpProtocol ? "Connection Log" : "Request Log"}</CardTitle>
            <CardDescription>
              {isTcpProtocol
                ? "Real-time TCP connections through this tunnel"
                : "Real-time HTTP requests through this tunnel"}
            </CardDescription>
          </div>
          {(isTcpProtocol ? tcpConnections.length > 0 : metrics.length > 0) && (
            <Button variant="outline" size="sm" onClick={handleClearMetrics}>
              <Trash2 className="h-4 w-4 mr-2" />
              Clear
            </Button>
          )}
        </CardHeader>
        <CardContent>
          {isTcpProtocol ? (
            /* TCP Connections Table */
            tcpConnections.length === 0 ? (
              <div className="rounded-lg border border-dashed border-border p-8 text-center">
                <Activity className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
                <h3 className="text-lg font-medium mb-2">No Connections Yet</h3>
                <p className="text-sm text-muted-foreground">
                  {isConnected
                    ? "Connections will appear here in real-time when traffic flows through the tunnel"
                    : "Start the tunnel and make some connections to see them here"}
                </p>
              </div>
            ) : (
              <>
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead className="w-[100px]">Time</TableHead>
                      <TableHead>Remote Address</TableHead>
                      <TableHead>Local Address</TableHead>
                      <TableHead className="w-[80px]">State</TableHead>
                      <TableHead className="w-[100px] text-right">Data In</TableHead>
                      <TableHead className="w-[100px] text-right">Data Out</TableHead>
                      <TableHead className="w-[80px] text-right">Duration</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {tcpConnections
                      .slice((currentPage - 1) * ITEMS_PER_PAGE, currentPage * ITEMS_PER_PAGE)
                      .map((conn) => (
                      <TableRow key={conn.id} className="hover:bg-muted/50">
                        <TableCell className="font-mono text-xs">
                          {formatTimestampString(conn.timestamp)}
                        </TableCell>
                        <TableCell className="font-mono text-sm">
                          {conn.remote_addr}
                        </TableCell>
                        <TableCell className="font-mono text-sm">
                          {conn.local_addr}
                        </TableCell>
                        <TableCell>
                          {conn.error ? (
                            <Badge className="bg-red-500/10 text-red-500">
                              <AlertCircle className="h-3 w-3 mr-1" />
                              Error
                            </Badge>
                          ) : (
                            getTcpStateBadge(conn.state)
                          )}
                        </TableCell>
                        <TableCell className="text-right text-sm text-muted-foreground">
                          {formatBytes(conn.bytes_received)}
                        </TableCell>
                        <TableCell className="text-right text-sm text-muted-foreground">
                          {formatBytes(conn.bytes_sent)}
                        </TableCell>
                        <TableCell className="text-right text-sm text-muted-foreground">
                          {conn.duration_ms != null ? `${conn.duration_ms}ms` : "-"}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
                {/* Pagination Controls for TCP */}
                {tcpConnections.length > ITEMS_PER_PAGE && (
                  <div className="flex items-center justify-between mt-4 pt-4 border-t">
                    <div className="text-sm text-muted-foreground">
                      Showing {((currentPage - 1) * ITEMS_PER_PAGE) + 1} - {Math.min(currentPage * ITEMS_PER_PAGE, tcpConnections.length)} of {tcpConnections.length} connections
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
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => setCurrentPage(p => Math.min(Math.ceil(tcpConnections.length / ITEMS_PER_PAGE), p + 1))}
                        disabled={currentPage >= Math.ceil(tcpConnections.length / ITEMS_PER_PAGE)}
                      >
                        Next
                        <ChevronRight className="h-4 w-4 ml-1" />
                      </Button>
                    </div>
                  </div>
                )}
              </>
            )
          ) : (
            /* HTTP Requests Table */
            metrics.length === 0 ? (
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
                {/* Filters */}
                <div className="flex flex-wrap gap-3 mb-4">
                  <Select
                    value={methodFilter}
                    onValueChange={(value) => {
                      setMethodFilter(value === "all" ? "" : value);
                      setCurrentPage(1);
                    }}
                  >
                    <SelectTrigger className="w-[120px]">
                      <SelectValue placeholder="Method" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">All Methods</SelectItem>
                      <SelectItem value="GET">GET</SelectItem>
                      <SelectItem value="POST">POST</SelectItem>
                      <SelectItem value="PUT">PUT</SelectItem>
                      <SelectItem value="PATCH">PATCH</SelectItem>
                      <SelectItem value="DELETE">DELETE</SelectItem>
                      <SelectItem value="OPTIONS">OPTIONS</SelectItem>
                      <SelectItem value="HEAD">HEAD</SelectItem>
                    </SelectContent>
                  </Select>

                  <div className="relative flex-1 min-w-[200px] max-w-[300px]">
                    <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
                    <Input
                      placeholder="Filter by path..."
                      value={pathFilter}
                      onChange={(e) => {
                        setPathFilter(e.target.value);
                        setCurrentPage(1);
                      }}
                      className="pl-8"
                    />
                    {pathFilter && (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="absolute right-0 top-0 h-full px-2"
                        onClick={() => {
                          setPathFilter("");
                          setCurrentPage(1);
                        }}
                      >
                        <X className="h-4 w-4" />
                      </Button>
                    )}
                  </div>

                  <Select
                    value={statusFilter}
                    onValueChange={(value) => {
                      setStatusFilter(value === "all" ? "" : value);
                      setCurrentPage(1);
                    }}
                  >
                    <SelectTrigger className="w-[130px]">
                      <SelectValue placeholder="Status" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">All Status</SelectItem>
                      <SelectItem value="2xx">2xx Success</SelectItem>
                      <SelectItem value="3xx">3xx Redirect</SelectItem>
                      <SelectItem value="4xx">4xx Client Error</SelectItem>
                      <SelectItem value="5xx">5xx Server Error</SelectItem>
                      <SelectItem value="error">Errors</SelectItem>
                      <SelectItem value="pending">Pending</SelectItem>
                    </SelectContent>
                  </Select>

                  {hasActiveFilter && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => {
                        setMethodFilter("");
                        setPathFilter("");
                        setStatusFilter("");
                        setCurrentPage(1);
                      }}
                    >
                      <X className="h-4 w-4 mr-1" />
                      Clear Filters
                    </Button>
                  )}

                  {hasActiveFilter && (
                    <span className="text-sm text-muted-foreground self-center">
                      {filteredMetrics.length} of {metrics.length} requests
                    </span>
                  )}
                </div>

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
                    {filteredMetrics
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
                          {metric.duration_ms != null ? `${metric.duration_ms}ms` : "-"}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
                {/* Pagination Controls */}
                {filteredMetrics.length > ITEMS_PER_PAGE && (
                  <div className="flex items-center justify-between mt-4 pt-4 border-t">
                    <div className="text-sm text-muted-foreground">
                      Showing {((currentPage - 1) * ITEMS_PER_PAGE) + 1} - {Math.min(currentPage * ITEMS_PER_PAGE, filteredMetrics.length)} of {filteredMetrics.length} requests
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
                        {Array.from({ length: Math.min(5, Math.ceil(filteredMetrics.length / ITEMS_PER_PAGE)) }, (_, i) => {
                          const totalPages = Math.ceil(filteredMetrics.length / ITEMS_PER_PAGE);
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
                        onClick={() => setCurrentPage(p => Math.min(Math.ceil(filteredMetrics.length / ITEMS_PER_PAGE), p + 1))}
                        disabled={currentPage >= Math.ceil(filteredMetrics.length / ITEMS_PER_PAGE)}
                      >
                        Next
                        <ChevronRight className="h-4 w-4 ml-1" />
                      </Button>
                    </div>
                  </div>
                )}
              </>
            )
          )}
        </CardContent>
      </Card>
      </div>

      {/* Request Detail Sidebar */}
      <RequestDetailSidebar
        request={selectedRequest}
        open={detailDialogOpen}
        onOpenChange={setDetailDialogOpen}
        tunnelId={id!}
      />
    </div>
  );
}
