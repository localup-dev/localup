import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { ChevronDown, ChevronRight, RefreshCw, AlertCircle } from "lucide-react";
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
  AppSettings,
  SettingKey,
  getSettings,
  updateSetting,
} from "@/api/settings";

// Daemon status interface
interface DaemonStatus {
  running: boolean;
  version: string | null;
  uptime_seconds: number | null;
  tunnel_count: number | null;
}

// Human-readable labels for settings
const settingLabels: Record<SettingKey, string> = {
  autostart: "Start on Login",
  start_minimized: "Start Minimized",
  auto_connect_tunnels: "Auto-Connect Tunnels",
  capture_traffic: "Capture Traffic",
  clear_on_close: "Clear on Close",
};

// Format uptime in human-readable format
function formatUptime(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  const hours = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  return `${hours}h ${mins}m`;
}

export function Settings() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [updating, setUpdating] = useState<SettingKey | null>(null);
  const [version, setVersion] = useState<string>("0.0.0");
  const [error, setError] = useState<string | null>(null);
  const [autostartDialogOpen, setAutostartDialogOpen] = useState(false);

  // Daemon state
  const [daemonStatus, setDaemonStatus] = useState<DaemonStatus | null>(null);
  const [daemonLoading, setDaemonLoading] = useState(true);
  const [daemonAction, setDaemonAction] = useState<"starting" | "stopping" | null>(null);

  // Daemon logs state
  const [daemonLogs, setDaemonLogs] = useState<string>("");
  const [logsOpen, setLogsOpen] = useState(false);
  const [logsLoading, setLogsLoading] = useState(false);
  const [logsError, setLogsError] = useState<string | null>(null);
  const logsEndRef = useRef<HTMLDivElement>(null);

  // Load daemon status
  const loadDaemonStatus = useCallback(async () => {
    try {
      const status = await invoke<DaemonStatus>("get_daemon_status");
      setDaemonStatus(status);
    } catch (err) {
      console.error("Failed to get daemon status:", err);
      setDaemonStatus({ running: false, version: null, uptime_seconds: null, tunnel_count: null });
    } finally {
      setDaemonLoading(false);
    }
  }, []);

  // Load daemon logs
  const loadDaemonLogs = useCallback(async () => {
    setLogsLoading(true);
    setLogsError(null);
    try {
      const logs = await invoke<string>("get_daemon_logs", { lines: 200 });
      setDaemonLogs(logs);
      // Scroll to bottom after loading
      setTimeout(() => {
        logsEndRef.current?.scrollIntoView({ behavior: "smooth" });
      }, 100);
    } catch (err) {
      console.error("Failed to load daemon logs:", err);
      const errorMsg = String(err);
      setLogsError(errorMsg);
      // Check for serialization errors specifically
      if (errorMsg.includes("serialize") || errorMsg.includes("deserialize")) {
        toast.error("Daemon communication error", {
          description: "There was a serialization error. Try restarting the daemon.",
        });
      }
    } finally {
      setLogsLoading(false);
    }
  }, []);

  // Load settings and daemon status on mount
  useEffect(() => {
    async function loadSettings() {
      try {
        const [loadedSettings, appVersion] = await Promise.all([
          getSettings(),
          invoke<string>("get_version"),
        ]);
        setSettings(loadedSettings);
        setVersion(appVersion);
        setError(null);
      } catch (err) {
        console.error("Failed to load settings:", err);
        setError(String(err));
      } finally {
        setLoading(false);
      }
    }
    loadSettings();
    loadDaemonStatus();

    // Refresh daemon status periodically
    const interval = setInterval(loadDaemonStatus, 5000);
    return () => clearInterval(interval);
  }, [loadDaemonStatus]);

  // Load logs when section is opened
  useEffect(() => {
    if (logsOpen) {
      loadDaemonLogs();
    }
  }, [logsOpen, loadDaemonLogs]);

  // Start daemon
  const handleStartDaemon = async () => {
    setDaemonAction("starting");
    try {
      const status = await invoke<DaemonStatus>("start_daemon");
      setDaemonStatus(status);
      toast.success("Daemon started successfully");
    } catch (err) {
      console.error("Failed to start daemon:", err);
      toast.error("Failed to start daemon", { description: String(err) });
    } finally {
      setDaemonAction(null);
    }
  };

  // Stop daemon
  const handleStopDaemon = async () => {
    setDaemonAction("stopping");
    try {
      await invoke("stop_daemon");
      setDaemonStatus({ running: false, version: null, uptime_seconds: null, tunnel_count: null });
      toast.success("Daemon stopped");
    } catch (err) {
      console.error("Failed to stop daemon:", err);
      toast.error("Failed to stop daemon", { description: String(err) });
    } finally {
      setDaemonAction(null);
    }
  };

  // Handle setting change
  const handleSettingChange = async (key: SettingKey, value: boolean) => {
    if (!settings) return;

    // Show confirmation dialog for autostart enable
    if (key === "autostart" && value && !settings.autostart) {
      setAutostartDialogOpen(true);
      return;
    }

    await doUpdateSetting(key, value);
  };

  // Actually perform the setting update
  const doUpdateSetting = async (key: SettingKey, value: boolean) => {
    if (!settings) return;

    setUpdating(key);
    setError(null);

    // Optimistically update UI
    setSettings((prev) => (prev ? { ...prev, [key]: value } : null));

    try {
      await updateSetting(key, value);
      const label = settingLabels[key];
      toast.success(`${label} ${value ? "enabled" : "disabled"}`);
    } catch (err) {
      console.error(`Failed to update ${key}:`, err);
      // Revert on error
      setSettings((prev) => (prev ? { ...prev, [key]: !value } : null));
      setError(String(err));
      toast.error(`Failed to update ${settingLabels[key]}`, {
        description: String(err),
      });
    } finally {
      setUpdating(null);
    }
  };

  // Handle autostart confirmation
  const handleAutostartConfirm = async () => {
    setAutostartDialogOpen(false);
    await doUpdateSetting("autostart", true);
  };

  if (loading) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-3xl font-bold">Settings</h1>
          <p className="text-muted-foreground">
            Configure LocalUp Desktop preferences
          </p>
        </div>

        {/* Startup Settings Skeleton */}
        <Card>
          <CardHeader>
            <Skeleton className="h-6 w-24" />
            <Skeleton className="h-4 w-64" />
          </CardHeader>
          <CardContent className="space-y-6">
            {[1, 2, 3].map((i) => (
              <div key={i}>
                <div className="flex items-center justify-between">
                  <div className="space-y-2">
                    <Skeleton className="h-4 w-32" />
                    <Skeleton className="h-3 w-48" />
                  </div>
                  <Skeleton className="h-6 w-11 rounded-full" />
                </div>
                {i < 3 && <Separator className="mt-6" />}
              </div>
            ))}
          </CardContent>
        </Card>

        {/* Traffic Settings Skeleton */}
        <Card>
          <CardHeader>
            <Skeleton className="h-6 w-20" />
            <Skeleton className="h-4 w-56" />
          </CardHeader>
          <CardContent className="space-y-6">
            {[1, 2].map((i) => (
              <div key={i}>
                <div className="flex items-center justify-between">
                  <div className="space-y-2">
                    <Skeleton className="h-4 w-32" />
                    <Skeleton className="h-3 w-48" />
                  </div>
                  <Skeleton className="h-6 w-11 rounded-full" />
                </div>
                {i < 2 && <Separator className="mt-6" />}
              </div>
            ))}
          </CardContent>
        </Card>

        {/* About Skeleton */}
        <Card>
          <CardHeader>
            <Skeleton className="h-6 w-16" />
            <Skeleton className="h-4 w-48" />
          </CardHeader>
          <CardContent>
            <div className="space-y-2">
              <div className="flex justify-between">
                <Skeleton className="h-4 w-16" />
                <Skeleton className="h-4 w-12" />
              </div>
              <div className="flex justify-between">
                <Skeleton className="h-4 w-16" />
                <Skeleton className="h-4 w-16" />
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  if (!settings) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-3xl font-bold">Settings</h1>
          <p className="text-destructive">
            Failed to load settings: {error || "Unknown error"}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Settings</h1>
        <p className="text-muted-foreground">
          Configure LocalUp Desktop preferences
        </p>
      </div>

      {error && (
        <div className="rounded-lg border border-destructive bg-destructive/10 p-4 text-sm text-destructive">
          {error}
        </div>
      )}

      {/* Autostart Confirmation Dialog */}
      <AlertDialog open={autostartDialogOpen} onOpenChange={setAutostartDialogOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Enable Start on Login?</AlertDialogTitle>
            <AlertDialogDescription>
              This will add LocalUp to your system's login items. The app will
              automatically start when you log in to your computer.
              <br /><br />
              On macOS, this creates a LaunchAgent. On Windows, this adds an entry
              to the Startup folder. On Linux, this creates an autostart entry.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleAutostartConfirm}>
              Enable
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Startup Settings */}
      <Card>
        <CardHeader>
          <CardTitle>Startup</CardTitle>
          <CardDescription>
            Configure how LocalUp starts and runs
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="autostart">Start on Login</Label>
              <p className="text-sm text-muted-foreground">
                Automatically start LocalUp when you log in
              </p>
            </div>
            <Switch
              id="autostart"
              checked={settings.autostart}
              disabled={updating === "autostart"}
              onCheckedChange={(checked) =>
                handleSettingChange("autostart", checked)
              }
            />
          </div>

          <Separator />

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="minimize-start">Start Minimized</Label>
              <p className="text-sm text-muted-foreground">
                Start in the system tray without showing the window
              </p>
            </div>
            <Switch
              id="minimize-start"
              checked={settings.start_minimized}
              disabled={updating === "start_minimized"}
              onCheckedChange={(checked) =>
                handleSettingChange("start_minimized", checked)
              }
            />
          </div>

          <Separator />

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="auto-connect">Auto-Connect Tunnels</Label>
              <p className="text-sm text-muted-foreground">
                Automatically start tunnels marked as "auto-start"
              </p>
            </div>
            <Switch
              id="auto-connect"
              checked={settings.auto_connect_tunnels}
              disabled={updating === "auto_connect_tunnels"}
              onCheckedChange={(checked) =>
                handleSettingChange("auto_connect_tunnels", checked)
              }
            />
          </div>
        </CardContent>
      </Card>

      {/* Daemon Settings */}
      <Card>
        <CardHeader>
          <CardTitle>Background Service</CardTitle>
          <CardDescription>
            The daemon runs tunnels independently of the app window
          </CardDescription>
        </CardHeader>
        <CardContent>
          {daemonLoading ? (
            <div className="flex items-center justify-between">
              <div className="space-y-2">
                <Skeleton className="h-4 w-32" />
                <Skeleton className="h-3 w-48" />
              </div>
              <Skeleton className="h-9 w-20" />
            </div>
          ) : (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <span className="font-medium">Status</span>
                    {daemonStatus?.running ? (
                      <Badge variant="default" className="bg-green-500 hover:bg-green-600">
                        Running
                      </Badge>
                    ) : (
                      <Badge variant="secondary">Stopped</Badge>
                    )}
                  </div>
                  {daemonStatus?.running && (
                    <p className="text-sm text-muted-foreground">
                      Version {daemonStatus.version} • Uptime {formatUptime(daemonStatus.uptime_seconds || 0)} • {daemonStatus.tunnel_count || 0} tunnel{daemonStatus.tunnel_count !== 1 ? "s" : ""}
                    </p>
                  )}
                  {!daemonStatus?.running && (
                    <p className="text-sm text-muted-foreground">
                      Tunnels will run in-process when daemon is stopped
                    </p>
                  )}
                </div>
                {daemonStatus?.running ? (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleStopDaemon}
                    disabled={daemonAction !== null}
                  >
                    {daemonAction === "stopping" ? "Stopping..." : "Stop"}
                  </Button>
                ) : (
                  <Button
                    variant="default"
                    size="sm"
                    onClick={handleStartDaemon}
                    disabled={daemonAction !== null}
                  >
                    {daemonAction === "starting" ? "Starting..." : "Start"}
                  </Button>
                )}
              </div>

              <Separator />

              <div className="text-sm text-muted-foreground">
                <p>
                  When the daemon is running, tunnels persist even when you close the app window.
                  The daemon starts automatically when you launch LocalUp.
                </p>
              </div>

              <Separator />

              {/* Daemon Logs Section */}
              <Collapsible open={logsOpen} onOpenChange={setLogsOpen}>
                <div className="flex items-center justify-between">
                  <CollapsibleTrigger asChild>
                    <Button variant="ghost" size="sm" className="gap-2 p-0 h-auto hover:bg-transparent">
                      {logsOpen ? (
                        <ChevronDown className="h-4 w-4" />
                      ) : (
                        <ChevronRight className="h-4 w-4" />
                      )}
                      <span className="font-medium">Daemon Logs</span>
                    </Button>
                  </CollapsibleTrigger>
                  {logsOpen && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={loadDaemonLogs}
                      disabled={logsLoading}
                      className="h-8 w-8 p-0"
                    >
                      <RefreshCw className={`h-4 w-4 ${logsLoading ? "animate-spin" : ""}`} />
                    </Button>
                  )}
                </div>
                <CollapsibleContent className="mt-3">
                  {logsError ? (
                    <div className="rounded-lg border border-destructive bg-destructive/10 p-3 text-sm">
                      <div className="flex items-start gap-2">
                        <AlertCircle className="h-4 w-4 mt-0.5 text-destructive" />
                        <div>
                          <p className="font-medium text-destructive">Failed to load logs</p>
                          <p className="text-muted-foreground mt-1">{logsError}</p>
                          {(logsError.includes("serialize") || logsError.includes("deserialize")) && (
                            <p className="text-muted-foreground mt-2">
                              This may be a serialization error. Try restarting the daemon.
                            </p>
                          )}
                        </div>
                      </div>
                    </div>
                  ) : logsLoading && !daemonLogs ? (
                    <div className="space-y-2">
                      <Skeleton className="h-4 w-full" />
                      <Skeleton className="h-4 w-3/4" />
                      <Skeleton className="h-4 w-5/6" />
                    </div>
                  ) : daemonLogs ? (
                    <ScrollArea className="h-64 rounded-lg border bg-muted/50">
                      <pre className="p-3 text-xs font-mono whitespace-pre-wrap break-all">
                        {daemonLogs}
                        <div ref={logsEndRef} />
                      </pre>
                    </ScrollArea>
                  ) : (
                    <p className="text-sm text-muted-foreground">No logs available.</p>
                  )}
                </CollapsibleContent>
              </Collapsible>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Traffic Settings */}
      <Card>
        <CardHeader>
          <CardTitle>Traffic</CardTitle>
          <CardDescription>
            Configure traffic inspection and storage
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="capture-traffic">Capture Traffic</Label>
              <p className="text-sm text-muted-foreground">
                Store request and response data for inspection
              </p>
            </div>
            <Switch
              id="capture-traffic"
              checked={settings.capture_traffic}
              disabled={updating === "capture_traffic"}
              onCheckedChange={(checked) =>
                handleSettingChange("capture_traffic", checked)
              }
            />
          </div>

          <Separator />

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="clear-on-close">Clear on Close</Label>
              <p className="text-sm text-muted-foreground">
                Clear traffic data when closing the app
              </p>
            </div>
            <Switch
              id="clear-on-close"
              checked={settings.clear_on_close}
              disabled={updating === "clear_on_close"}
              onCheckedChange={(checked) =>
                handleSettingChange("clear_on_close", checked)
              }
            />
          </div>
        </CardContent>
      </Card>

      {/* About */}
      <Card>
        <CardHeader>
          <CardTitle>About</CardTitle>
          <CardDescription>
            LocalUp Desktop application information
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-2 text-sm">
            <div className="flex justify-between">
              <span className="text-muted-foreground">Version</span>
              <span>{version}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Platform</span>
              <span>Desktop</span>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
