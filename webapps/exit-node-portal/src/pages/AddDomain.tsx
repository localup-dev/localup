import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import {
  ArrowLeft,
  Globe,
  Shield,
  RefreshCw,
  Server,
  Copy,
  CheckCircle2,
  AlertCircle,
  ExternalLink,
  FileText,
  Network
} from 'lucide-react';
import { Button } from '../components/ui/button';
import { Input } from '../components/ui/input';
import { Label } from '../components/ui/label';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../components/ui/tabs';
import { CodeBlock } from '../components/CodeBlock';

type ChallengeType = 'http-01' | 'dns-01';
type ProvisioningMethod = 'letsencrypt' | 'manual';
type Step = 'input' | 'challenge' | 'verifying' | 'success' | 'error';

// Challenge info matches the API's tagged enum format
// The API uses serde's tag = "type", rename_all = "lowercase"
interface Http01ChallengeInfo {
  type: 'http01';
  domain: string;
  token: string;
  key_authorization: string;
  file_path: string;
  instructions: string[];
}

interface Dns01ChallengeInfo {
  type: 'dns01';
  domain: string;
  record_name: string;
  record_value: string;
  instructions: string[];
}

type ChallengeInfo = Http01ChallengeInfo | Dns01ChallengeInfo;

interface ChallengeResponse {
  domain: string;
  challenge_id: string;
  expires_at: string;
  challenge: ChallengeInfo;
}

export default function AddDomain() {
  const navigate = useNavigate();

  // Form state
  const [domain, setDomain] = useState('');
  const [provisioningMethod, setProvisioningMethod] = useState<ProvisioningMethod>('letsencrypt');
  const [challengeType, setChallengeType] = useState<ChallengeType>('http-01');
  const [certPem, setCertPem] = useState('');
  const [keyPem, setKeyPem] = useState('');

  // Flow state
  const [step, setStep] = useState<Step>('input');
  const [challengeData, setChallengeData] = useState<ChallengeResponse | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // Initiate challenge mutation
  const initiateMutation = useMutation({
    mutationFn: async (params: { domain: string; challenge_type: string }) => {
      const response = await fetch('/api/domains/challenge/initiate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(params),
        credentials: 'include',
      });
      if (!response.ok) {
        const error = await response.json();
        throw new Error(error.error || 'Failed to initiate challenge');
      }
      return response.json() as Promise<ChallengeResponse>;
    },
    onSuccess: (data) => {
      setChallengeData(data);
      setStep('challenge');
      toast.success('Challenge initiated! Follow the instructions below.');
    },
    onError: (err: Error) => {
      setErrorMessage(err.message);
      setStep('error');
    },
  });

  // Complete challenge mutation
  const completeMutation = useMutation({
    mutationFn: async (params: { domain: string; challenge_id: string }) => {
      const response = await fetch('/api/domains/challenge/complete', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(params),
        credentials: 'include',
      });
      if (!response.ok) {
        const error = await response.json();
        throw new Error(error.error || 'Failed to complete challenge');
      }
      return response.json();
    },
    onSuccess: () => {
      setStep('success');
      toast.success('Certificate provisioned successfully!');
    },
    onError: (err: Error) => {
      setErrorMessage(err.message);
      setStep('error');
    },
  });

  // Manual upload mutation
  const uploadMutation = useMutation({
    mutationFn: async (params: { domain: string; cert_pem: string; key_pem: string }) => {
      const response = await fetch('/api/domains/upload', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          domain: params.domain,
          cert_pem: btoa(params.cert_pem),
          key_pem: btoa(params.key_pem),
          auto_renew: false,
        }),
        credentials: 'include',
      });
      if (!response.ok) {
        const error = await response.json();
        throw new Error(error.error || 'Failed to upload certificate');
      }
      return response.json();
    },
    onSuccess: () => {
      setStep('success');
      toast.success('Certificate uploaded successfully!');
    },
    onError: (err: Error) => {
      setErrorMessage(err.message);
      setStep('error');
    },
  });

  const handleInitiateChallenge = (e: React.FormEvent) => {
    e.preventDefault();

    if (provisioningMethod === 'manual') {
      uploadMutation.mutate({ domain, cert_pem: certPem, key_pem: keyPem });
    } else {
      initiateMutation.mutate({ domain, challenge_type: challengeType });
    }
  };

  const handleCompleteChallenge = () => {
    if (!challengeData) return;
    setStep('verifying');
    completeMutation.mutate({
      domain: challengeData.domain,
      challenge_id: challengeData.challenge_id,
    });
  };

  const copyToClipboard = (text: string, label: string) => {
    navigator.clipboard.writeText(text);
    toast.success(`${label} copied to clipboard`);
  };

  const getServerIP = () => {
    // In production, this would come from the API
    return window.location.hostname === 'localhost' ? '127.0.0.1' : window.location.hostname;
  };

  return (
    <div className="min-h-screen bg-background text-foreground">
      {/* Header */}
      <div className="border-b border-border">
        <div className="max-w-4xl mx-auto px-6 py-6">
          <Button
            variant="ghost"
            className="mb-4 gap-2 -ml-2"
            onClick={() => navigate('/domains')}
          >
            <ArrowLeft className="h-4 w-4" />
            Back to Domains
          </Button>
          <div className="flex items-center gap-3">
            <div className="w-12 h-12 rounded-lg bg-primary/10 flex items-center justify-center">
              <Globe className="h-6 w-6 text-primary" />
            </div>
            <div>
              <h1 className="text-2xl font-bold">Add Custom Domain</h1>
              <p className="text-muted-foreground">
                Configure SSL certificate for your domain
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="max-w-4xl mx-auto px-6 py-8">
        {/* Step 1: Input */}
        {step === 'input' && (
          <form onSubmit={handleInitiateChallenge} className="space-y-8">
            {/* Domain Input */}
            <div className="bg-card rounded-lg border border-border p-6">
              <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
                <Globe className="h-5 w-5" />
                Domain Name
              </h2>
              <div className="space-y-2">
                <Label htmlFor="domain">Enter your domain</Label>
                <Input
                  id="domain"
                  type="text"
                  value={domain}
                  onChange={(e) => setDomain(e.target.value)}
                  required
                  placeholder="api.example.com"
                  className="max-w-md"
                />
                <p className="text-sm text-muted-foreground">
                  Enter the full domain name (e.g., api.example.com or *.example.com for wildcard)
                </p>
              </div>
            </div>

            {/* Provisioning Method */}
            <div className="bg-card rounded-lg border border-border p-6">
              <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
                <Shield className="h-5 w-5" />
                Certificate Source
              </h2>

              <Tabs value={provisioningMethod} onValueChange={(v) => setProvisioningMethod(v as ProvisioningMethod)}>
                <TabsList className="grid w-full grid-cols-2 max-w-md">
                  <TabsTrigger value="letsencrypt" className="gap-2">
                    <RefreshCw className="h-4 w-4" />
                    Let's Encrypt (Free)
                  </TabsTrigger>
                  <TabsTrigger value="manual" className="gap-2">
                    <Shield className="h-4 w-4" />
                    Upload Certificate
                  </TabsTrigger>
                </TabsList>

                <TabsContent value="letsencrypt" className="mt-6 space-y-6">
                  {/* Challenge Type Selection */}
                  <div>
                    <Label className="text-base font-medium mb-3 block">Challenge Type</Label>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                      {/* HTTP-01 */}
                      <div
                        className={`cursor-pointer rounded-lg border-2 p-4 transition-all ${
                          challengeType === 'http-01'
                            ? 'border-primary bg-primary/5'
                            : 'border-border hover:border-primary/50'
                        }`}
                        onClick={() => setChallengeType('http-01')}
                      >
                        <div className="flex items-center gap-3 mb-2">
                          <div className={`w-8 h-8 rounded-full flex items-center justify-center ${
                            challengeType === 'http-01' ? 'bg-primary text-primary-foreground' : 'bg-muted'
                          }`}>
                            <Server className="h-4 w-4" />
                          </div>
                          <div className="font-medium">HTTP-01</div>
                        </div>
                        <p className="text-sm text-muted-foreground">
                          We automatically serve the challenge. Just point your domain's DNS A record to this server.
                        </p>
                        <div className="mt-3 px-3 py-2 bg-green-500/10 rounded text-sm text-green-600">
                          Recommended for most users
                        </div>
                      </div>

                      {/* DNS-01 */}
                      <div
                        className={`cursor-pointer rounded-lg border-2 p-4 transition-all ${
                          challengeType === 'dns-01'
                            ? 'border-primary bg-primary/5'
                            : 'border-border hover:border-primary/50'
                        }`}
                        onClick={() => setChallengeType('dns-01')}
                      >
                        <div className="flex items-center gap-3 mb-2">
                          <div className={`w-8 h-8 rounded-full flex items-center justify-center ${
                            challengeType === 'dns-01' ? 'bg-primary text-primary-foreground' : 'bg-muted'
                          }`}>
                            <Network className="h-4 w-4" />
                          </div>
                          <div className="font-medium">DNS-01</div>
                        </div>
                        <p className="text-sm text-muted-foreground">
                          Add a TXT record to your DNS. Required for wildcard certificates (*.domain.com).
                        </p>
                        <div className="mt-3 px-3 py-2 bg-blue-500/10 rounded text-sm text-blue-600">
                          Required for wildcards
                        </div>
                      </div>
                    </div>
                  </div>

                  {/* HTTP-01 Info */}
                  {challengeType === 'http-01' && (
                    <div className="bg-blue-500/10 border border-blue-500/30 rounded-lg p-4">
                      <h4 className="font-medium text-blue-600 mb-2 flex items-center gap-2">
                        <CheckCircle2 className="h-4 w-4" />
                        How HTTP-01 works
                      </h4>
                      <ol className="text-sm text-blue-600/80 space-y-2 list-decimal list-inside">
                        <li>You point your domain's DNS A record to this server</li>
                        <li>We automatically serve the ACME challenge at <code className="bg-blue-500/20 px-1 rounded">/.well-known/acme-challenge/</code></li>
                        <li>Let's Encrypt verifies you control the domain</li>
                        <li>Certificate is issued and ready to use!</li>
                      </ol>
                    </div>
                  )}

                  {/* DNS-01 Info */}
                  {challengeType === 'dns-01' && (
                    <div className="bg-purple-500/10 border border-purple-500/30 rounded-lg p-4">
                      <h4 className="font-medium text-purple-600 mb-2 flex items-center gap-2">
                        <FileText className="h-4 w-4" />
                        How DNS-01 works
                      </h4>
                      <ol className="text-sm text-purple-600/80 space-y-2 list-decimal list-inside">
                        <li>We'll generate a unique TXT record value</li>
                        <li>You add this TXT record to your DNS (at _acme-challenge.yourdomain.com)</li>
                        <li>Let's Encrypt verifies the DNS record</li>
                        <li>Certificate is issued and ready to use!</li>
                      </ol>
                      <p className="text-sm text-purple-600/80 mt-3">
                        <strong>Note:</strong> DNS propagation can take up to 48 hours, but usually completes within minutes.
                      </p>
                    </div>
                  )}
                </TabsContent>

                <TabsContent value="manual" className="mt-6 space-y-4">
                  <div className="bg-amber-500/10 border border-amber-500/30 rounded-lg p-4 mb-4">
                    <h4 className="font-medium text-amber-600 mb-1">Upload your own certificate</h4>
                    <p className="text-sm text-amber-600/80">
                      Paste your certificate and private key in PEM format. Make sure the certificate matches your domain.
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="cert">Certificate (PEM)</Label>
                    <textarea
                      id="cert"
                      value={certPem}
                      onChange={(e) => setCertPem(e.target.value)}
                      required={provisioningMethod === 'manual'}
                      placeholder="-----BEGIN CERTIFICATE-----&#10;...&#10;-----END CERTIFICATE-----"
                      className="w-full h-32 px-3 py-2 bg-background border border-input rounded-md text-sm font-mono"
                    />
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="key">Private Key (PEM)</Label>
                    <textarea
                      id="key"
                      value={keyPem}
                      onChange={(e) => setKeyPem(e.target.value)}
                      required={provisioningMethod === 'manual'}
                      placeholder="-----BEGIN PRIVATE KEY-----&#10;...&#10;-----END PRIVATE KEY-----"
                      className="w-full h-32 px-3 py-2 bg-background border border-input rounded-md text-sm font-mono"
                    />
                  </div>
                </TabsContent>
              </Tabs>
            </div>

            {/* Submit Button */}
            <div className="flex justify-end gap-4">
              <Button type="button" variant="outline" onClick={() => navigate('/domains')}>
                Cancel
              </Button>
              <Button
                type="submit"
                disabled={initiateMutation.isPending || uploadMutation.isPending || !domain}
              >
                {initiateMutation.isPending || uploadMutation.isPending
                  ? 'Processing...'
                  : provisioningMethod === 'letsencrypt'
                    ? 'Start Challenge'
                    : 'Upload Certificate'}
              </Button>
            </div>
          </form>
        )}

        {/* Step 2: Challenge Details */}
        {step === 'challenge' && challengeData && (
          <div className="space-y-8">
            {/* Challenge Info Header */}
            <div className="bg-card rounded-lg border border-border p-6">
              <div className="flex items-center justify-between mb-4">
                <h2 className="text-lg font-semibold flex items-center gap-2">
                  <Shield className="h-5 w-5 text-primary" />
                  Domain Verification Required
                </h2>
                <div className="text-sm text-muted-foreground">
                  Expires: {new Date(challengeData.expires_at).toLocaleString()}
                </div>
              </div>
              <p className="text-muted-foreground">
                Complete the following steps to verify you control <strong className="text-foreground">{challengeData.domain}</strong>
              </p>
            </div>

            {/* HTTP-01 Challenge */}
            {challengeData.challenge.type === 'http01' && (
              <div className="space-y-6">
                {/* Step 1: DNS */}
                <div className="bg-card rounded-lg border border-border p-6">
                  <div className="flex items-center gap-3 mb-4">
                    <div className="w-8 h-8 rounded-full bg-primary text-primary-foreground flex items-center justify-center font-bold">
                      1
                    </div>
                    <h3 className="text-lg font-semibold">Point your DNS to this server</h3>
                  </div>
                  <p className="text-muted-foreground mb-4">
                    Add or update the DNS A record for your domain to point to this server's IP address:
                  </p>
                  <div className="bg-muted rounded-lg p-4 space-y-3">
                    <div className="grid grid-cols-3 gap-4 text-sm">
                      <div>
                        <div className="text-muted-foreground mb-1">Type</div>
                        <div className="font-mono font-medium">A</div>
                      </div>
                      <div>
                        <div className="text-muted-foreground mb-1">Name</div>
                        <div className="font-mono font-medium">{challengeData.domain.split('.')[0]}</div>
                      </div>
                      <div>
                        <div className="text-muted-foreground mb-1">Value</div>
                        <div className="flex items-center gap-2">
                          <span className="font-mono font-medium">{getServerIP()}</span>
                          <Button
                            size="sm"
                            variant="ghost"
                            className="h-6 w-6 p-0"
                            onClick={() => copyToClipboard(getServerIP(), 'IP address')}
                          >
                            <Copy className="h-3 w-3" />
                          </Button>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>

                {/* Step 2: Challenge served automatically */}
                <div className="bg-card rounded-lg border border-border p-6">
                  <div className="flex items-center gap-3 mb-4">
                    <div className="w-8 h-8 rounded-full bg-green-500 text-white flex items-center justify-center font-bold">
                      <CheckCircle2 className="h-5 w-5" />
                    </div>
                    <h3 className="text-lg font-semibold">Challenge is served automatically</h3>
                  </div>
                  <p className="text-muted-foreground mb-4">
                    We're automatically serving the ACME challenge response at:
                  </p>
                  <div className="bg-muted rounded-lg p-4">
                    <div className="flex items-center gap-2 mb-2">
                      <code className="text-sm font-mono flex-1 break-all">
                        http://{challengeData.domain}/.well-known/acme-challenge/{challengeData.challenge.token}
                      </code>
                      <Button
                        size="sm"
                        variant="ghost"
                        className="shrink-0"
                        onClick={() => copyToClipboard(
                          `http://${challengeData.domain}/.well-known/acme-challenge/${(challengeData.challenge as Http01ChallengeInfo).token}`,
                          'URL'
                        )}
                      >
                        <Copy className="h-4 w-4" />
                      </Button>
                      <a
                        href={`http://${challengeData.domain}/.well-known/acme-challenge/${challengeData.challenge.token}`}
                        target="_blank"
                        rel="noopener noreferrer"
                      >
                        <Button size="sm" variant="ghost" className="shrink-0">
                          <ExternalLink className="h-4 w-4" />
                        </Button>
                      </a>
                    </div>
                  </div>

                  <details className="mt-4">
                    <summary className="cursor-pointer text-sm text-muted-foreground hover:text-foreground">
                      View challenge details
                    </summary>
                    <div className="mt-3 space-y-2">
                      <div>
                        <div className="text-sm text-muted-foreground">Token</div>
                        <CodeBlock
                          code={challengeData.challenge.token}
                          language="text"
                        />
                      </div>
                      <div>
                        <div className="text-sm text-muted-foreground">Key Authorization</div>
                        <CodeBlock
                          code={challengeData.challenge.key_authorization}
                          language="text"
                        />
                      </div>
                    </div>
                  </details>
                </div>

                {/* Step 3: Complete */}
                <div className="bg-card rounded-lg border border-border p-6">
                  <div className="flex items-center gap-3 mb-4">
                    <div className="w-8 h-8 rounded-full bg-primary text-primary-foreground flex items-center justify-center font-bold">
                      3
                    </div>
                    <h3 className="text-lg font-semibold">Complete Verification</h3>
                  </div>
                  <p className="text-muted-foreground mb-4">
                    Once your DNS is configured and propagated, click below to verify and get your certificate.
                  </p>
                  <Button
                    onClick={handleCompleteChallenge}
                    disabled={completeMutation.isPending}
                    className="gap-2"
                  >
                    {completeMutation.isPending ? 'Verifying...' : 'Verify & Get Certificate'}
                  </Button>
                </div>
              </div>
            )}

            {/* DNS-01 Challenge */}
            {challengeData.challenge.type === 'dns01' && (
              <div className="space-y-6">
                {/* Step 1: Add TXT Record */}
                <div className="bg-card rounded-lg border border-border p-6">
                  <div className="flex items-center gap-3 mb-6">
                    <div className="w-10 h-10 rounded-full bg-primary text-primary-foreground flex items-center justify-center font-bold text-lg">
                      1
                    </div>
                    <div>
                      <h3 className="text-lg font-semibold">Add DNS TXT Record</h3>
                      <p className="text-sm text-muted-foreground">
                        Add this record to your domain's DNS settings
                      </p>
                    </div>
                  </div>

                  {/* Record Details - Clean Card Style */}
                  <div className="space-y-4">
                    {/* Record Name */}
                    <div className="bg-muted/50 rounded-lg p-4 border border-border">
                      <div className="flex items-center justify-between mb-2">
                        <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                          Record Name (Host)
                        </span>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="h-7 px-2 text-xs gap-1"
                          onClick={() => copyToClipboard((challengeData.challenge as Dns01ChallengeInfo).record_name, 'Record name')}
                        >
                          <Copy className="h-3 w-3" />
                          Copy
                        </Button>
                      </div>
                      <code className="font-mono text-sm break-all">
                        {challengeData.challenge.record_name}
                      </code>
                    </div>

                    {/* Record Value */}
                    <div className="bg-muted/50 rounded-lg p-4 border border-border">
                      <div className="flex items-center justify-between mb-2">
                        <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                          Record Value (Content)
                        </span>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="h-7 px-2 text-xs gap-1"
                          onClick={() => copyToClipboard((challengeData.challenge as Dns01ChallengeInfo).record_value, 'Record value')}
                        >
                          <Copy className="h-3 w-3" />
                          Copy
                        </Button>
                      </div>
                      <code className="font-mono text-sm break-all">
                        {challengeData.challenge.record_value}
                      </code>
                    </div>

                    {/* Type and TTL - Inline */}
                    <div className="flex gap-4">
                      <div className="bg-muted/50 rounded-lg px-4 py-3 border border-border">
                        <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider block mb-1">
                          Type
                        </span>
                        <span className="font-mono font-semibold">TXT</span>
                      </div>
                      <div className="bg-muted/50 rounded-lg px-4 py-3 border border-border">
                        <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider block mb-1">
                          TTL
                        </span>
                        <span className="font-mono">300</span>
                        <span className="text-xs text-muted-foreground ml-1">(or lowest)</span>
                      </div>
                    </div>
                  </div>

                  {/* Help Note */}
                  <div className="mt-6 flex items-start gap-3 text-sm text-muted-foreground">
                    <AlertCircle className="h-4 w-4 mt-0.5 shrink-0" />
                    <p>
                      DNS propagation usually takes a few minutes but can take up to 48 hours.{' '}
                      <a
                        href={`https://dnschecker.org/#TXT/${(challengeData.challenge as Dns01ChallengeInfo).record_name}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-primary hover:underline"
                      >
                        Check propagation status
                      </a>
                    </p>
                  </div>
                </div>

                {/* Step 2: Verify */}
                <div className="bg-card rounded-lg border border-border p-6">
                  <div className="flex items-center gap-3 mb-4">
                    <div className="w-10 h-10 rounded-full bg-muted text-muted-foreground flex items-center justify-center font-bold text-lg">
                      2
                    </div>
                    <div>
                      <h3 className="text-lg font-semibold">Verify & Get Certificate</h3>
                      <p className="text-sm text-muted-foreground">
                        Click below once you've added the DNS record
                      </p>
                    </div>
                  </div>
                  <Button
                    onClick={handleCompleteChallenge}
                    disabled={completeMutation.isPending}
                    size="lg"
                    className="gap-2"
                  >
                    {completeMutation.isPending ? (
                      <>
                        <RefreshCw className="h-4 w-4 animate-spin" />
                        Verifying...
                      </>
                    ) : (
                      <>
                        <CheckCircle2 className="h-4 w-4" />
                        Verify & Get Certificate
                      </>
                    )}
                  </Button>
                </div>
              </div>
            )}

            {/* Back Button */}
            <div className="flex justify-start">
              <Button variant="ghost" onClick={() => setStep('input')} className="gap-2">
                <ArrowLeft className="h-4 w-4" />
                Back to Edit
              </Button>
            </div>
          </div>
        )}

        {/* Step 3: Verifying */}
        {step === 'verifying' && (
          <div className="bg-card rounded-lg border border-border p-12 text-center">
            <div className="w-16 h-16 mx-auto mb-6 rounded-full bg-primary/10 flex items-center justify-center">
              <RefreshCw className="h-8 w-8 text-primary animate-spin" />
            </div>
            <h2 className="text-2xl font-bold mb-2">Verifying Domain Ownership</h2>
            <p className="text-muted-foreground max-w-md mx-auto">
              We're verifying your domain ownership and provisioning your SSL certificate.
              This may take a few moments...
            </p>
          </div>
        )}

        {/* Success */}
        {step === 'success' && (
          <div className="bg-card rounded-lg border border-border p-12 text-center">
            <div className="w-16 h-16 mx-auto mb-6 rounded-full bg-green-500/10 flex items-center justify-center">
              <CheckCircle2 className="h-8 w-8 text-green-500" />
            </div>
            <h2 className="text-2xl font-bold mb-2">Certificate Provisioned!</h2>
            <p className="text-muted-foreground max-w-md mx-auto mb-6">
              Your SSL certificate for <strong className="text-foreground">{domain}</strong> has been
              successfully provisioned and is ready to use with your tunnels.
            </p>
            <Button onClick={() => navigate('/domains')} className="gap-2">
              <CheckCircle2 className="h-4 w-4" />
              Done
            </Button>
          </div>
        )}

        {/* Error */}
        {step === 'error' && (
          <div className="bg-card rounded-lg border border-border p-12 text-center">
            <div className="w-16 h-16 mx-auto mb-6 rounded-full bg-destructive/10 flex items-center justify-center">
              <AlertCircle className="h-8 w-8 text-destructive" />
            </div>
            <h2 className="text-2xl font-bold mb-2 text-destructive">Verification Failed</h2>
            <p className="text-muted-foreground max-w-md mx-auto mb-6">
              {errorMessage || 'An error occurred while verifying your domain.'}
            </p>
            <div className="bg-muted rounded-lg p-4 max-w-md mx-auto text-left mb-6">
              <p className="font-medium mb-2">Troubleshooting tips:</p>
              <ul className="text-sm text-muted-foreground space-y-1 list-disc list-inside">
                <li>Verify your DNS records are correctly configured</li>
                <li>Wait for DNS propagation (can take up to 48 hours)</li>
                <li>Ensure port 80 is accessible for HTTP-01 challenges</li>
                <li>Check that your domain is not behind a CDN or proxy</li>
              </ul>
            </div>
            <div className="flex justify-center gap-4">
              <Button variant="outline" onClick={() => setStep(challengeData ? 'challenge' : 'input')}>
                Try Again
              </Button>
              <Button onClick={() => navigate('/domains')}>
                Back to Domains
              </Button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
