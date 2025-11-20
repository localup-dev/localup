import { type ReactNode, useEffect } from 'react';
import { NavLink, useNavigate } from 'react-router-dom';
import { useQuery, useMutation } from '@tanstack/react-query';
import { getCurrentUserOptions, logoutMutation } from '../api/client/@tanstack/react-query.gen';
import { useTeam } from '../contexts/TeamContext';

interface LayoutProps {
  children: ReactNode;
}

export default function Layout({ children }: LayoutProps) {
  const navigate = useNavigate();
  const { teams, selectedTeam, selectTeam } = useTeam();

  const { data: user, isError } = useQuery({
    ...getCurrentUserOptions(),
    retry: false,
  });

  // Redirect to login if not authenticated (React Query v5 pattern)
  useEffect(() => {
    if (isError) {
      navigate('/login');
    }
  }, [isError, navigate]);

  const logout = useMutation({
    ...logoutMutation(),
    onSuccess: () => {
      navigate('/login');
    },
  });

  const handleLogout = () => {
    logout.mutate({});
  };

  return (
    <div className="min-h-screen bg-gray-900 flex">
      {/* Sidebar */}
      <div className="w-64 bg-gray-800 border-r border-gray-700 flex flex-col">
        <div className="p-6">
          <h1 className="text-2xl font-bold text-white">LocalUp</h1>
          <p className="text-sm text-gray-400 mt-1">{user?.email}</p>
        </div>

        {/* Team Selector */}
        {teams.length > 0 && (
          <div className="px-4 mb-4">
            <label htmlFor="team-select" className="block text-xs text-gray-400 mb-2">
              Team
            </label>
            <select
              id="team-select"
              value={selectedTeam?.id || ''}
              onChange={(e) => {
                const team = teams.find((t) => t.id === e.target.value);
                if (team) selectTeam(team);
              }}
              className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-md text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
            >
              {teams.map((team) => (
                <option key={team.id} value={team.id}>
                  {team.name}
                </option>
              ))}
            </select>
          </div>
        )}

        <nav className="flex-1 px-4">
          <div className="space-y-1">
            <NavLink
              to="/dashboard"
              className={({ isActive }) =>
                `block px-4 py-2 rounded-md text-sm font-medium transition ${
                  isActive
                    ? 'bg-gray-700 text-white'
                    : 'text-gray-400 hover:bg-gray-700 hover:text-white'
                }`
              }
            >
              🏠 Getting Started
            </NavLink>

            <NavLink
              to="/tunnels"
              className={({ isActive }) =>
                `block px-4 py-2 rounded-md text-sm font-medium transition ${
                  isActive
                    ? 'bg-gray-700 text-white'
                    : 'text-gray-400 hover:bg-gray-700 hover:text-white'
                }`
              }
            >
              🔌 Tunnels
            </NavLink>

            <NavLink
              to="/tokens"
              className={({ isActive }) =>
                `block px-4 py-2 rounded-md text-sm font-medium transition ${
                  isActive
                    ? 'bg-gray-700 text-white'
                    : 'text-gray-400 hover:bg-gray-700 hover:text-white'
                }`
              }
            >
              🔑 Auth Tokens
            </NavLink>
          </div>
        </nav>

        <div className="p-4 border-t border-gray-700">
          <button
            onClick={handleLogout}
            className="w-full px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded-md text-sm font-medium text-gray-300 transition"
          >
            🚪 Logout
          </button>
        </div>
      </div>

      {/* Main Content */}
      <div className="flex-1 overflow-auto">{children}</div>
    </div>
  );
}
