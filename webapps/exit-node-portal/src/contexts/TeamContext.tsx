import { createContext, useContext, useState, useEffect, type ReactNode } from 'react';
import { useQuery } from '@tanstack/react-query';
import { listUserTeamsOptions } from '../api/client/@tanstack/react-query.gen';
import type { Team } from '../api/client/types.gen';

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

export const TeamProvider = ({ children }: TeamProviderProps) => {
  const [selectedTeam, setSelectedTeam] = useState<Team | null>(null);

  const { data, isLoading } = useQuery(listUserTeamsOptions());
  const teams = data?.teams || [];

  // Auto-select first team when data loads
  useEffect(() => {
    if (teams.length > 0 && !selectedTeam) {
      setSelectedTeam(teams[0]);
    }
  }, [teams, selectedTeam]);

  const selectTeam = (team: Team) => {
    setSelectedTeam(team);
  };

  return (
    <TeamContext.Provider value={{ teams, selectedTeam, selectTeam, isLoading }}>
      {children}
    </TeamContext.Provider>
  );
};
