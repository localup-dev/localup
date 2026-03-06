import { useEffect, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { Loader2, AlertCircle } from 'lucide-react';
import { Button } from '../components/ui/button';
import { Logo } from '../components/Logo';
import { client } from '../api/client/client.gen';

export default function OAuthCallback() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const [error, setError] = useState<string | null>(null);
  const [processing, setProcessing] = useState(true);

  useEffect(() => {
    const exchangeCode = async () => {
      const code = searchParams.get('code');
      const state = searchParams.get('state');
      const errorParam = searchParams.get('error');
      const errorDescription = searchParams.get('error_description');

      // Handle OAuth provider errors
      if (errorParam) {
        setError(errorDescription || `OAuth error: ${errorParam}`);
        setProcessing(false);
        return;
      }

      if (!code || !state) {
        setError('Missing authorization code or state parameter');
        setProcessing(false);
        return;
      }

      // Verify CSRF state
      const savedState = sessionStorage.getItem('oauth_state');
      const provider = sessionStorage.getItem('oauth_provider');
      const redirectUri = sessionStorage.getItem('oauth_redirect_uri');

      if (!savedState || !provider || !redirectUri) {
        setError('OAuth session expired. Please try logging in again.');
        setProcessing(false);
        return;
      }

      if (state !== savedState) {
        setError('State mismatch - possible CSRF attack. Please try again.');
        setProcessing(false);
        return;
      }

      // Clear OAuth session data
      sessionStorage.removeItem('oauth_state');
      sessionStorage.removeItem('oauth_provider');
      sessionStorage.removeItem('oauth_redirect_uri');

      try {
        // Exchange the code with our backend
        const response = await client.post({
          url: '/api/auth/oauth/{provider}/callback',
          path: { provider },
          body: {
            code,
            state,
            redirect_uri: redirectUri,
          },
          headers: {
            'Content-Type': 'application/json',
          },
        });

        if (response.error) {
          const errData = response.error as { error?: string };
          setError(errData?.error || 'Authentication failed');
          setProcessing(false);
          return;
        }

        // Success - navigate to dashboard
        navigate('/dashboard', { replace: true });
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to complete authentication');
        setProcessing(false);
      }
    };

    exchangeCode();
  }, [searchParams, navigate]);

  if (processing) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center px-4">
        <div className="max-w-md w-full text-center">
          <div className="flex justify-center mb-6">
            <Logo size="lg" className="text-foreground" />
          </div>
          <div className="bg-card rounded-lg shadow-lg p-8 border border-border">
            <Loader2 className="h-8 w-8 animate-spin mx-auto mb-4 text-primary" />
            <p className="text-muted-foreground">Completing sign in...</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-background flex items-center justify-center px-4">
      <div className="max-w-md w-full text-center">
        <div className="flex justify-center mb-6">
          <Logo size="lg" className="text-foreground" />
        </div>
        <div className="bg-card rounded-lg shadow-lg p-8 border border-border">
          <div className="bg-destructive/10 border border-destructive/50 text-destructive px-4 py-3 rounded-lg flex items-center gap-2 mb-6">
            <AlertCircle className="h-4 w-4 flex-shrink-0" />
            <span className="text-sm">{error}</span>
          </div>
          <Button
            type="button"
            onClick={() => navigate('/login', { replace: true })}
            className="w-full"
          >
            Back to login
          </Button>
        </div>
      </div>
    </div>
  );
}
