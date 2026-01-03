import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Plus, Network } from "lucide-react";

export function Tunnels() {
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">Tunnels</h1>
          <p className="text-muted-foreground">
            Manage your tunnel configurations
          </p>
        </div>
        <Button>
          <Plus className="h-4 w-4 mr-2" />
          New Tunnel
        </Button>
      </div>

      {/* Empty State */}
      <Card>
        <CardHeader>
          <CardTitle>Your Tunnels</CardTitle>
          <CardDescription>
            Tunnels let you expose local servers to the internet
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="rounded-lg border border-dashed border-border p-12 text-center">
            <Network className="mx-auto h-12 w-12 text-muted-foreground mb-4" />
            <h3 className="text-lg font-medium mb-2">No Tunnels Configured</h3>
            <p className="text-sm text-muted-foreground mb-4 max-w-md mx-auto">
              Create a tunnel to expose your local development server.
              You'll need to add a relay server first.
            </p>
            <Button variant="outline">
              <Plus className="h-4 w-4 mr-2" />
              Create Your First Tunnel
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
