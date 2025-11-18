export interface User {
  id: string;
  email: string;
  username?: string;
  role: string;
}

// All authentication state is managed via HTTP-only session cookies
// No data is stored in localStorage for maximum security

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || 'http://localhost:13080';

/**
 * Get current user data from the backend
 * Returns user if authenticated, null if not
 */
export const getCurrentUser = async (): Promise<User | null> => {
  try {
    const response = await fetch(`${API_BASE_URL}/api/auth/me`, {
      credentials: 'include', // Include HTTP-only session cookie
    });

    if (response.ok) {
      const data = await response.json();
      return data.user;
    }

    return null;
  } catch (error) {
    console.error('Failed to get current user:', error);
    return null;
  }
};

/**
 * Logout the current user
 * Clears the session cookie on the backend
 */
export const logout = async (): Promise<void> => {
  try {
    await fetch(`${API_BASE_URL}/api/auth/logout`, {
      method: 'POST',
      credentials: 'include', // Include cookies in the request
    });
  } catch (error) {
    console.error('Failed to logout:', error);
  }
};
