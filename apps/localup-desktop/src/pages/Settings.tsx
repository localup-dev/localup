import { useEffect, useState } from "react";
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

// Human-readable labels for settings
const settingLabels: Record<SettingKey, string> = {
  autostart: "Start on Login",
  start_minimized: "Start Minimized",
  auto_connect_tunnels: "Auto-Connect Tunnels",
  capture_traffic: "Capture Traffic",
  clear_on_close: "Clear on Close",
};

export function Settings() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [updating, setUpdating] = useState<SettingKey | null>(null);
  const [version, setVersion] = useState<string>("0.0.0");
  const [error, setError] = useState<string | null>(null);
  const [autostartDialogOpen, setAutostartDialogOpen] = useState(false);

  // Load settings on mount
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
  }, []);

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
