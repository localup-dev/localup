import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useMutation } from '@tanstack/react-query';
import { Mail, Lock, Eye, EyeOff, AlertCircle, Loader2, Wand2, CheckCircle2 } from 'lucide-react';
import { loginMutation } from '../api/client/@tanstack/react-query.gen';
import { Input } from '../components/ui/input';
import { Label } from '../components/ui/label';
import { Button } from '../components/ui/button';
import { Logo } from '../components/Logo';
import { useAuthConfig } from '../contexts/AuthConfigContext';
import { client } from '../api/client/client.gen';

// SVG icons for social providers
function GoogleIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor" role="img" aria-label="Google">
      <title>Google</title>
      <path d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 0 1-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z" fill="#4285F4"/>
      <path d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" fill="#34A853"/>
      <path d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" fill="#FBBC05"/>
      <path d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" fill="#EA4335"/>
    </svg>
  );
}

function GitHubIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor" role="img" aria-label="GitHub">
      <title>GitHub</title>
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
    </svg>
  );
}

type LoginMode = 'password' | 'magic-link';

export default function Login() {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [socialLoading, setSocialLoading] = useState<string | null>(null);
  const [socialError, setSocialError] = useState<string | null>(null);
  const [loginMode, setLoginMode] = useState<LoginMode>('password');
  const [magicLinkSent, setMagicLinkSent] = useState(false);
  const [magicLinkLoading, setMagicLinkLoading] = useState(false);
  const [magicLinkError, setMagicLinkError] = useState<string | null>(null);
  const navigate = useNavigate();
  const { authConfig } = useAuthConfig();

  const magicLinkEnabled = authConfig?.magic_link_enabled ?? false;

  const mutation = useMutation({
    ...loginMutation(),
    onSuccess: () => {
      navigate('/dashboard');
    },
  });

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    mutation.mutate({
      body: { email, password },
    });
  };

  const handleMagicLinkSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setMagicLinkLoading(true);
    setMagicLinkError(null);

    try {
      const response = await client.post({
        url: '/api/auth/magic-link/send',
        body: { email },
        headers: { 'Content-Type': 'application/json' },
      });

      if (response.response.ok) {
        setMagicLinkSent(true);
      } else {
        const data = response.data as { message?: string } | undefined;
        setMagicLinkError(data?.message ?? 'Failed to send magic link');
      }
    } catch (err) {
      setMagicLinkError(err instanceof Error ? err.message : 'Failed to send magic link');
    } finally {
      setMagicLinkLoading(false);
    }
  };

  const handleSocialLogin = async (providerId: string) => {
    setSocialLoading(providerId);
    setSocialError(null);

    try {
      // Build the redirect URI for the OAuth callback (current origin + callback path)
      const redirectUri = `${window.location.origin}/oauth/callback`;

      // Get the authorization URL from our backend
      const response = await client.get({
        url: '/api/auth/oauth/{provider}/url',
        path: { provider: providerId },
        query: { redirect_uri: redirectUri },
      });

      const data = response.data as { url: string; state: string } | undefined;
      if (data?.url) {
        // Store state for CSRF verification when the callback returns
        sessionStorage.setItem('oauth_state', data.state);
        sessionStorage.setItem('oauth_provider', providerId);
        sessionStorage.setItem('oauth_redirect_uri', redirectUri);

        // Redirect user to the OAuth provider
        window.location.href = data.url;
      } else {
        setSocialError('Failed to get authorization URL');
      }
    } catch (err) {
      setSocialError(err instanceof Error ? err.message : 'Failed to initiate social login');
    } finally {
      setSocialLoading(null);
    }
  };

  const socialProviders = authConfig?.social_providers ?? [];
  const hasSocialLogin = socialProviders.length > 0;

  const providerIcon = (id: string) => {
    switch (id) {
      case 'google': return <GoogleIcon className="h-5 w-5" />;
      case 'github': return <GitHubIcon className="h-5 w-5" />;
      default: return null;
    }
  };

  // Shared error display for any login method
  const errorMessage = socialError || magicLinkError || (!hasSocialLogin && mutation.error ? (mutation.error.message || 'Login failed') : null);

  return (
    <div className="min-h-screen bg-background flex items-center justify-center px-4">
      <div className="max-w-md w-full">
        <div className="text-center mb-8">
          <div className="flex justify-center mb-4">
            <Logo size="lg" className="text-foreground" />
          </div>
          <p className="text-muted-foreground">Sign in to your account</p>
        </div>

        <div className="bg-card rounded-lg shadow-lg p-8 border border-border">
          {/* Social login buttons */}
          {hasSocialLogin && (
            <>
              <div className="space-y-3 mb-6">
                {socialProviders.map((provider) => (
                  <Button
                    key={provider.id}
                    type="button"
                    variant="outline"
                    className="w-full justify-center gap-3"
                    disabled={socialLoading !== null}
                    onClick={() => handleSocialLogin(provider.id)}
                  >
                    {socialLoading === provider.id ? (
                      <Loader2 className="h-5 w-5 animate-spin" />
                    ) : (
                      providerIcon(provider.id)
                    )}
                    Continue with {provider.name}
                  </Button>
                ))}
              </div>

              {/* Divider */}
              <div className="relative mb-6">
                <div className="absolute inset-0 flex items-center">
                  <span className="w-full border-t border-border" />
                </div>
                <div className="relative flex justify-center text-xs uppercase">
                  <span className="bg-card px-2 text-muted-foreground">Or continue with email</span>
                </div>
              </div>
            </>
          )}

          {/* Error display */}
          {errorMessage && (
            <div className="bg-destructive/10 border border-destructive/50 text-destructive px-4 py-3 rounded-lg flex items-center gap-2 mb-6">
              <AlertCircle className="h-4 w-4 flex-shrink-0" />
              <span className="text-sm">{errorMessage}</span>
            </div>
          )}

          {/* Login mode toggle (only shown when magic link is enabled) */}
          {magicLinkEnabled && (
            <div className="flex rounded-lg border border-border p-1 mb-6">
              <button
                type="button"
                className={`flex-1 text-sm py-2 px-3 rounded-md transition-colors ${
                  loginMode === 'password'
                    ? 'bg-primary text-primary-foreground font-medium'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
                onClick={() => {
                  setLoginMode('password');
                  setMagicLinkSent(false);
                  setMagicLinkError(null);
                }}
              >
                Password
              </button>
              <button
                type="button"
                className={`flex-1 text-sm py-2 px-3 rounded-md transition-colors ${
                  loginMode === 'magic-link'
                    ? 'bg-primary text-primary-foreground font-medium'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
                onClick={() => {
                  setLoginMode('magic-link');
                  mutation.reset();
                }}
              >
                Magic link
              </button>
            </div>
          )}

          {/* Password login form */}
          {loginMode === 'password' && (
            <form onSubmit={handleSubmit} className="space-y-6">
              <div className="space-y-2">
                <Label htmlFor="email">Email</Label>
                <div className="relative">
                  <Mail className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                  <Input
                    id="email"
                    type="email"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    required
                    className="pl-10"
                    placeholder="you@example.com"
                  />
                </div>
              </div>

              <div className="space-y-2">
                <Label htmlFor="password">Password</Label>
                <div className="relative">
                  <Lock className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                  <Input
                    id="password"
                    type={showPassword ? 'text' : 'password'}
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    required
                    className="pl-10 pr-10"
                    placeholder="Enter your password"
                  />
                  <button
                    type="button"
                    onClick={() => setShowPassword(!showPassword)}
                    className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                  >
                    {showPassword ? (
                      <EyeOff className="h-4 w-4" />
                    ) : (
                      <Eye className="h-4 w-4" />
                    )}
                  </button>
                </div>
              </div>

              <Button
                type="submit"
                disabled={mutation.isPending}
                className="w-full"
              >
                {mutation.isPending ? (
                  <>
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                    Signing in...
                  </>
                ) : (
                  'Sign in'
                )}
              </Button>
            </form>
          )}

          {/* Magic link form */}
          {loginMode === 'magic-link' && (
            <>
              {magicLinkSent ? (
                <div className="text-center py-4 space-y-4">
                  <div className="flex justify-center">
                    <CheckCircle2 className="h-12 w-12 text-green-500" />
                  </div>
                  <div>
                    <h3 className="font-medium text-foreground">Check your email</h3>
                    <p className="text-sm text-muted-foreground mt-1">
                      We sent a sign-in link to <strong>{email}</strong>. Click the link in the email to sign in.
                    </p>
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    className="w-full"
                    onClick={() => {
                      setMagicLinkSent(false);
                      setMagicLinkError(null);
                    }}
                  >
                    Send another link
                  </Button>
                </div>
              ) : (
                <form onSubmit={handleMagicLinkSubmit} className="space-y-6">
                  <div className="space-y-2">
                    <Label htmlFor="magic-email">Email</Label>
                    <div className="relative">
                      <Mail className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
                      <Input
                        id="magic-email"
                        type="email"
                        value={email}
                        onChange={(e) => setEmail(e.target.value)}
                        required
                        className="pl-10"
                        placeholder="you@example.com"
                      />
                    </div>
                  </div>

                  <Button
                    type="submit"
                    disabled={magicLinkLoading}
                    className="w-full"
                  >
                    {magicLinkLoading ? (
                      <>
                        <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                        Sending link...
                      </>
                    ) : (
                      <>
                        <Wand2 className="h-4 w-4 mr-2" />
                        Send magic link
                      </>
                    )}
                  </Button>

                  <p className="text-xs text-center text-muted-foreground">
                    We'll email you a link to sign in without a password.
                  </p>
                </form>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
