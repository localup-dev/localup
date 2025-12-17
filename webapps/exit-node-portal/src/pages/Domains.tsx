import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Globe, Plus, Trash2, Shield, Clock, AlertCircle, CheckCircle2, Eye } from 'lucide-react';
import {
  listCustomDomainsOptions,
  deleteCustomDomainMutation,
} from '../api/client/@tanstack/react-query.gen';
import type { CustomDomain, CustomDomainStatus } from '../api/client/types.gen';
import { Button } from '../components/ui/button';
import { Badge } from '../components/ui/badge';
import { Skeleton } from '../components/ui/skeleton';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '../components/ui/alert-dialog';

export default function Domains() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [domainToDelete, setDomainToDelete] = useState<string | null>(null);

  const { data, isLoading, error } = useQuery(listCustomDomainsOptions());
  const domains = data?.domains || [];

  // Delete mutation
  const deleteMutation = useMutation({
    ...deleteCustomDomainMutation(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: listCustomDomainsOptions().queryKey });
      toast.success('Domain deleted');
      setDomainToDelete(null);
    },
    onError: (err) => {
      toast.error(err.message || 'Failed to delete domain');
    },
  });

  const handleDeleteDomain = () => {
    if (domainToDelete) {
      deleteMutation.mutate({ path: { domain: domainToDelete } });
    }
  };

  const getStatusBadge = (status: CustomDomainStatus) => {
    switch (status) {
      case 'active':
        return <Badge variant="success" className="gap-1"><CheckCircle2 className="h-3 w-3" /> Active</Badge>;
      case 'pending':
        return <Badge variant="secondary" className="gap-1"><Clock className="h-3 w-3" /> Pending</Badge>;
      case 'expired':
        return <Badge variant="destructive" className="gap-1"><AlertCircle className="h-3 w-3" /> Expired</Badge>;
      case 'failed':
        return <Badge variant="destructive" className="gap-1"><AlertCircle className="h-3 w-3" /> Failed</Badge>;
      default:
        return <Badge variant="secondary">{status}</Badge>;
    }
  };

  return (
    <div className="min-h-screen bg-background text-foreground">
      {/* Header */}
      <div className="border-b border-border">
        <div className="max-w-7xl mx-auto px-6 py-6">
          <div className="flex items-center justify-between">
            <div>
              <h1 className="text-3xl font-bold">Custom Domains</h1>
              <p className="text-muted-foreground mt-2">
                Provision SSL certificates and use your own domains with tunnels
              </p>
            </div>
            <Button onClick={() => navigate('/domains/add')} className="gap-2">
              <Plus className="h-4 w-4" />
              Add Domain
            </Button>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="max-w-7xl mx-auto px-6 py-8">
        {error && (
          <div className="mb-6 bg-destructive/10 border border-destructive/50 text-destructive px-4 py-3 rounded-lg">
            {error.message || 'An error occurred'}
          </div>
        )}

        {isLoading ? (
          <div className="bg-card rounded-lg border border-border overflow-hidden">
            <div className="bg-muted px-6 py-3">
              <div className="grid grid-cols-5 gap-4">
                {['Domain', 'Status', 'Provisioned', 'Expires', 'Actions'].map((h) => (
                  <Skeleton key={h} className="h-4 w-16" />
                ))}
              </div>
            </div>
            <div className="divide-y divide-border">
              {[1, 2, 3].map((i) => (
                <div key={i} className="px-6 py-4 grid grid-cols-5 gap-4 items-center">
                  <Skeleton className="h-4 w-32" />
                  <Skeleton className="h-6 w-20 rounded-full" />
                  <Skeleton className="h-4 w-24" />
                  <Skeleton className="h-4 w-24" />
                  <Skeleton className="h-8 w-20" />
                </div>
              ))}
            </div>
          </div>
        ) : domains.length === 0 ? (
          <div className="bg-card rounded-lg border border-border p-12 text-center">
            <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center">
              <Globe className="h-8 w-8 text-muted-foreground" />
            </div>
            <h2 className="text-2xl font-bold mb-2">No custom domains yet</h2>
            <p className="text-muted-foreground mb-6 max-w-md mx-auto">
              Add your own domains to use with tunnels. You can automatically provision SSL certificates
              via Let's Encrypt or upload your own certificates.
            </p>
            <Button onClick={() => navigate('/domains/add')} size="lg" className="gap-2">
              <Plus className="h-4 w-4" />
              Add Your First Domain
            </Button>
          </div>
        ) : (
          <div className="bg-card rounded-lg border border-border overflow-hidden">
            <table className="w-full">
              <thead className="bg-muted">
                <tr>
                  <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Domain
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Status
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Provisioned
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Expires
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Auto-Renew
                  </th>
                  <th className="px-6 py-3 text-right text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Actions
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {domains.map((d: CustomDomain) => (
                  <tr key={d.domain} className="hover:bg-muted/50 transition-colors">
                    <td className="px-6 py-4 whitespace-nowrap">
                      <div className="flex items-center gap-2">
                        <Shield className="h-4 w-4 text-green-500" />
                        <span className="font-medium">{d.domain}</span>
                      </div>
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap">
                      {getStatusBadge(d.status)}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-muted-foreground">
                      {new Date(d.provisioned_at).toLocaleDateString()}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-muted-foreground">
                      {d.expires_at ? new Date(d.expires_at).toLocaleDateString() : 'Never'}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap">
                      <Badge variant={d.auto_renew ? 'success' : 'secondary'}>
                        {d.auto_renew ? 'Yes' : 'No'}
                      </Badge>
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-right text-sm">
                      <div className="flex items-center justify-end gap-2">
                        <Button
                          onClick={() => navigate(`/domains/${d.id}`)}
                          variant="outline"
                          size="sm"
                          className="gap-2"
                        >
                          <Eye className="h-4 w-4" />
                          View
                        </Button>
                        <Button
                          onClick={() => setDomainToDelete(d.domain)}
                          variant="destructive"
                          size="sm"
                          className="gap-2"
                        >
                          <Trash2 className="h-4 w-4" />
                          Delete
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Delete Confirmation Dialog */}
      <AlertDialog open={!!domainToDelete} onOpenChange={(open) => !open && setDomainToDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Domain</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete <strong>{domainToDelete}</strong>? This will remove the SSL certificate
              and the domain will no longer be available for tunnels.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDeleteDomain}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {deleteMutation.isPending ? 'Deleting...' : 'Delete Domain'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
