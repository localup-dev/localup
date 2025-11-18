import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { AuthConfigProvider } from './contexts/AuthConfigContext';
import { TeamProvider } from './contexts/TeamContext';
import Layout from './components/Layout';
import Login from './pages/Login';
import Register from './pages/Register';
import Dashboard from './pages/Dashboard';
import AuthTokens from './pages/AuthTokens';
import Tunnels from './pages/Tunnels';

// Note: Auth checking is now handled by individual pages via getCurrentUser() API call
// No client-side auth state - pages check with backend and redirect if needed

function App() {
  return (
    <AuthConfigProvider>
      <TeamProvider>
        <BrowserRouter>
          <Routes>
            <Route path="/login" element={<Login />} />
            <Route path="/register" element={<Register />} />
            <Route path="/dashboard" element={<Layout><Dashboard /></Layout>} />
            <Route path="/tokens" element={<Layout><AuthTokens /></Layout>} />
            <Route path="/tunnels" element={<Layout><Tunnels /></Layout>} />
            <Route path="/" element={<Navigate to="/dashboard" />} />
          </Routes>
        </BrowserRouter>
      </TeamProvider>
    </AuthConfigProvider>
  );
}

export default App;
