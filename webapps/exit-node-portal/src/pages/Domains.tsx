import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Globe, Plus, Trash2, RefreshCw, Shield, Clock, AlertCircle, CheckCircle2, Loader2 } from 'lucide-react';
import {
  listCustomDomainsOptions,
  uploadCustomDomainMutation,
  deleteCustomDomainMutation,
} from '../api/client/@tanstack/react-query.gen';
import type { CustomDomain, CustomDomainStatus } from '../api/client/types.gen';
import { Button } from '../components/ui/button';
import { Badge } from '../components/ui/badge';
import { Input } from '../components/ui/input';
import { Label } from '../components/ui/label';
import { Skeleton } from '../components/ui/skeleton';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../components/ui/tabs';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../components/ui/dialog';
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

type ProvisioningStep = 'input' | 'verifying' | 'success' | 'error';

export default function Domains() {
  const queryClient = useQueryClient();
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [domainToDelete, setDomainToDelete] = useState<string | null>(null);

  // Form state
  const [domain, setDomain] = useState('');
  const [provisioningMethod, setProvisioningMethod] = useState<'letsencrypt' | 'manual'>('letsencrypt');
  const [certPem, setCertPem] = useState('');
  const [keyPem, setKeyPem] = useState('');

  // Provisioning flow state
  const [provisioningStep, setProvisioningStep] = useState<ProvisioningStep>('input');
  const [provisioningError, setProvisioningError] = useState<string | null>(null);
  const [challengeInfo, setChallengeInfo] = useState<{ token: string; domain: string; challengeId: string } | null>(null);

  const { data, isLoading, error } = useQuery(listCustomDomainsOptions());
  const domains = data?.domains || [];

  // Request ACME certificate mutation
  const requestCertMutation = useMutation({
    mutationFn: async (domainName: string) => {
      const response = await fetch(`/api/domains/${encodeURIComponent(domainName)}/certificate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'include',
      });
      if (!response.ok) {
        const error = await response.json();
        throw new Error(error.error || 'Failed to request certificate');
      }
      return response.json();
    },
    onSuccess: (response) => {
      // Extract challenge info from HTTP-01 response
      if ('challenge' in response && response.challenge && 'type' in response.challenge) {
        const challenge = response.challenge as { type: string; token?: string };
        if (challenge.type === 'http01' && challenge.token) {
          setChallengeInfo({
            token: challenge.token,
            domain: response.domain,
            challengeId: response.challenge_id,
          });
        }
      }
      setProvisioningStep('verifying');
      toast.info('Certificate provisioning started. Verifying domain...');

      // Auto-complete the challenge after a short delay
      setTimeout(() => {
        completeCertMutation.mutate({
          body: {
            domain: response.domain,
            challenge_id: response.challenge_id,
          },
        });
      }, 3000);
    },
    onError: (err: Error) => {
      setProvisioningStep('error');
      setProvisioningError(err.message || 'Failed to initiate certificate request');
    },
  });

  // Complete challenge mutation
  const completeCertMutation = useMutation({
    mutationFn: async (params: { body: { domain: string; challenge_id: string } }) => {
      const response = await fetch('/api/domains/challenge/complete', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(params.body),
        credentials: 'include',
      });
      if (!response.ok) {
        const error = await response.json();
        throw new Error(error.error || 'Failed to complete challenge');
      }
      return response.json();
    },
    onSuccess: () => {
      setProvisioningStep('success');
      queryClient.invalidateQueries({ queryKey: listCustomDomainsOptions().queryKey });
      toast.success('Certificate provisioned successfully!');
    },
    onError: (err) => {
      setProvisioningStep('error');
      setProvisioningError(err.message || 'Failed to verify domain ownership');
    },
  });

  // Manual upload mutation
  const uploadMutation = useMutation({
    ...uploadCustomDomainMutation(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: listCustomDomainsOptions().queryKey });
      toast.success('Custom domain certificate uploaded');
      closeAddDialog();
    },
    onError: (err) => {
      toast.error(err.message || 'Failed to upload certificate');
    },
  });

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

  const handleAddDomain = async (e: React.FormEvent) => {
    e.preventDefault();

    if (provisioningMethod === 'letsencrypt') {
      // Start ACME flow
      setProvisioningStep('verifying');
      requestCertMutation.mutate(domain);
    } else {
      // Manual upload
      uploadMutation.mutate({
        body: {
          domain,
          cert_pem: btoa(certPem),
          key_pem: btoa(keyPem),
          auto_renew: false,
        },
      });
    }
  };

  const handleDeleteDomain = () => {
    if (domainToDelete) {
      deleteMutation.mutate({ path: { domain: domainToDelete } });
    }
  };

  const closeAddDialog = () => {
    setShowAddDialog(false);
    setDomain('');
    setCertPem('');
    setKeyPem('');
    setProvisioningStep('input');
    setProvisioningError(null);
    setChallengeInfo(null);
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
            <Button onClick={() => setShowAddDialog(true)} className="gap-2">
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
            <Button onClick={() => setShowAddDialog(true)} size="lg" className="gap-2">
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
                      <Button
                        onClick={() => setDomainToDelete(d.domain)}
                        variant="destructive"
                        size="sm"
                        className="gap-2"
                      >
                        <Trash2 className="h-4 w-4" />
                        Delete
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Add Domain Dialog */}
      <Dialog open={showAddDialog} onOpenChange={(open) => !open && closeAddDialog()}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>
              {provisioningStep === 'success' ? 'Domain Added Successfully' : 'Add Custom Domain'}
            </DialogTitle>
            <DialogDescription>
              {provisioningStep === 'success'
                ? 'Your domain is now ready to use with tunnels.'
                : 'Add a custom domain with SSL certificate.'}
            </DialogDescription>
          </DialogHeader>

          {provisioningStep === 'input' && (
            <form onSubmit={handleAddDomain} className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="domain">Domain Name</Label>
                <Input
                  id="domain"
                  type="text"
                  value={domain}
                  onChange={(e) => setDomain(e.target.value)}
                  required
                  placeholder="api.example.com"
                />
                <p className="text-xs text-muted-foreground">
                  Enter the domain name you want to use (must point to this server)
                </p>
              </div>

              <Tabs value={provisioningMethod} onValueChange={(v) => setProvisioningMethod(v as 'letsencrypt' | 'manual')}>
                <TabsList className="grid w-full grid-cols-2">
                  <TabsTrigger value="letsencrypt" className="gap-2">
                    <RefreshCw className="h-4 w-4" />
                    Let's Encrypt
                  </TabsTrigger>
                  <TabsTrigger value="manual" className="gap-2">
                    <Shield className="h-4 w-4" />
                    Upload Certificate
                  </TabsTrigger>
                </TabsList>

                <TabsContent value="letsencrypt" className="space-y-4 mt-4">
                  <div className="bg-blue-500/10 border border-blue-500/30 rounded-lg p-4">
                    <h4 className="font-medium text-blue-500 mb-2">Automatic SSL via Let's Encrypt</h4>
                    <p className="text-sm text-blue-500/80">
                      We'll automatically provision a free SSL certificate. Make sure your domain's
                      DNS is already pointing to this server before proceeding.
                    </p>
                  </div>
                </TabsContent>

                <TabsContent value="manual" className="space-y-4 mt-4">
                  <div className="space-y-2">
                    <Label htmlFor="cert">Certificate (PEM)</Label>
                    <textarea
                      id="cert"
                      value={certPem}
                      onChange={(e) => setCertPem(e.target.value)}
                      required={provisioningMethod === 'manual'}
                      placeholder="-----BEGIN CERTIFICATE-----
...
-----END CERTIFICATE-----"
                      className="w-full h-24 px-3 py-2 bg-background border border-input rounded-md text-sm font-mono"
                    />
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="key">Private Key (PEM)</Label>
                    <textarea
                      id="key"
                      value={keyPem}
                      onChange={(e) => setKeyPem(e.target.value)}
                      required={provisioningMethod === 'manual'}
                      placeholder="-----BEGIN PRIVATE KEY-----
...
-----END PRIVATE KEY-----"
                      className="w-full h-24 px-3 py-2 bg-background border border-input rounded-md text-sm font-mono"
                    />
                  </div>
                </TabsContent>
              </Tabs>

              <DialogFooter>
                <Button type="button" variant="outline" onClick={closeAddDialog}>
                  Cancel
                </Button>
                <Button
                  type="submit"
                  disabled={requestCertMutation.isPending || uploadMutation.isPending}
                >
                  {requestCertMutation.isPending || uploadMutation.isPending ? 'Adding...' : 'Add Domain'}
                </Button>
              </DialogFooter>
            </form>
          )}

          {provisioningStep === 'verifying' && (
            <div className="py-8 text-center space-y-4">
              <Loader2 className="h-12 w-12 mx-auto animate-spin text-primary" />
              <div>
                <h3 className="font-medium text-lg">Verifying Domain Ownership</h3>
                <p className="text-muted-foreground mt-2">
                  We're verifying that you control <strong>{domain}</strong> and provisioning your SSL certificate.
                  This may take a moment...
                </p>
              </div>
              {challengeInfo && (
                <div className="bg-muted rounded-lg p-4 text-left text-sm">
                  <p className="font-medium mb-1">Challenge Token:</p>
                  <code className="text-xs break-all">{challengeInfo.token}</code>
                </div>
              )}
            </div>
          )}

          {provisioningStep === 'success' && (
            <div className="py-8 text-center space-y-4">
              <div className="w-16 h-16 mx-auto rounded-full bg-green-500/10 flex items-center justify-center">
                <CheckCircle2 className="h-8 w-8 text-green-500" />
              </div>
              <div>
                <h3 className="font-medium text-lg">Certificate Provisioned!</h3>
                <p className="text-muted-foreground mt-2">
                  Your domain <strong>{domain}</strong> is now ready to use with tunnels.
                </p>
              </div>
              <DialogFooter className="justify-center">
                <Button onClick={closeAddDialog}>Done</Button>
              </DialogFooter>
            </div>
          )}

          {provisioningStep === 'error' && (
            <div className="py-8 text-center space-y-4">
              <div className="w-16 h-16 mx-auto rounded-full bg-destructive/10 flex items-center justify-center">
                <AlertCircle className="h-8 w-8 text-destructive" />
              </div>
              <div>
                <h3 className="font-medium text-lg text-destructive">Provisioning Failed</h3>
                <p className="text-muted-foreground mt-2">
                  {provisioningError || 'An error occurred while provisioning the certificate.'}
                </p>
              </div>
              <div className="bg-muted rounded-lg p-4 text-left text-sm">
                <p className="font-medium mb-2">Troubleshooting tips:</p>
                <ul className="list-disc list-inside space-y-1 text-muted-foreground">
                  <li>Make sure your domain's DNS points to this server</li>
                  <li>Check that port 80 is accessible for the HTTP challenge</li>
                  <li>Wait for DNS propagation if you recently changed records</li>
                </ul>
              </div>
              <DialogFooter className="justify-center gap-2">
                <Button variant="outline" onClick={() => setProvisioningStep('input')}>Try Again</Button>
                <Button onClick={closeAddDialog}>Close</Button>
              </DialogFooter>
            </div>
          )}
        </DialogContent>
      </Dialog>

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
