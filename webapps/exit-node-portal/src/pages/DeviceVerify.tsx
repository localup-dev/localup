import { useState, useEffect } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Smartphone, CheckCircle2, XCircle, Loader2, AlertCircle } from 'lucide-react';
import { Input } from '../components/ui/input';
import { Label } from '../components/ui/label';
import { Button } from '../components/ui/button';
import { client } from '../api/client/client.gen';

interface DeviceInfo {
  valid: boolean;
  client_id?: string;
  client_name?: string;
  scope?: string;
  message: string;
}

type VerifyState = 'input' | 'loading' | 'confirm' | 'approved' | 'denied' | 'error';

export default function DeviceVerify() {
  const [searchParams] = useSearchParams();
  const initialCode = searchParams.get('code') || '';

  const [userCode, setUserCode] = useState(initialCode);
  const [state, setState] = useState<VerifyState>(initialCode ? 'loading' : 'input');
  const [deviceInfo, setDeviceInfo] = useState<DeviceInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState(false);

  // Format the code as user types (auto-insert dash)
  const formatCode = (raw: string): string => {
    const clean = raw.toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 8);
    if (clean.length > 4) {
      return `${clean.slice(0, 4)}-${clean.slice(4)}`;
    }
    return clean;
  };

  const handleCodeChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setUserCode(formatCode(e.target.value));
    setError(null);
  };

  // Look up device authorization info
  const lookupCode = async (code: string) => {
    setState('loading');
    setError(null);
    try {
      const response = await client.get<{ body: DeviceInfo }>({
        url: '/api/device/info',
        query: { user_code: code },
      });
      const info = response.data as unknown as DeviceInfo;
      setDeviceInfo(info);
      if (info.valid) {
        setState('confirm');
      } else {
        setState('input');
        setError(info.message);
      }
    } catch {
      setState('input');
      setError('Failed to verify code. Please try again.');
    }
  };

  // Auto-lookup if code is provided in URL (only on mount)
  useEffect(() => {
    if (initialCode) {
      lookupCode(initialCode);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleSubmitCode = (e: React.FormEvent) => {
    e.preventDefault();
    const clean = userCode.replace(/[^A-Z0-9]/gi, '');
    if (clean.length !== 8) {
      setError('Please enter a valid 8-character code');
      return;
    }
    lookupCode(userCode);
  };

  const handleApprove = async () => {
    setActionLoading(true);
    try {
      const response = await client.post<{ body: { approved: boolean; message: string } }>({
        url: '/api/device/verify',
        body: { user_code: userCode },
        headers: { 'Content-Type': 'application/json' },
      });
      const result = response.data as unknown as { approved: boolean; message: string };
      if (result.approved) {
        setState('approved');
      } else {
        setError('Failed to approve. Please try again.');
      }
    } catch (err) {
      setError('Failed to approve authorization. Please try again.');
      console.error('Device verify failed:', err);
    } finally {
      setActionLoading(false);
    }
  };

  const handleDeny = async () => {
    setActionLoading(true);
    try {
      await client.post({
        url: '/api/device/deny',
        body: { user_code: userCode },
        headers: { 'Content-Type': 'application/json' },
      });
      setState('denied');
    } catch (err) {
      setError('Failed to deny authorization. Please try again.');
      console.error('Device deny failed:', err);
    } finally {
      setActionLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-background">
      <div className="border-b border-border">
        <div className="max-w-2xl mx-auto px-6 py-6">
          <div className="flex items-center gap-3">
            <Smartphone className="h-6 w-6 text-primary" />
            <h1 className="text-2xl font-bold text-foreground">Device Authorization</h1>
          </div>
          <p className="text-muted-foreground mt-2">
            Authorize a device or application to access your account
          </p>
        </div>
      </div>

      <div className="max-w-md mx-auto px-6 py-12">
        {/* Input State: Enter code */}
        {state === 'input' && (
          <form onSubmit={handleSubmitCode} className="space-y-6">
            <div className="bg-card rounded-lg border border-border p-6 space-y-4">
              <div className="text-center mb-2">
                <p className="text-sm text-muted-foreground">
                  Enter the code shown on your device or application
                </p>
              </div>
              <div className="space-y-2">
                <Label htmlFor="user-code">Device Code</Label>
                <Input
                  id="user-code"
                  type="text"
                  value={userCode}
                  onChange={handleCodeChange}
                  placeholder="XXXX-YYYY"
                  className="text-center text-2xl font-mono tracking-widest"
                  maxLength={9}
                  autoFocus
                  autoComplete="off"
                />
              </div>
              {error && (
                <div className="flex items-center gap-2 text-sm text-destructive">
                  <AlertCircle className="h-4 w-4 shrink-0" />
                  {error}
                </div>
              )}
              <Button type="submit" className="w-full" disabled={userCode.replace(/[^A-Z0-9]/gi, '').length !== 8}>
                Continue
              </Button>
            </div>
          </form>
        )}

        {/* Loading State */}
        {state === 'loading' && (
          <div className="bg-card rounded-lg border border-border p-8 text-center space-y-4">
            <Loader2 className="h-8 w-8 animate-spin mx-auto text-primary" />
            <p className="text-muted-foreground">Verifying code...</p>
          </div>
        )}

        {/* Confirm State: Show app info and approve/deny */}
        {state === 'confirm' && deviceInfo && (
          <div className="bg-card rounded-lg border border-border p-6 space-y-6">
            <div className="text-center space-y-2">
              <Smartphone className="h-12 w-12 mx-auto text-primary" />
              <h2 className="text-lg font-semibold text-foreground">Authorize Application</h2>
              <p className="text-sm text-muted-foreground">
                The following application is requesting access to your account:
              </p>
            </div>

            <div className="bg-muted rounded-md p-4 space-y-2">
              <div className="flex justify-between items-center">
                <span className="text-sm text-muted-foreground">Application</span>
                <span className="text-sm font-medium text-foreground">
                  {deviceInfo.client_name || <span className="font-mono">{deviceInfo.client_id}</span>}
                </span>
              </div>
              {deviceInfo.client_name && deviceInfo.client_id && (
                <div className="flex justify-between items-center">
                  <span className="text-sm text-muted-foreground">Client ID</span>
                  <span className="text-sm font-medium text-foreground font-mono">{deviceInfo.client_id}</span>
                </div>
              )}
              {deviceInfo.scope && (
                <div className="flex justify-between items-center">
                  <span className="text-sm text-muted-foreground">Scopes</span>
                  <span className="text-sm font-medium text-foreground">{deviceInfo.scope}</span>
                </div>
              )}
              <div className="flex justify-between items-center">
                <span className="text-sm text-muted-foreground">Code</span>
                <span className="text-sm font-mono font-medium text-foreground">{userCode}</span>
              </div>
            </div>

            {error && (
              <div className="flex items-center gap-2 text-sm text-destructive">
                <AlertCircle className="h-4 w-4 shrink-0" />
                {error}
              </div>
            )}

            <div className="flex gap-3">
              <Button
                variant="outline"
                className="flex-1"
                onClick={handleDeny}
                disabled={actionLoading}
              >
                {actionLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Deny'}
              </Button>
              <Button
                className="flex-1"
                onClick={handleApprove}
                disabled={actionLoading}
              >
                {actionLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Approve'}
              </Button>
            </div>
          </div>
        )}

        {/* Approved State */}
        {state === 'approved' && (
          <div className="bg-card rounded-lg border border-border p-8 text-center space-y-4">
            <CheckCircle2 className="h-12 w-12 mx-auto text-green-500" />
            <h2 className="text-lg font-semibold text-foreground">Device Authorized</h2>
            <p className="text-sm text-muted-foreground">
              The application has been authorized. You can close this page and return to your device.
            </p>
          </div>
        )}

        {/* Denied State */}
        {state === 'denied' && (
          <div className="bg-card rounded-lg border border-border p-8 text-center space-y-4">
            <XCircle className="h-12 w-12 mx-auto text-destructive" />
            <h2 className="text-lg font-semibold text-foreground">Authorization Denied</h2>
            <p className="text-sm text-muted-foreground">
              The authorization request has been denied. The application will not be granted access.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
