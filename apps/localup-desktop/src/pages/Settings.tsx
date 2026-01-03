import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";

export function Settings() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold">Settings</h1>
        <p className="text-muted-foreground">
          Configure LocalUp Desktop preferences
        </p>
      </div>

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
            <Switch id="autostart" />
          </div>

          <Separator />

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="minimize-start">Start Minimized</Label>
              <p className="text-sm text-muted-foreground">
                Start in the system tray without showing the window
              </p>
            </div>
            <Switch id="minimize-start" />
          </div>

          <Separator />

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="auto-connect">Auto-Connect Tunnels</Label>
              <p className="text-sm text-muted-foreground">
                Automatically start tunnels marked as "auto-start"
              </p>
            </div>
            <Switch id="auto-connect" defaultChecked />
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
            <Switch id="capture-traffic" defaultChecked />
          </div>

          <Separator />

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <Label htmlFor="clear-on-close">Clear on Close</Label>
              <p className="text-sm text-muted-foreground">
                Clear traffic data when closing the app
              </p>
            </div>
            <Switch id="clear-on-close" />
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
              <span>0.1.0</span>
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
