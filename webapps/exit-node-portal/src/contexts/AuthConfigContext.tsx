import { createContext, useContext, useState, useEffect, type ReactNode } from 'react';
import { getAuthConfig } from '../utils/api';

interface AuthConfig {
  signup_enabled: boolean;
}

interface AuthConfigContextType {
  authConfig: AuthConfig | null;
  loading: boolean;
}

const AuthConfigContext = createContext<AuthConfigContextType | undefined>(undefined);

export function AuthConfigProvider({ children }: { children: ReactNode }) {
  const [authConfig, setAuthConfig] = useState<AuthConfig | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function fetchAuthConfig() {
      try {
        const config = await getAuthConfig();
        setAuthConfig(config);
      } catch (error) {
        console.error('Failed to fetch auth config:', error);
        // Default to signup disabled on error for security
        setAuthConfig({ signup_enabled: false });
      } finally {
        setLoading(false);
      }
    }

    fetchAuthConfig();
  }, []);

  return (
    <AuthConfigContext.Provider value={{ authConfig, loading }}>
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
