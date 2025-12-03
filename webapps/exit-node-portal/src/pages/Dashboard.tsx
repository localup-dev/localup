import { useQuery } from '@tanstack/react-query';
import { Apple, Monitor, Container, ExternalLink, Key, Cable, BookOpen } from 'lucide-react';
import { getCurrentUserOptions, listAuthTokensOptions } from '../api/client/@tanstack/react-query.gen';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../components/ui/tabs';
import { CodeBlock } from '../components/CodeBlock';
import { Link } from 'react-router-dom';

const platforms = [
  { id: 'macos', name: 'macOS', icon: Apple },
  { id: 'windows', name: 'Windows', icon: Monitor },
  { id: 'linux', name: 'Linux', icon: Monitor },
  { id: 'docker', name: 'Docker', icon: Container },
];

const installCommands: Record<string, { method: string; steps: Array<{ title: string; command: string; description: string }> }> = {
  macos: {
    method: 'Homebrew',
    steps: [
      {
        title: 'Install via Homebrew',
        command: 'brew tap localup-dev/tap && brew install localup',
        description: 'Install LocalUp via Homebrew',
      },
    ],
  },
  windows: {
    method: 'Download',
    steps: [
      {
        title: 'Download the installer',
        command: 'https://github.com/localup-dev/localup/releases/latest',
        description: 'Download the Windows installer from GitHub releases',
      },
    ],
  },
  linux: {
    method: 'Homebrew',
    steps: [
      {
        title: 'Install via Homebrew',
        command: 'brew tap localup-dev/tap && brew install localup',
        description: 'Install LocalUp via Homebrew (works on Linux too)',
      },
    ],
  },
  docker: {
    method: 'Docker',
    steps: [
      {
        title: 'Run with Docker',
        command: 'docker run -it localup/localup:latest --port 3000 --relay tunnel.kfs.es:4443 --token $TOKEN',
        description: 'Run LocalUp in a Docker container',
      },
    ],
  },
};

export default function Dashboard() {
  const { data: user } = useQuery(getCurrentUserOptions());
  const { data: tokensData } = useQuery(listAuthTokensOptions());

  const hasDefaultToken = tokensData?.tokens?.some((t) => t.name === 'Default') || false;

  return (
    <div className="min-h-screen bg-background text-foreground">
      {/* Header */}
      <div className="border-b border-border">
        <div className="max-w-7xl mx-auto px-6 py-6">
          <h1 className="text-3xl font-bold">Welcome{user?.username ? `, ${user.username}` : ''}!</h1>
          <p className="text-muted-foreground mt-2">
            LocalUp is your app's front door—a globally distributed reverse proxy that secures,
            protects and accelerates your applications and network services, no matter where you run them.
          </p>
        </div>
      </div>

      {/* Main Content */}
      <div className="max-w-7xl mx-auto px-6 py-8">
        <div className="bg-card rounded-lg border border-border p-8">
          <div className="flex items-center gap-3 mb-6">
            <div className="w-8 h-8 rounded-full bg-primary flex items-center justify-center text-primary-foreground font-bold text-sm">
              1
            </div>
            <h2 className="text-2xl font-bold">Get an endpoint online</h2>
          </div>

          {/* Platform Selector Tabs */}
          <Tabs defaultValue="macos" className="w-full">
            <div className="flex items-center gap-4 mb-6">
              <span className="text-sm text-muted-foreground">Agent</span>
              <TabsList>
                {platforms.map((platform) => {
                  const Icon = platform.icon;
                  return (
                    <TabsTrigger key={platform.id} value={platform.id} className="gap-2">
                      <Icon className="h-4 w-4" />
                      {platform.name}
                    </TabsTrigger>
                  );
                })}
              </TabsList>
            </div>

            {platforms.map((platform) => (
              <TabsContent key={platform.id} value={platform.id} className="space-y-8">
                {/* Installation */}
                <div>
                  <h3 className="text-lg font-semibold mb-4">Installation</h3>
                  <div className="space-y-4">
                    {installCommands[platform.id].steps.map((step, index) => (
                      <div key={index}>
                        <p className="text-muted-foreground mb-2">{step.description}</p>
                        <CodeBlock code={step.command} />
                      </div>
                    ))}
                  </div>
                </div>

                {/* Setup authtoken */}
                <div>
                  <h3 className="text-lg font-semibold mb-4">Setup your authtoken</h3>
                  <p className="text-muted-foreground mb-4">
                    Run the following command to set your JWT token in the configuration.
                  </p>

                  <CodeBlock code="localup config set-token <YOUR_JWT_TOKEN>" />

                  <p className="text-muted-foreground text-sm mt-3 mb-2">
                    Or export it as an environment variable:
                  </p>
                  <CodeBlock code="export TOKEN=<YOUR_JWT_TOKEN>" />

                  <div className="mt-4 p-4 bg-primary/10 border border-primary/30 rounded-lg">
                    <div className="flex items-start gap-3">
                      <Key className="h-5 w-5 text-primary flex-shrink-0 mt-0.5" />
                      <div>
                        <p className="text-primary font-medium text-sm">
                          Your auth token was automatically created when you {hasDefaultToken ? 'logged in' : 'registered'}.
                        </p>
                        <p className="text-muted-foreground text-sm mt-1">
                          Go to the{' '}
                          <Link to="/tokens" className="text-primary hover:underline">
                            Auth Tokens
                          </Link>{' '}
                          page to view your tokens and create new ones if needed.
                        </p>
                      </div>
                    </div>
                  </div>
                </div>

                {/* Connect Command */}
                <div>
                  <h3 className="text-lg font-semibold mb-4">Deploy your app online</h3>
                  <p className="text-muted-foreground mb-2">
                    For HTTP tunnels, run the following to expose a local web server:
                  </p>
                  <CodeBlock code='localup --relay=tunnel.kfs.es:4443 --port=3000 --token=$TOKEN --subdomain="myapp"' />

                  <p className="text-muted-foreground text-sm mt-4 mb-2">
                    For TCP tunnels (e.g., SSH, databases):
                  </p>
                  <CodeBlock code='localup --port=22 --relay=tunnel.kfs.es:5443 --protocol=tcp --token=$TOKEN' />

                  <p className="text-muted-foreground text-sm mt-4">
                    Go to your dev domain to see your app!
                  </p>
                  <a
                    href="http://myapp.tunnel.kfs.es"
                    className="inline-flex items-center gap-2 text-primary hover:underline text-sm font-mono mt-1"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    http://myapp.tunnel.kfs.es
                    <ExternalLink className="h-3 w-3" />
                  </a>
                </div>
              </TabsContent>
            ))}
          </Tabs>

          {/* Next Steps */}
          <div className="mt-8 pt-6 border-t border-border">
            <h3 className="text-lg font-semibold mb-4">Next Steps</h3>
            <ul className="space-y-3">
              <li className="flex items-start gap-3 text-muted-foreground">
                <Key className="h-5 w-5 text-muted-foreground flex-shrink-0 mt-0.5" />
                <span>
                  Visit the{' '}
                  <Link to="/tokens" className="text-primary hover:underline">
                    Auth Tokens
                  </Link>{' '}
                  page to manage your API tokens
                </span>
              </li>
              <li className="flex items-start gap-3 text-muted-foreground">
                <Cable className="h-5 w-5 text-muted-foreground flex-shrink-0 mt-0.5" />
                <span>
                  View your{' '}
                  <Link to="/tunnels" className="text-primary hover:underline">
                    Active Tunnels
                  </Link>{' '}
                  and monitor traffic
                </span>
              </li>
              <li className="flex items-start gap-3 text-muted-foreground">
                <BookOpen className="h-5 w-5 text-muted-foreground flex-shrink-0 mt-0.5" />
                <span>
                  Check out the{' '}
                  <a
                    href="https://github.com/localup-dev/localup"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-primary hover:underline inline-flex items-center gap-1"
                  >
                    documentation
                    <ExternalLink className="h-3 w-3" />
                  </a>{' '}
                  for advanced features
                </span>
              </li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}
