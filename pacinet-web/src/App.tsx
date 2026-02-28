import { useState, useEffect, useCallback } from 'react';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import AppLayout from './components/layout/AppLayout';
import DashboardPage from './components/dashboard/DashboardPage';
import NodesPage from './components/nodes/NodesPage';
import DeployPage from './components/deploy/DeployPage';
import CountersPage from './components/counters/CountersPage';
import FsmPage from './components/fsm/FsmPage';
import WatchPage from './components/watch/WatchPage';
import ApiKeyPrompt from './components/auth/ApiKeyPrompt';

export default function App() {
  const [showAuthPrompt, setShowAuthPrompt] = useState(false);
  const queryClient = useQueryClient();

  const handleAuthRequired = useCallback(() => {
    setShowAuthPrompt(true);
  }, []);

  useEffect(() => {
    window.addEventListener('pacinet:auth-required', handleAuthRequired);
    return () => window.removeEventListener('pacinet:auth-required', handleAuthRequired);
  }, [handleAuthRequired]);

  const handleAuthSubmit = () => {
    setShowAuthPrompt(false);
    queryClient.invalidateQueries();
  };

  return (
    <BrowserRouter>
      {showAuthPrompt && (
        <ApiKeyPrompt
          onSubmit={handleAuthSubmit}
          onDismiss={() => setShowAuthPrompt(false)}
        />
      )}
      <Routes>
        <Route element={<AppLayout />}>
          <Route path="/" element={<DashboardPage />} />
          <Route path="/nodes" element={<NodesPage />} />
          <Route path="/deploy" element={<DeployPage />} />
          <Route path="/counters" element={<CountersPage />} />
          <Route path="/fsm" element={<FsmPage />} />
          <Route path="/watch" element={<WatchPage />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
