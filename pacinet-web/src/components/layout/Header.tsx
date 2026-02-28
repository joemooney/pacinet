import { useLocation } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import { RefreshCw, Sun, Moon } from 'lucide-react';
import { useState, useCallback } from 'react';

const titles: Record<string, string> = {
  '/': 'Dashboard',
  '/nodes': 'Nodes',
  '/deploy': 'Deploy',
  '/counters': 'Counters',
  '/fsm': 'FSM',
  '/watch': 'Watch',
};

export default function Header() {
  const location = useLocation();
  const queryClient = useQueryClient();
  const [dark, setDark] = useState(true);

  const toggleTheme = useCallback(() => {
    setDark((d) => {
      document.documentElement.classList.toggle('light', d);
      return !d;
    });
  }, []);

  const title = titles[location.pathname] || 'PaciNet';

  return (
    <header className="h-14 border-b border-edge flex items-center justify-between px-6 bg-surface-alt">
      <h1 className="text-lg font-semibold">{title}</h1>
      <div className="flex items-center gap-2">
        <button
          onClick={() => queryClient.invalidateQueries()}
          className="p-2 rounded-lg hover:bg-surface-hover text-content-secondary hover:text-content transition-colors"
          title="Refresh"
        >
          <RefreshCw size={16} />
        </button>
        <button
          onClick={toggleTheme}
          className="p-2 rounded-lg hover:bg-surface-hover text-content-secondary hover:text-content transition-colors"
          title="Toggle theme"
        >
          {dark ? <Sun size={16} /> : <Moon size={16} />}
        </button>
      </div>
    </header>
  );
}
