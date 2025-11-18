import { useState, useEffect } from 'react';
import { getCurrentUser, type User } from '../utils/auth';
import { getAuthTokens } from '../utils/api';
import { Button } from '../components/ui/button';

const platforms = [
  { id: 'macos', name: 'macOS', icon: '🍎' },
  { id: 'windows', name: 'Windows', icon: '🪟' },
  { id: 'linux', name: 'Linux', icon: '🐧' },
  { id: 'docker', name: 'Docker', icon: '🐳' },
];

export default function Dashboard() {
  const [selectedPlatform, setSelectedPlatform] = useState('macos');
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);
  const [hasDefaultToken, setHasDefaultToken] = useState(false);
  const [user, setUser] = useState<User | null>(null);

  useEffect(() => {
    async function init() {
      // Get current user
      const currentUser = await getCurrentUser();
      setUser(currentUser);

      // Fetch auth tokens
      try {
        const response = await getAuthTokens();
        const defaultToken = response.tokens.find((t: any) => t.name === 'Default');
        setHasDefaultToken(!!defaultToken);
      } catch (error) {
        console.error('Failed to fetch auth tokens:', error);
      }
    }
    init();
  }, []);

  const copyToClipboard = (text: string, index: number) => {
    navigator.clipboard.writeText(text);
    setCopiedIndex(index);
    setTimeout(() => setCopiedIndex(null), 2000);
  };

  const installCommands = {
    macos: {
      method: 'Homebrew',
      steps: [
        {
          title: 'Install via Homebrew',
          command: 'brew tap localup/tap && brew install localup',
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
      method: 'Download Binary',
      steps: [
        {
          title: 'Download and install',
          command: 'curl -fsSL https://get.localup.dev | sh',
          description: 'Install LocalUp on Linux',
        },
      ],
    },
    docker: {
      method: 'Docker',
      steps: [
        {
          title: 'Run with Docker',
          command: 'docker run -it localup/localup:latest --port 3000 --relay relay.localup.dev:4443',
          description: 'Run LocalUp in a Docker container',
        },
      ],
    },
  };

  const currentPlatform = installCommands[selectedPlatform as keyof typeof installCommands];

  return (
    <div className="min-h-screen bg-gray-900 text-white">
      {/* Header */}
      <div className="border-b border-gray-800">
        <div className="max-w-7xl mx-auto px-6 py-6">
          <h1 className="text-3xl font-bold">Welcome{user?.username ? `, ${user.username}` : ''}!</h1>
          <p className="text-gray-400 mt-2">
            LocalUp is your app's front door—a globally distributed reverse proxy that secures,
            protects and accelerates your applications and network services, no matter where you run them.
          </p>
        </div>
      </div>

      {/* Main Content */}
      <div className="max-w-7xl mx-auto px-6 py-8">
        <div className="bg-gray-800 rounded-lg p-8">
          <div className="flex items-center justify-between mb-6">
            <div>
              <div className="flex items-center gap-3 mb-2">
                <div className="w-8 h-8 rounded-full bg-blue-600 flex items-center justify-center text-white font-bold">
                  1
                </div>
                <h2 className="text-2xl font-bold">Get an endpoint online</h2>
              </div>
            </div>
          </div>

          {/* Platform Selector */}
          <div className="mb-8">
            <div className="flex items-center gap-2 mb-4">
              <span className="text-sm text-gray-400">Agent</span>
              <div className="flex gap-2">
                {platforms.map((platform) => (
                  <Button
                    key={platform.id}
                    onClick={() => setSelectedPlatform(platform.id)}
                    variant={selectedPlatform === platform.id ? 'default' : 'outline'}
                  >
                    <span className="mr-2">{platform.icon}</span>
                    {platform.name}
                  </Button>
                ))}
              </div>
            </div>
          </div>

          {/* Installation Steps */}
          <div className="space-y-6">
            {/* Installation */}
            <div>
              <h3 className="text-lg font-semibold mb-4">Installation</h3>
              <div className="space-y-4">
                {currentPlatform.steps.map((step, index) => (
                  <div key={index}>
                    <p className="text-gray-400 mb-2">{step.description}</p>
                    <div className="bg-gray-900 rounded-md p-4 flex items-center justify-between">
                      <code className="text-blue-400 font-mono text-sm">{step.command}</code>
                      <Button
                        onClick={() => copyToClipboard(step.command, index)}
                        variant="secondary"
                        size="sm"
                        className="ml-4 flex-shrink-0"
                      >
                        {copiedIndex === index ? '✓ Copied' : '📋 Copy'}
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* Setup authtoken */}
            <div>
              <h3 className="text-lg font-semibold mb-4">Setup your authtoken</h3>
              <p className="text-gray-400 mb-4">
                Run the following command to add your authtoken to the default configuration file.
              </p>

              <div>
                <div className="bg-gray-900 rounded-md p-4 mb-4">
                  <code className="text-blue-400 font-mono text-sm">
                    localup config add-authtoken {'<YOUR_AUTH_TOKEN>'}
                  </code>
                </div>
                <div className="p-4 bg-blue-900/20 border border-blue-600/30 rounded-md">
                  <p className="text-blue-300 text-sm">
                    💡 <strong>Your auth token was automatically created when you {hasDefaultToken ? 'logged in' : 'registered'}.</strong>
                  </p>
                  <p className="text-gray-400 text-sm mt-2">
                    Go to the{' '}
                    <a href="/tokens" className="text-blue-400 hover:text-blue-300 underline">
                      Auth Tokens
                    </a>{' '}
                    page to view your tokens and create new ones if needed.
                  </p>
                </div>
              </div>
            </div>

            {/* Connect Command */}
            <div>
              <h3 className="text-lg font-semibold mb-4">Deploy your app online</h3>
              <p className="text-gray-400 mb-2">
                Run the following in the command line to expose a local web server:
              </p>
              <div className="bg-gray-900 rounded-md p-4 flex items-center justify-between">
                <code className="text-blue-400 font-mono text-sm">
                  localup http 80
                </code>
                <Button
                  onClick={() => copyToClipboard('localup http 80', 100)}
                  variant="secondary"
                  size="sm"
                  className="ml-4 flex-shrink-0"
                >
                  {copiedIndex === 100 ? '✓ Copied' : '📋 Copy'}
                </Button>
              </div>
              <p className="text-gray-400 text-sm mt-4">
                Go to your dev domain to see your app!
              </p>
              <a
                href="http://localhost:18080"
                className="text-blue-400 hover:text-blue-300 text-sm font-mono"
                target="_blank"
                rel="noopener noreferrer"
              >
                http://localhost:18080
              </a>
            </div>

            {/* Next Steps */}
            <div className="mt-8 pt-6 border-t border-gray-700">
              <h3 className="text-lg font-semibold mb-4">Next Steps</h3>
              <ul className="space-y-2 text-gray-400">
                <li className="flex items-start">
                  <span className="mr-2">•</span>
                  <span>
                    Visit the{' '}
                    <a href="/tokens" className="text-blue-400 hover:text-blue-300">
                      Auth Tokens
                    </a>{' '}
                    page to manage your API tokens
                  </span>
                </li>
                <li className="flex items-start">
                  <span className="mr-2">•</span>
                  <span>
                    View your{' '}
                    <a href="/tunnels" className="text-blue-400 hover:text-blue-300">
                      Active Tunnels
                    </a>{' '}
                    and monitor traffic
                  </span>
                </li>
                <li className="flex items-start">
                  <span className="mr-2">•</span>
                  <span>
                    Check out the{' '}
                    <a href="https://github.com/localup-dev/localup" target="_blank" rel="noopener noreferrer" className="text-blue-400 hover:text-blue-300">
                      documentation
                    </a>{' '}
                    for advanced features
                  </span>
                </li>
              </ul>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
