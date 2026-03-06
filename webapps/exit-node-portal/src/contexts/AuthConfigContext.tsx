import { createContext, useContext, type ReactNode } from 'react';
import { useQuery } from '@tanstack/react-query';
import { authConfigOptions } from '../api/client/@tanstack/react-query.gen';
import type { AuthConfigResponse } from '../api/client/types.gen';

/** Social auth provider info returned by the backend */
export interface SocialAuthProvider {
  /** Provider identifier (e.g., "google", "github") */
  id: string;
  /** Human-readable provider name */
  name: string;
}

/**
 * Extended AuthConfig type that includes social_providers and magic_link_enabled.
 * This extends the auto-generated type until the API client is regenerated.
 */
export type AuthConfigWithSocial = AuthConfigResponse & {
  social_providers?: SocialAuthProvider[];
  magic_link_enabled?: boolean;
};

interface AuthConfigContextType {
  authConfig: AuthConfigWithSocial | null;
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
    <AuthConfigContext.Provider value={{ authConfig: (authConfig as AuthConfigWithSocial) ?? null, loading: isLoading }}>
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
