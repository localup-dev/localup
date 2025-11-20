import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { AuthConfigProvider } from './contexts/AuthConfigContext';
import { TeamProvider } from './contexts/TeamContext';
import Layout from './components/Layout';
import Login from './pages/Login';
import Register from './pages/Register';
import Dashboard from './pages/Dashboard';
import AuthTokens from './pages/AuthTokens';
import Tunnels from './pages/Tunnels';
import TunnelDetail from './pages/TunnelDetail';
import { client } from './api/client/client.gen';

// Configure OpenAPI client with credentials and base URL
// VITE_API_BASE_URL can be:
// - undefined/not set: defaults to 'http://localhost:13080'
// - empty string "": uses relative paths (same origin)
// - full URL: uses specified backend URL
const apiBaseUrl = import.meta.env.VITE_API_BASE_URL !== undefined
  ? import.meta.env.VITE_API_BASE_URL
  : 'http://localhost:13080';

client.setConfig({
  baseUrl: apiBaseUrl,
  // Include credentials to send HTTP-only session cookies automatically
  credentials: 'include',
});

// Note: Auth checking is now handled by individual pages via getCurrentUser() API call
// No client-side auth state - pages check with backend and redirect if needed

// Create a client
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchInterval: 3000, // Auto-refresh every 3 seconds
      refetchOnWindowFocus: false,
    },
  },
});

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <AuthConfigProvider>
        <TeamProvider>
          <BrowserRouter>
            <Routes>
              <Route path="/login" element={<Login />} />
              <Route path="/register" element={<Register />} />
              <Route path="/dashboard" element={<Layout><Dashboard /></Layout>} />
              <Route path="/tokens" element={<Layout><AuthTokens /></Layout>} />
              <Route path="/tunnels" element={<Layout><Tunnels /></Layout>} />
              <Route path="/tunnels/:tunnelId" element={<Layout><TunnelDetail /></Layout>} />
              <Route path="/" element={<Navigate to="/dashboard" />} />
            </Routes>
          </BrowserRouter>
        </TeamProvider>
      </AuthConfigProvider>
    </QueryClientProvider>
  );
}

export default App;
