import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { listAuthTokensOptions, createAuthTokenMutation, deleteAuthTokenMutation } from '../api/client/@tanstack/react-query.gen';
import { Button } from '../components/ui/button';
import { Badge } from '../components/ui/badge';
import { useTeam } from '../contexts/TeamContext';

export default function AuthTokens() {
  const { selectedTeam } = useTeam();
  const queryClient = useQueryClient();
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [createdToken, setCreatedToken] = useState<string | null>(null);
  const [createdTokenExpiresAt, setCreatedTokenExpiresAt] = useState<string | null>(null);
  const [tokenName, setTokenName] = useState('');
  const [tokenDescription, setTokenDescription] = useState('');
  const [expiresInDays, setExpiresInDays] = useState<number | null>(null);

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
      // Invalidate and refetch tokens list
      queryClient.invalidateQueries({ queryKey: listAuthTokensOptions().queryKey });
    },
  });

  const deleteMutation = useMutation({
    ...deleteAuthTokenMutation(),
    onSuccess: () => {
      // Invalidate and refetch tokens list
      queryClient.invalidateQueries({ queryKey: listAuthTokensOptions().queryKey });
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

  const handleDeleteToken = async (id: string) => {
    if (!confirm('Are you sure you want to delete this token? This action cannot be undone.')) {
      return;
    }
    deleteMutation.mutate({ path: { id } });
  };

  const copyToken = () => {
    if (createdToken) {
      navigator.clipboard.writeText(createdToken);
    }
  };

  return (
    <div className="min-h-screen bg-gray-900 text-white">
      {/* Header */}
      <div className="border-b border-gray-800">
        <div className="max-w-7xl mx-auto px-6 py-6">
          <div className="flex items-center justify-between">
            <div>
              <h1 className="text-3xl font-bold">Auth Tokens</h1>
              <p className="text-gray-400 mt-2">
                Create and manage authentication tokens for your tunnels
              </p>
            </div>
            <Button
              onClick={() => setShowCreateModal(true)}
            >
              + New Auth Token
            </Button>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="max-w-7xl mx-auto px-6 py-8">
        {(error || createMutation.error || deleteMutation.error) && (
          <div className="mb-6 bg-red-900/50 border border-red-500 text-red-200 px-4 py-3 rounded">
            {error?.message || createMutation.error?.message || deleteMutation.error?.message || 'An error occurred'}
          </div>
        )}

        {isLoading ? (
          <div className="text-center py-12 text-gray-400">Loading tokens...</div>
        ) : tokens.length === 0 ? (
          <div className="bg-gray-800 rounded-lg p-12 text-center">
            <div className="text-6xl mb-4">🔑</div>
            <h2 className="text-2xl font-bold mb-2">No auth tokens yet</h2>
            <p className="text-gray-400 mb-6">
              Create your first auth token to start using LocalUp tunnels
            </p>
            <Button
              onClick={() => setShowCreateModal(true)}
              size="lg"
            >
              Create Auth Token
            </Button>
          </div>
        ) : (
          <div className="bg-gray-800 rounded-lg overflow-hidden">
            <table className="w-full">
              <thead className="bg-gray-900">
                <tr>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider">
                    Name
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider">
                    Description
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider">
                    Created
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider">
                    Last Used
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider">
                    Expires
                  </th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider">
                    Status
                  </th>
                  <th className="px-6 py-3 text-right text-xs font-medium text-gray-400 uppercase tracking-wider">
                    Actions
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-700">
                {tokens.map((token) => (
                  <tr key={token.id}>
                    <td className="px-6 py-4 whitespace-nowrap text-sm font-medium">{token.name}</td>
                    <td className="px-6 py-4 text-sm text-gray-400">{token.description || '-'}</td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-400">
                      {new Date(token.created_at).toLocaleDateString()}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-400">
                      {token.last_used_at ? new Date(token.last_used_at).toLocaleDateString() : 'Never'}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-400">
                      {token.expires_at ? new Date(token.expires_at).toLocaleDateString() : 'Never'}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap">
                      <Badge variant={token.is_active ? 'success' : 'secondary'}>
                        {token.is_active ? 'Active' : 'Inactive'}
                      </Badge>
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-right text-sm">
                      <Button
                        onClick={() => handleDeleteToken(token.id)}
                        variant="destructive"
                        size="sm"
                      >
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

      {/* Create Token Modal */}
      {showCreateModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-gray-800 rounded-lg p-8 max-w-lg w-full mx-4">
            <h2 className="text-2xl font-bold mb-6">Create New Auth Token</h2>

            {createdToken ? (
              <div className="space-y-4">
                <div className="bg-yellow-900/20 border border-yellow-500/50 rounded-md p-4">
                  <p className="text-yellow-200 text-sm font-medium mb-2">⚠️ Important</p>
                  <p className="text-yellow-200/80 text-sm">
                    This is the only time you'll see this token. Copy it now and store it securely.
                  </p>
                </div>

                <div>
                  <label className="block text-sm font-medium text-gray-300 mb-2">
                    Your Auth Token
                  </label>
                  <div className="bg-gray-900 rounded-md p-4 flex items-center justify-between">
                    <code className="text-blue-400 font-mono text-sm break-all mr-2">{createdToken}</code>
                    <Button
                      onClick={copyToken}
                      variant="secondary"
                      size="sm"
                      className="flex-shrink-0"
                    >
                      📋 Copy
                    </Button>
                  </div>
                </div>

                {createdTokenExpiresAt && (
                  <div className="bg-gray-700/50 rounded-md p-3">
                    <p className="text-sm text-gray-300">
                      <span className="font-medium">Expires:</span>{' '}
                      {new Date(createdTokenExpiresAt).toLocaleString()}
                    </p>
                  </div>
                )}

                <div className="flex justify-end">
                  <Button
                    onClick={() => {
                      setCreatedToken(null);
                      setCreatedTokenExpiresAt(null);
                      setShowCreateModal(false);
                    }}
                  >
                    Done
                  </Button>
                </div>
              </div>
            ) : (
              <form onSubmit={handleCreateToken} className="space-y-4">
                <div>
                  <label htmlFor="name" className="block text-sm font-medium text-gray-300 mb-2">
                    Token Name
                  </label>
                  <input
                    id="name"
                    type="text"
                    value={tokenName}
                    onChange={(e) => setTokenName(e.target.value)}
                    required
                    className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-md text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500"
                    placeholder="Production API Token"
                  />
                </div>

                <div>
                  <label htmlFor="description" className="block text-sm font-medium text-gray-300 mb-2">
                    Description (Optional)
                  </label>
                  <textarea
                    id="description"
                    value={tokenDescription}
                    onChange={(e) => setTokenDescription(e.target.value)}
                    rows={3}
                    className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-md text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500"
                    placeholder="Token for production tunnels"
                  />
                </div>

                <div>
                  <label htmlFor="expiresInDays" className="block text-sm font-medium text-gray-300 mb-2">
                    Expires In (Days)
                  </label>
                  <input
                    id="expiresInDays"
                    type="number"
                    min="1"
                    value={expiresInDays || ''}
                    onChange={(e) => setExpiresInDays(e.target.value ? parseInt(e.target.value, 10) : null)}
                    className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-md text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500"
                    placeholder="Leave empty for no expiration"
                  />
                  <p className="mt-1 text-xs text-gray-400">
                    Leave empty for a token that never expires
                  </p>
                </div>

                <div className="flex justify-end gap-3">
                  <Button
                    type="button"
                    onClick={() => setShowCreateModal(false)}
                    variant="outline"
                  >
                    Cancel
                  </Button>
                  <Button
                    type="submit"
                    disabled={createMutation.isPending}
                  >
                    {createMutation.isPending ? 'Creating...' : 'Create Token'}
                  </Button>
                </div>
              </form>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
