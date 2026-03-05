import { useEffect, useState } from 'react';
import { Outlet } from 'react-router-dom';
import Sidebar from './Sidebar';
import Header from './Header';
import AlertRibbons from './AlertRibbons';
import CommandPalette from './CommandPalette';

const DENSITY_KEY = 'pacinet_density';

export default function AppLayout() {
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [compact, setCompact] = useState(() => localStorage.getItem(DENSITY_KEY) === 'compact');

  useEffect(() => {
    document.documentElement.classList.toggle('density-compact', compact);
    localStorage.setItem(DENSITY_KEY, compact ? 'compact' : 'comfortable');
  }, [compact]);

  return (
    <div className="flex h-screen bg-surface text-content">
      <Sidebar open={sidebarOpen} compact={compact} onClose={() => setSidebarOpen(false)} />
      <div className="flex flex-col flex-1 overflow-hidden">
        <Header
          compact={compact}
          onToggleCompact={() => setCompact((v) => !v)}
          onMenuToggle={() => setSidebarOpen((s) => !s)}
        />
        <AlertRibbons compact={compact} />
        <main className={`relative flex-1 overflow-auto ${compact ? 'p-3 md:p-4 lg:p-5' : 'p-4 md:p-6 lg:p-8'}`}>
          <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_20%_0%,rgba(36,208,197,0.10),transparent_35%),radial-gradient(circle_at_80%_100%,rgba(18,149,202,0.12),transparent_30%)]" />
          <div className="relative z-10">
            <Outlet />
          </div>
        </main>
      </div>
      <CommandPalette compact={compact} onToggleCompact={() => setCompact((v) => !v)} />
    </div>
  );
}
