import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Key, Plus, Trash2, Copy, Check, Eye, EyeOff, AlertTriangle } from 'lucide-react';
import { listAuthTokensOptions, createAuthTokenMutation, deleteAuthTokenMutation } from '../api/client/@tanstack/react-query.gen';
import { Button } from '../components/ui/button';
import { Badge } from '../components/ui/badge';
import { Input } from '../components/ui/input';
import { Label } from '../components/ui/label';
import { Skeleton } from '../components/ui/skeleton';
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
import { useTeam } from '../contexts/TeamContext';

export default function AuthTokens() {
  const { selectedTeam } = useTeam();
  const queryClient = useQueryClient();
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [createdToken, setCreatedToken] = useState<string | null>(null);
  const [createdTokenExpiresAt, setCreatedTokenExpiresAt] = useState<string | null>(null);
  const [tokenName, setTokenName] = useState('');
  const [tokenDescription, setTokenDescription] = useState('');
  const [expiresInDays, setExpiresInDays] = useState<number | null>(null);
  const [tokenToDelete, setTokenToDelete] = useState<{ id: string; name: string } | null>(null);
  const [showToken, setShowToken] = useState(false);
  const [copied, setCopied] = useState(false);

  const { data, isLoading, error } = useQuery(listAuthTokensOptions());
  const tokens = data?.tokens || [];

  const createMutation = useMutation({
    ...createAuthTokenMutation(),
    onSuccess: (response) => {
      setCreatedToken(response.token);
      setCreatedTokenExpiresAt(response.expires_at || null);
      setTokenName('');
      setTokenDescription('');
      setExpiresInDays(null);
      queryClient.invalidateQueries({ queryKey: listAuthTokensOptions().queryKey });
      toast.success('Token created successfully');
    },
    onError: (err) => {
      toast.error(err.message || 'Failed to create token');
    },
  });

  const deleteMutation = useMutation({
    ...deleteAuthTokenMutation(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: listAuthTokensOptions().queryKey });
      toast.success('Token deleted');
      setTokenToDelete(null);
    },
    onError: (err) => {
      toast.error(err.message || 'Failed to delete token');
    },
  });

  const handleCreateToken = async (e: React.FormEvent) => {
    e.preventDefault();
    createMutation.mutate({
      body: {
        name: tokenName,
        description: tokenDescription || null,
        expires_in_days: expiresInDays,
        team_id: selectedTeam?.id || null,
      },
    });
  };

  const handleDeleteToken = () => {
    if (tokenToDelete) {
      deleteMutation.mutate({ path: { id: tokenToDelete.id } });
    }
  };

  const copyToken = async () => {
    if (createdToken) {
      await navigator.clipboard.writeText(createdToken);
      setCopied(true);
      toast.success('Copied to clipboard');
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const closeCreateDialog = () => {
    setShowCreateDialog(false);
    setCreatedToken(null);
    setCreatedTokenExpiresAt(null);
    setShowToken(false);
    setCopied(false);
  };

  return (
    <div className="min-h-screen bg-background text-foreground">
      {/* Header */}
      <div className="border-b border-border">
        <div className="max-w-7xl mx-auto px-6 py-6">
          <div className="flex items-center justify-between">
            <div>
              <h1 className="text-3xl font-bold">Auth Tokens</h1>
              <p className="text-muted-foreground mt-2">
                Create and manage authentication tokens for your tunnels
              </p>
            </div>
            <Button onClick={() => setShowCreateDialog(true)} className="gap-2">
              <Plus className="h-4 w-4" />
              New Auth Token
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
              <div className="grid grid-cols-7 gap-4">
                {['Name', 'Description', 'Created', 'Last Used', 'Expires', 'Status', 'Actions'].map((h) => (
                  <Skeleton key={h} className="h-4 w-16" />
                ))}
              </div>
            </div>
            <div className="divide-y divide-border">
              {[1, 2, 3].map((i) => (
                <div key={i} className="px-6 py-4 grid grid-cols-7 gap-4 items-center">
                  <Skeleton className="h-4 w-24" />
                  <Skeleton className="h-4 w-32" />
                  <Skeleton className="h-4 w-20" />
                  <Skeleton className="h-4 w-16" />
                  <Skeleton className="h-4 w-16" />
                  <Skeleton className="h-6 w-16 rounded-full" />
                  <Skeleton className="h-8 w-16" />
                </div>
              ))}
            </div>
          </div>
        ) : tokens.length === 0 ? (
          <div className="bg-card rounded-lg border border-border p-12 text-center">
            <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center">
              <Key className="h-8 w-8 text-muted-foreground" />
            </div>
            <h2 className="text-2xl font-bold mb-2">No auth tokens yet</h2>
            <p className="text-muted-foreground mb-6">
              Create your first auth token to start using LocalUp tunnels
            </p>
            <Button onClick={() => setShowCreateDialog(true)} size="lg" className="gap-2">
              <Plus className="h-4 w-4" />
              Create Auth Token
            </Button>
          </div>
        ) : (
          <div className="bg-card rounded-lg border border-border overflow-hidden">
            <table className="w-full">
              <thead className="bg-muted">
                <tr>
                  <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Name
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Description
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Created
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Last Used
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Expires
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Status
                  </th>
                  <th className="px-6 py-3 text-right text-xs font-medium text-muted-foreground uppercase tracking-wider">
                    Actions
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {tokens.map((token) => (
                  <tr key={token.id} className="hover:bg-muted/50 transition-colors">
                    <td className="px-6 py-4 whitespace-nowrap text-sm font-medium">{token.name}</td>
                    <td className="px-6 py-4 text-sm text-muted-foreground">{token.description || '-'}</td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-muted-foreground">
                      {new Date(token.created_at).toLocaleDateString()}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-muted-foreground">
                      {token.last_used_at ? new Date(token.last_used_at).toLocaleDateString() : 'Never'}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-muted-foreground">
                      {token.expires_at ? new Date(token.expires_at).toLocaleDateString() : 'Never'}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap">
                      <Badge variant={token.is_active ? 'success' : 'secondary'}>
                        {token.is_active ? 'Active' : 'Inactive'}
                      </Badge>
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-right text-sm">
                      <Button
                        onClick={() => setTokenToDelete({ id: token.id, name: token.name })}
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

      {/* Create Token Dialog */}
      <Dialog open={showCreateDialog} onOpenChange={(open) => !open && closeCreateDialog()}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>
              {createdToken ? 'Token Created Successfully' : 'Create New Auth Token'}
            </DialogTitle>
            <DialogDescription>
              {createdToken
                ? 'Copy your token now. You won\'t be able to see it again.'
                : 'Create a new authentication token for your tunnels.'}
            </DialogDescription>
          </DialogHeader>

          {createdToken ? (
            <div className="space-y-4">
              <div className="bg-yellow-500/10 border border-yellow-500/50 rounded-lg p-4 flex items-start gap-3">
                <AlertTriangle className="h-5 w-5 text-yellow-500 flex-shrink-0 mt-0.5" />
                <div>
                  <p className="text-yellow-500 font-medium text-sm">Important</p>
                  <p className="text-yellow-500/80 text-sm mt-1">
                    This is the only time you'll see this token. Copy it now and store it securely.
                  </p>
                </div>
              </div>

              <div className="space-y-2">
                <Label>Your Auth Token</Label>
                <div className="bg-muted rounded-lg p-4 flex items-center gap-3">
                  <code className="text-primary font-mono text-sm break-all flex-1">
                    {showToken ? createdToken : '•'.repeat(Math.min(createdToken.length, 40))}
                  </code>
                  <div className="flex gap-2 flex-shrink-0">
                    <Button
                      onClick={() => setShowToken(!showToken)}
                      variant="ghost"
                      size="sm"
                    >
                      {showToken ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                    </Button>
                    <Button
                      onClick={copyToken}
                      variant="secondary"
                      size="sm"
                      className="gap-2"
                    >
                      {copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
                      {copied ? 'Copied' : 'Copy'}
                    </Button>
                  </div>
                </div>
              </div>

              {createdTokenExpiresAt && (
                <div className="bg-muted/50 rounded-lg p-3">
                  <p className="text-sm text-muted-foreground">
                    <span className="font-medium">Expires:</span>{' '}
                    {new Date(createdTokenExpiresAt).toLocaleString()}
                  </p>
                </div>
              )}

              <DialogFooter>
                <Button onClick={closeCreateDialog}>Done</Button>
              </DialogFooter>
            </div>
          ) : (
            <form onSubmit={handleCreateToken} className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="name">Token Name</Label>
                <Input
                  id="name"
                  type="text"
                  value={tokenName}
                  onChange={(e) => setTokenName(e.target.value)}
                  required
                  placeholder="Production API Token"
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="description">Description (Optional)</Label>
                <Input
                  id="description"
                  value={tokenDescription}
                  onChange={(e) => setTokenDescription(e.target.value)}
                  placeholder="Token for production tunnels"
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="expiresInDays">Expires In (Days)</Label>
                <Input
                  id="expiresInDays"
                  type="number"
                  min="1"
                  value={expiresInDays || ''}
                  onChange={(e) => setExpiresInDays(e.target.value ? parseInt(e.target.value, 10) : null)}
                  placeholder="Leave empty for no expiration"
                />
                <p className="text-xs text-muted-foreground">
                  Leave empty for a token that never expires
                </p>
              </div>

              <DialogFooter>
                <Button type="button" variant="outline" onClick={closeCreateDialog}>
                  Cancel
                </Button>
                <Button type="submit" disabled={createMutation.isPending}>
                  {createMutation.isPending ? 'Creating...' : 'Create Token'}
                </Button>
              </DialogFooter>
            </form>
          )}
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation Dialog */}
      <AlertDialog open={!!tokenToDelete} onOpenChange={(open) => !open && setTokenToDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Token</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete the token "{tokenToDelete?.name}"? This action cannot be undone.
              Any tunnels using this token will no longer be able to authenticate.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDeleteToken}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {deleteMutation.isPending ? 'Deleting...' : 'Delete Token'}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
