import { createContext, useContext, type ReactNode } from 'react';
import { useQuery } from '@tanstack/react-query';
import { authConfigOptions } from '../api/client/@tanstack/react-query.gen';
import type { AuthConfigResponse } from '../api/client/types.gen';

interface AuthConfigContextType {
  authConfig: AuthConfigResponse | null;
  loading: boolean;
}

const AuthConfigContext = createContext<AuthConfigContextType | undefined>(undefined);

export function AuthConfigProvider({ children }: { children: ReactNode }) {
  const { data: authConfig, isLoading } = useQuery({
    ...authConfigOptions(),
    // Default to signup disabled on error for security
    placeholderData: { signup_enabled: false },
  });

  return (
    <AuthConfigContext.Provider value={{ authConfig: authConfig ?? null, loading: isLoading }}>
      {children}
    </AuthConfigContext.Provider>
  );
}

export function useAuthConfig() {
  const context = useContext(AuthConfigContext);
  if (context === undefined) {
    throw new Error('useAuthConfig must be used within an AuthConfigProvider');
  }
  return context;
}
