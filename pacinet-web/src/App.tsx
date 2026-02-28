import { BrowserRouter, Routes, Route } from 'react-router-dom';
import AppLayout from './components/layout/AppLayout';
import DashboardPage from './components/dashboard/DashboardPage';
import NodesPage from './components/nodes/NodesPage';
import DeployPage from './components/deploy/DeployPage';
import CountersPage from './components/counters/CountersPage';
import FsmPage from './components/fsm/FsmPage';
import WatchPage from './components/watch/WatchPage';

export default function App() {
  return (
    <BrowserRouter>
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
