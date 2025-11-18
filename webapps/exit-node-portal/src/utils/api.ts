const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || 'http://localhost:13080';

export const apiCall = async (endpoint: string, options: RequestInit = {}) => {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options.headers as Record<string, string>),
  };

  // Include credentials to send HTTP-only session cookies automatically
  const response = await fetch(`${API_BASE_URL}${endpoint}`, {
    ...options,
    headers,
    credentials: 'include', // Always include cookies (session_token)
  });

  if (!response.ok) {
    const error = await response.json().catch(() => ({ error: 'Request failed', code: null }));
    const err: any = new Error(error.error || `HTTP ${response.status}`);
    err.code = error.code; // Attach error code for special handling
    throw err;
  }

  return response.json();
};

export const register = async (email: string, password: string, username: string) => {
  return apiCall('/api/auth/register', {
    method: 'POST',
    body: JSON.stringify({ email, password, username }),
  });
};

export const login = async (email: string, password: string) => {
  return apiCall('/api/auth/login', {
    method: 'POST',
    body: JSON.stringify({ email, password }),
  });
};

export const getAuthConfig = async () => {
  return apiCall('/api/auth/config');
};

export const listAuthTokens = async () => {
  return apiCall('/api/auth-tokens');
};

export const getAuthTokens = listAuthTokens; // Alias for consistency

export const createAuthToken = async (params: {
  name: string;
  description?: string | null;
  expires_in_days?: number | null;
  team_id?: string | null;
}) => {
  return apiCall('/api/auth-tokens', {
    method: 'POST',
    body: JSON.stringify(params),
  });
};

export const deleteAuthToken = async (id: string) => {
  return apiCall(`/api/auth-tokens/${id}`, {
    method: 'DELETE',
  });
};

export const listTunnels = async () => {
  return apiCall('/api/tunnels');
};
