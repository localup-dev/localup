import { type ReactNode, useEffect } from 'react';
import { NavLink, useNavigate } from 'react-router-dom';
import { useQuery, useMutation } from '@tanstack/react-query';
import { Home, Cable, Key, Globe, LogOut, ChevronDown } from 'lucide-react';
import { getCurrentUserOptions, logoutMutation } from '../api/client/@tanstack/react-query.gen';
import { useTeam } from '../contexts/TeamContext';
import { Avatar, AvatarFallback } from './ui/avatar';
import { Separator } from './ui/separator';
import { Button } from './ui/button';
import { Logo } from './Logo';

interface LayoutProps {
  children: ReactNode;
}

const navItems = [
  { to: '/dashboard', icon: Home, label: 'Getting Started' },
  { to: '/tunnels', icon: Cable, label: 'Tunnels' },
  { to: '/domains', icon: Globe, label: 'Custom Domains' },
  { to: '/tokens', icon: Key, label: 'Auth Tokens' },
];

export default function Layout({ children }: LayoutProps) {
  const navigate = useNavigate();
  const { teams, selectedTeam, selectTeam } = useTeam();

  const { data: userData, isLoading, isError } = useQuery({
    ...getCurrentUserOptions(),
    retry: false,
  });

  // API returns { user: { email, ... } } structure
  const user = userData?.user as { email?: string; id?: string } | undefined;

  const logout = useMutation({
    ...logoutMutation(),
    onSuccess: () => {
      navigate('/login');
    },
  });

  useEffect(() => {
    if (isError) {
      navigate('/login');
    }
  }, [isError, navigate]);

  const handleLogout = () => {
    logout.mutate({});
  };

  const getInitials = (email?: string) => {
    if (!email) return '?';
    return email.substring(0, 2).toUpperCase();
  };

  // Show loading state while checking authentication
  if (isLoading) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="text-muted-foreground">Loading...</div>
      </div>
    );
  }

  // If error occurred, we're redirecting - don't render anything
  if (isError) {
    return null;
  }

  return (
    <div className="min-h-screen bg-background flex">
      {/* Sidebar */}
      <div className="w-64 bg-card border-r border-border flex flex-col">
        <div className="p-6">
          <div className="flex items-center justify-between">
            <Logo size="md" className="text-foreground" />
            <span className="text-xs text-muted-foreground">v{__APP_VERSION__}</span>
          </div>
          <div className="flex items-center gap-3 mt-4">
            <Avatar className="h-8 w-8">
              <AvatarFallback className="bg-primary text-primary-foreground text-xs">
                {getInitials(user?.email)}
              </AvatarFallback>
            </Avatar>
            <p className="text-sm text-muted-foreground truncate">{user?.email}</p>
          </div>
        </div>

        <Separator />

        {/* Team Selector */}
        {teams.length > 0 && (
          <div className="px-4 py-4">
            <label htmlFor="team-select" className="block text-xs text-muted-foreground mb-2">
              Team
            </label>
            <div className="relative">
              <select
                id="team-select"
                value={selectedTeam?.id || ''}
                onChange={(e) => {
                  const team = teams.find((t) => t.id === e.target.value);
                  if (team) selectTeam(team);
                }}
                className="w-full px-3 py-2 bg-muted border border-border rounded-md text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-ring appearance-none cursor-pointer"
              >
                {teams.map((team) => (
                  <option key={team.id} value={team.id}>
                    {team.name}
                  </option>
                ))}
              </select>
              <ChevronDown className="absolute right-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground pointer-events-none" />
            </div>
          </div>
        )}

        <nav className="flex-1 px-4 py-2">
          <div className="space-y-1">
            {navItems.map((item) => {
              const Icon = item.icon;
              return (
                <NavLink
                  key={item.to}
                  to={item.to}
                  className={({ isActive }) =>
                    `flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium transition-colors ${
                      isActive
                        ? 'bg-muted text-foreground'
                        : 'text-muted-foreground hover:bg-muted hover:text-foreground'
                    }`
                  }
                >
                  <Icon className="h-4 w-4" />
                  {item.label}
                </NavLink>
              );
            })}
          </div>
        </nav>

        <Separator />

        <div className="p-4">
          <Button
            onClick={handleLogout}
            variant="ghost"
            className="w-full justify-start gap-3 text-muted-foreground hover:text-foreground"
          >
            <LogOut className="h-4 w-4" />
            Logout
          </Button>
        </div>
      </div>

      {/* Main Content */}
      <div className="flex-1 overflow-auto">{children}</div>
    </div>
  );
}
