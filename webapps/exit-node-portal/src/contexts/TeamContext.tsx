import { createContext, useContext, useState, useEffect, type ReactNode } from 'react';

interface Team {
  id: string;
  name: string;
  slug: string;
  role: string;
  created_at: string;
}

interface TeamContextType {
  teams: Team[];
  selectedTeam: Team | null;
  selectTeam: (team: Team) => void;
  isLoading: boolean;
}

const TeamContext = createContext<TeamContextType | undefined>(undefined);

export const useTeam = () => {
  const context = useContext(TeamContext);
  if (!context) {
    throw new Error('useTeam must be used within TeamProvider');
  }
  return context;
};

interface TeamProviderProps {
  children: ReactNode;
}

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || 'http://localhost:13080';

export const TeamProvider = ({ children }: TeamProviderProps) => {
  const [teams, setTeams] = useState<Team[]>([]);
  const [selectedTeam, setSelectedTeam] = useState<Team | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    async function loadTeams() {
      try {
        setIsLoading(true);
        const response = await fetch(`${API_BASE_URL}/api/teams`, {
          credentials: 'include', // Include HTTP-only session cookie
        });

        if (response.ok) {
          const data = await response.json();
          setTeams(data.teams);

          // Auto-select first team if none selected
          if (data.teams.length > 0 && !selectedTeam) {
            setSelectedTeam(data.teams[0]);
          }
        }
      } catch (error) {
        console.error('Failed to load teams:', error);
      } finally {
        setIsLoading(false);
      }
    }

    loadTeams();
  }, []);

  const selectTeam = (team: Team) => {
    setSelectedTeam(team);
  };

  return (
    <TeamContext.Provider value={{ teams, selectedTeam, selectTeam, isLoading }}>
      {children}
    </TeamContext.Provider>
  );
};
