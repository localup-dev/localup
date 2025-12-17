import { useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import {
  ArrowLeft,
  Globe,
  Shield,
  Clock,
  AlertCircle,
  CheckCircle2,
  RefreshCw,
  XCircle,
  Copy,
  ExternalLink,
  FileText,
  Key,
  Fingerprint,
  Search,
} from 'lucide-react';
import {
  getDomainByIdOptions,
  getDomainChallengesOptions,
  cancelChallengeMutation,
  restartChallengeMutation,
  completeChallengeMutation,
} from '../api/client/@tanstack/react-query.gen';
import type { CustomDomainStatus, ChallengeInfo } from '../api/client/types.gen';

// Certificate details type (matches backend CertificateDetails model)
interface CertificateDetails {
  domain: string;
  subject: string;
  issuer: string;
  serial_number: string;
  not_before: string;
  not_after: string;
  san: string[];
  signature_algorithm: string;
  public_key_algorithm: string;
  fingerprint_sha256: string;
  pem: string;
}

// Pre-validation response type (matches backend PreValidateChallengeResponse model)
interface PreValidationResult {
  ready: boolean;
  challenge_type: string;
  checked: string;
  expected: string;
  found?: string;
  error?: string;
  details?: string;
}
import { Button } from '../components/ui/button';
import { Badge } from '../components/ui/badge';
import { Skeleton } from '../components/ui/skeleton';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../components/ui/card';

export default function DomainDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [showCertPem, setShowCertPem] = useState(false);
  const [preValidationResult, setPreValidationResult] = useState<PreValidationResult | null>(null);
  const [isPreValidating, setIsPreValidating] = useState(false);

  // Fetch domain by ID
  const { data: domain, isLoading, error } = useQuery(
    getDomainByIdOptions({ path: { id: id || '' } })
  );

  // Fetch pending challenges for this domain
  const { data: challenges } = useQuery({
    ...getDomainChallengesOptions({ path: { domain: domain?.domain || '' } }),
    enabled: !!domain?.domain,
  });

  // Fetch certificate details when domain is active
  const { data: certDetails, isLoading: certLoading } = useQuery({
    queryKey: ['certificateDetails', domain?.domain],
    queryFn: async () => {
      if (!domain?.domain) return null;
      const response = await fetch(`/api/domains/${encodeURIComponent(domain.domain)}/certificate-details`, {
        credentials: 'include',
      });
      if (!response.ok) {
        throw new Error('Failed to fetch certificate details');
      }
      return response.json() as Promise<CertificateDetails>;
    },
    enabled: !!domain?.domain && domain?.status === 'active',
  });

  // Cancel challenge mutation
  const cancelMutation = useMutation({
    ...cancelChallengeMutation(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: getDomainByIdOptions({ path: { id: id || '' } }).queryKey });
      queryClient.invalidateQueries({ queryKey: getDomainChallengesOptions({ path: { domain: domain?.domain || '' } }).queryKey });
      toast.success('Challenge cancelled');
    },
    onError: (err) => {
      toast.error(err.message || 'Failed to cancel challenge');
    },
  });

  // Restart challenge mutation
  const restartMutation = useMutation({
    ...restartChallengeMutation(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: getDomainByIdOptions({ path: { id: id || '' } }).queryKey });
      queryClient.invalidateQueries({ queryKey: getDomainChallengesOptions({ path: { domain: domain?.domain || '' } }).queryKey });
      toast.success('Challenge restarted');
    },
    onError: (err) => {
      toast.error(err.message || 'Failed to restart challenge');
    },
  });

  // Complete challenge mutation
  const completeMutation = useMutation({
    ...completeChallengeMutation(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: getDomainByIdOptions({ path: { id: id || '' } }).queryKey });
      queryClient.invalidateQueries({ queryKey: getDomainChallengesOptions({ path: { domain: domain?.domain || '' } }).queryKey });
      toast.success('Challenge verification started');
    },
    onError: (err) => {
      toast.error(err.message || 'Failed to verify challenge');
    },
  });

  const handleCancelChallenge = () => {
    if (domain?.domain) {
      cancelMutation.mutate({ path: { domain: domain.domain } });
    }
  };

  const handleRestartChallenge = (challengeType: string) => {
    if (domain?.domain) {
      restartMutation.mutate({
        path: { domain: domain.domain },
        body: { domain: domain.domain, challenge_type: challengeType },
      });
    }
  };

  const handleVerifyChallenge = (challengeId: string) => {
    if (domain?.domain) {
      completeMutation.mutate({
        body: { domain: domain.domain, challenge_id: challengeId },
      });
    }
  };

  const handlePreValidate = async (challengeId: string) => {
    if (!domain?.domain) return;

    setIsPreValidating(true);
    setPreValidationResult(null);

    try {
      const response = await fetch('/api/domains/challenge/pre-validate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'include',
        body: JSON.stringify({
          domain: domain.domain,
          challenge_id: challengeId,
        }),
      });

      if (!response.ok) {
        const errorData = await response.json();
        toast.error(errorData.error || 'Pre-validation failed');
        return;
      }

      const result: PreValidationResult = await response.json();
      setPreValidationResult(result);

      if (result.ready) {
        toast.success('Challenge is ready! You can now verify with Let\'s Encrypt.');
      } else {
        toast.error(result.error || 'Challenge is not ready yet');
      }
    } catch (err) {
      toast.error('Failed to pre-validate challenge');
    } finally {
      setIsPreValidating(false);
    }
  };

  const copyToClipboard = (text: string, label: string) => {
    navigator.clipboard.writeText(text);
    toast.success(`${label} copied to clipboard`);
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

  const renderChallengeDetails = (challenge: ChallengeInfo) => {
    if (challenge.type === 'http01') {
      return (
        <div className="space-y-4">
          <div>
            <p className="text-sm text-muted-foreground mb-1">Challenge URL</p>
            <div className="flex items-center gap-2">
              <code className="flex-1 bg-muted px-3 py-2 rounded text-sm break-all">
                {challenge.file_path}
              </code>
              <Button
                variant="outline"
                size="icon"
                onClick={() => copyToClipboard(challenge.file_path, 'URL')}
              >
                <Copy className="h-4 w-4" />
              </Button>
              <Button
                variant="outline"
                size="icon"
                asChild
              >
                <a href={challenge.file_path} target="_blank" rel="noopener noreferrer">
                  <ExternalLink className="h-4 w-4" />
                </a>
              </Button>
            </div>
          </div>
          <div>
            <p className="text-sm text-muted-foreground mb-1">Key Authorization</p>
            <div className="flex items-center gap-2">
              <code className="flex-1 bg-muted px-3 py-2 rounded text-sm break-all">
                {challenge.key_authorization}
              </code>
              <Button
                variant="outline"
                size="icon"
                onClick={() => copyToClipboard(challenge.key_authorization, 'Key authorization')}
              >
                <Copy className="h-4 w-4" />
              </Button>
            </div>
          </div>
          <div className="bg-blue-50 dark:bg-blue-950 border border-blue-200 dark:border-blue-800 rounded-lg p-4">
            <p className="text-sm text-blue-800 dark:text-blue-200">
              <strong>Instructions:</strong> The relay server automatically serves the HTTP-01 challenge response.
              Make sure your domain's DNS points to this relay server, then click "Verify Challenge" below.
            </p>
          </div>
        </div>
      );
    }

    if (challenge.type === 'dns01') {
      return (
        <div className="space-y-4">
          <div>
            <p className="text-sm text-muted-foreground mb-1">DNS Record Name</p>
            <div className="flex items-center gap-2">
              <code className="flex-1 bg-muted px-3 py-2 rounded text-sm">
                {challenge.record_name}
              </code>
              <Button
                variant="outline"
                size="icon"
                onClick={() => copyToClipboard(challenge.record_name, 'Record name')}
              >
                <Copy className="h-4 w-4" />
              </Button>
            </div>
          </div>
          <div>
            <p className="text-sm text-muted-foreground mb-1">TXT Record Value</p>
            <div className="flex items-center gap-2">
              <code className="flex-1 bg-muted px-3 py-2 rounded text-sm break-all">
                {challenge.record_value}
              </code>
              <Button
                variant="outline"
                size="icon"
                onClick={() => copyToClipboard(challenge.record_value, 'Record value')}
              >
                <Copy className="h-4 w-4" />
              </Button>
            </div>
          </div>
          <div className="bg-amber-50 dark:bg-amber-950 border border-amber-200 dark:border-amber-800 rounded-lg p-4">
            <p className="text-sm text-amber-800 dark:text-amber-200">
              <strong>Instructions:</strong>
            </p>
            <ol className="text-sm text-amber-800 dark:text-amber-200 mt-2 space-y-1 list-decimal list-inside">
              <li>Go to your DNS provider's control panel</li>
              <li>Add a new TXT record with the name and value above</li>
              <li>Wait for DNS propagation (usually 1-10 minutes)</li>
              <li>Click "Verify Challenge" below once the record is live</li>
            </ol>
          </div>
        </div>
      );
    }

    return null;
  };

  if (isLoading) {
    return (
      <div className="min-h-screen bg-background text-foreground">
        <div className="border-b border-border">
          <div className="max-w-4xl mx-auto px-6 py-6">
            <Skeleton className="h-8 w-64" />
          </div>
        </div>
        <div className="max-w-4xl mx-auto px-6 py-8 space-y-6">
          <Skeleton className="h-48 w-full" />
          <Skeleton className="h-32 w-full" />
        </div>
      </div>
    );
  }

  if (error || !domain) {
    return (
      <div className="min-h-screen bg-background text-foreground">
        <div className="border-b border-border">
          <div className="max-w-4xl mx-auto px-6 py-6">
            <Button variant="ghost" onClick={() => navigate('/domains')} className="gap-2 mb-4">
              <ArrowLeft className="h-4 w-4" />
              Back to Domains
            </Button>
          </div>
        </div>
        <div className="max-w-4xl mx-auto px-6 py-8">
          <div className="bg-destructive/10 border border-destructive/50 text-destructive px-4 py-3 rounded-lg">
            {error?.message || 'Domain not found'}
          </div>
        </div>
      </div>
    );
  }

  const pendingChallenges = challenges || [];

  return (
    <div className="min-h-screen bg-background text-foreground">
      {/* Header */}
      <div className="border-b border-border">
        <div className="max-w-4xl mx-auto px-6 py-6">
          <Button variant="ghost" onClick={() => navigate('/domains')} className="gap-2 mb-4">
            <ArrowLeft className="h-4 w-4" />
            Back to Domains
          </Button>
          <div className="flex items-center gap-4">
            <div className="w-12 h-12 rounded-lg bg-primary/10 flex items-center justify-center">
              <Globe className="h-6 w-6 text-primary" />
            </div>
            <div className="flex-1">
              <div className="flex items-center gap-3">
                <h1 className="text-2xl font-bold">{domain.domain}</h1>
                {getStatusBadge(domain.status)}
              </div>
              <p className="text-muted-foreground text-sm mt-1">
                ID: {domain.id}
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="max-w-4xl mx-auto px-6 py-8 space-y-6">
        {/* Domain Info Card */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Shield className="h-5 w-5" />
              Certificate Information
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 gap-4">
              <div>
                <p className="text-sm text-muted-foreground">Status</p>
                <p className="font-medium">{getStatusBadge(domain.status)}</p>
              </div>
              <div>
                <p className="text-sm text-muted-foreground">Auto-Renew</p>
                <Badge variant={domain.auto_renew ? 'success' : 'secondary'}>
                  {domain.auto_renew ? 'Enabled' : 'Disabled'}
                </Badge>
              </div>
              <div>
                <p className="text-sm text-muted-foreground">Provisioned</p>
                <p className="font-medium">
                  {new Date(domain.provisioned_at).toLocaleString()}
                </p>
              </div>
              <div>
                <p className="text-sm text-muted-foreground">Expires</p>
                <p className="font-medium">
                  {domain.expires_at
                    ? new Date(domain.expires_at).toLocaleString()
                    : 'Not set'}
                </p>
              </div>
              {domain.error_message && (
                <div className="col-span-2">
                  <p className="text-sm text-muted-foreground">Error</p>
                  <p className="text-destructive font-medium">{domain.error_message}</p>
                </div>
              )}
            </div>
          </CardContent>
        </Card>

        {/* Pending Challenges */}
        {domain.status === 'pending' && pendingChallenges.length > 0 && (
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Clock className="h-5 w-5" />
                Pending Challenge
              </CardTitle>
              <CardDescription>
                Complete the challenge below to verify domain ownership
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              {pendingChallenges.map((challengeResponse) => (
                <div key={challengeResponse.challenge_id} className="space-y-4">
                  <div className="flex items-center justify-between">
                    <Badge variant="outline">
                      {challengeResponse.challenge.type === 'http01' ? 'HTTP-01' : 'DNS-01'} Challenge
                    </Badge>
                    <span className="text-sm text-muted-foreground">
                      Expires: {new Date(challengeResponse.expires_at).toLocaleString()}
                    </span>
                  </div>

                  {renderChallengeDetails(challengeResponse.challenge)}

                  {/* Pre-validation Result */}
                  {preValidationResult && (
                    <div className={`rounded-lg p-4 ${preValidationResult.ready
                      ? 'bg-green-50 dark:bg-green-950 border border-green-200 dark:border-green-800'
                      : 'bg-red-50 dark:bg-red-950 border border-red-200 dark:border-red-800'
                    }`}>
                      <div className="flex items-start gap-3">
                        {preValidationResult.ready ? (
                          <CheckCircle2 className="h-5 w-5 text-green-600 dark:text-green-400 mt-0.5" />
                        ) : (
                          <XCircle className="h-5 w-5 text-red-600 dark:text-red-400 mt-0.5" />
                        )}
                        <div className="flex-1 space-y-2">
                          <p className={`font-medium ${preValidationResult.ready
                            ? 'text-green-800 dark:text-green-200'
                            : 'text-red-800 dark:text-red-200'
                          }`}>
                            {preValidationResult.ready
                              ? '✅ Challenge is ready to verify!'
                              : '❌ Challenge not ready'
                            }
                          </p>
                          <div className="text-sm space-y-1">
                            <p><span className="text-muted-foreground">Checked:</span> {preValidationResult.checked}</p>
                            <p><span className="text-muted-foreground">Expected:</span> <code className="bg-muted px-1 rounded text-xs">{preValidationResult.expected}</code></p>
                            {preValidationResult.found && (
                              <p><span className="text-muted-foreground">Found:</span> <code className="bg-muted px-1 rounded text-xs">{preValidationResult.found}</code></p>
                            )}
                            {preValidationResult.error && !preValidationResult.ready && (
                              <p className="text-red-700 dark:text-red-300">{preValidationResult.error}</p>
                            )}
                          </div>
                          {preValidationResult.details && (
                            <p className="text-sm whitespace-pre-wrap mt-2 opacity-80">{preValidationResult.details}</p>
                          )}
                        </div>
                      </div>
                    </div>
                  )}

                  <div className="flex flex-wrap gap-2 pt-4 border-t">
                    <Button
                      variant="outline"
                      onClick={() => handlePreValidate(challengeResponse.challenge_id)}
                      disabled={isPreValidating}
                      className="gap-2"
                    >
                      {isPreValidating ? (
                        <>
                          <RefreshCw className="h-4 w-4 animate-spin" />
                          Checking...
                        </>
                      ) : (
                        <>
                          <Search className="h-4 w-4" />
                          Verify Setup
                        </>
                      )}
                    </Button>
                    <Button
                      onClick={() => handleVerifyChallenge(challengeResponse.challenge_id)}
                      disabled={completeMutation.isPending}
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
                          Verify Challenge
                        </>
                      )}
                    </Button>
                    <Button
                      variant="outline"
                      onClick={() => handleRestartChallenge(challengeResponse.challenge.type === 'http01' ? 'http-01' : 'dns-01')}
                      disabled={restartMutation.isPending}
                      className="gap-2"
                    >
                      <RefreshCw className="h-4 w-4" />
                      Restart
                    </Button>
                    <Button
                      variant="destructive"
                      onClick={handleCancelChallenge}
                      disabled={cancelMutation.isPending}
                      className="gap-2"
                    >
                      <XCircle className="h-4 w-4" />
                      Cancel
                    </Button>
                  </div>
                </div>
              ))}
            </CardContent>
          </Card>
        )}

        {/* Failed/Expired - Show restart option */}
        {(domain.status === 'failed' || domain.status === 'expired') && (
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-destructive">
                <AlertCircle className="h-5 w-5" />
                {domain.status === 'failed' ? 'Challenge Failed' : 'Certificate Expired'}
              </CardTitle>
              <CardDescription>
                {domain.status === 'failed'
                  ? 'The certificate challenge failed. You can restart the process below.'
                  : 'Your certificate has expired. Restart the challenge to get a new certificate.'}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="flex gap-2">
                <Button
                  onClick={() => handleRestartChallenge('http-01')}
                  disabled={restartMutation.isPending}
                  className="gap-2"
                >
                  <RefreshCw className="h-4 w-4" />
                  Restart with HTTP-01
                </Button>
                <Button
                  variant="outline"
                  onClick={() => handleRestartChallenge('dns-01')}
                  disabled={restartMutation.isPending}
                  className="gap-2"
                >
                  <RefreshCw className="h-4 w-4" />
                  Restart with DNS-01
                </Button>
              </div>
            </CardContent>
          </Card>
        )}

        {/* Active Certificate */}
        {domain.status === 'active' && (
          <Card className="border-green-200 dark:border-green-800">
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-green-700 dark:text-green-400">
                <CheckCircle2 className="h-5 w-5" />
                Certificate Active
              </CardTitle>
              <CardDescription>
                Your SSL certificate is active and will be used for HTTPS connections.
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              {certLoading ? (
                <div className="space-y-2">
                  <Skeleton className="h-4 w-full" />
                  <Skeleton className="h-4 w-3/4" />
                  <Skeleton className="h-4 w-1/2" />
                </div>
              ) : certDetails ? (
                <>
                  {/* Certificate Info Grid */}
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <div className="space-y-1">
                      <p className="text-sm text-muted-foreground flex items-center gap-1">
                        <FileText className="h-3 w-3" />
                        Subject
                      </p>
                      <p className="font-mono text-sm break-all">{certDetails.subject}</p>
                    </div>
                    <div className="space-y-1">
                      <p className="text-sm text-muted-foreground flex items-center gap-1">
                        <Shield className="h-3 w-3" />
                        Issuer
                      </p>
                      <p className="font-mono text-sm break-all">{certDetails.issuer}</p>
                    </div>
                    <div className="space-y-1">
                      <p className="text-sm text-muted-foreground">Serial Number</p>
                      <div className="flex items-center gap-2">
                        <code className="text-xs bg-muted px-2 py-1 rounded break-all">
                          {certDetails.serial_number}
                        </code>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6"
                          onClick={() => copyToClipboard(certDetails.serial_number, 'Serial number')}
                        >
                          <Copy className="h-3 w-3" />
                        </Button>
                      </div>
                    </div>
                    <div className="space-y-1">
                      <p className="text-sm text-muted-foreground flex items-center gap-1">
                        <Fingerprint className="h-3 w-3" />
                        Fingerprint (SHA-256)
                      </p>
                      <div className="flex items-center gap-2">
                        <code className="text-xs bg-muted px-2 py-1 rounded break-all max-w-[300px] truncate">
                          {certDetails.fingerprint_sha256}
                        </code>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-6 w-6"
                          onClick={() => copyToClipboard(certDetails.fingerprint_sha256, 'Fingerprint')}
                        >
                          <Copy className="h-3 w-3" />
                        </Button>
                      </div>
                    </div>
                    <div className="space-y-1">
                      <p className="text-sm text-muted-foreground">Valid From</p>
                      <p className="font-medium">{new Date(certDetails.not_before).toLocaleString()}</p>
                    </div>
                    <div className="space-y-1">
                      <p className="text-sm text-muted-foreground">Valid Until</p>
                      <p className="font-medium">{new Date(certDetails.not_after).toLocaleString()}</p>
                    </div>
                    <div className="space-y-1">
                      <p className="text-sm text-muted-foreground flex items-center gap-1">
                        <Key className="h-3 w-3" />
                        Signature Algorithm
                      </p>
                      <p className="font-mono text-sm">{certDetails.signature_algorithm}</p>
                    </div>
                    <div className="space-y-1">
                      <p className="text-sm text-muted-foreground">Public Key Algorithm</p>
                      <p className="font-mono text-sm">{certDetails.public_key_algorithm}</p>
                    </div>
                  </div>

                  {/* Subject Alternative Names */}
                  {certDetails.san && certDetails.san.length > 0 && (
                    <div className="space-y-2">
                      <p className="text-sm text-muted-foreground">Subject Alternative Names (SANs)</p>
                      <div className="flex flex-wrap gap-2">
                        {certDetails.san.map((name, i) => (
                          <Badge key={i} variant="outline">{name}</Badge>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* PEM Certificate */}
                  <div className="space-y-2 pt-4 border-t">
                    <div className="flex items-center justify-between">
                      <p className="text-sm text-muted-foreground">Certificate (PEM)</p>
                      <div className="flex gap-2">
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => setShowCertPem(!showCertPem)}
                        >
                          {showCertPem ? 'Hide' : 'Show'}
                        </Button>
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => copyToClipboard(certDetails.pem, 'Certificate PEM')}
                          className="gap-1"
                        >
                          <Copy className="h-3 w-3" />
                          Copy
                        </Button>
                      </div>
                    </div>
                    {showCertPem && (
                      <pre className="bg-muted p-4 rounded-lg text-xs overflow-x-auto max-h-64 overflow-y-auto font-mono">
                        {certDetails.pem}
                      </pre>
                    )}
                  </div>
                </>
              ) : (
                <p className="text-muted-foreground">Failed to load certificate details</p>
              )}
            </CardContent>
          </Card>
        )}
      </div>
    </div>
  );
}
