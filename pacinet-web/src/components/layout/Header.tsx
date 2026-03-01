import { useLocation } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import { Menu, RefreshCw, Sun, Moon } from 'lucide-react';
import { useState, useCallback, useEffect } from 'react';

const THEME_KEY = 'pacinet_theme';

const titles: Record<string, string> = {
  '/': 'Dashboard',
  '/nodes': 'Nodes',
  '/deploy': 'Deploy',
  '/counters': 'Counters',
  '/fsm': 'FSM',
  '/watch': 'Watch',
  '/audit': 'Audit Log',
  '/templates': 'Templates',
};

interface HeaderProps {
  onMenuToggle?: () => void;
}

export default function Header({ onMenuToggle }: HeaderProps) {
  const location = useLocation();
  const queryClient = useQueryClient();
  const [dark, setDark] = useState(() => {
    const saved = localStorage.getItem(THEME_KEY);
    return saved !== 'light';
  });

  useEffect(() => {
    document.documentElement.classList.toggle('light', !dark);
    localStorage.setItem(THEME_KEY, dark ? 'dark' : 'light');
  }, [dark]);

  const toggleTheme = useCallback(() => {
    setDark((d) => !d);
  }, []);

  const title = titles[location.pathname] || 'PaciNet';

  return (
    <header className="h-14 border-b border-edge flex items-center justify-between px-6 bg-surface-alt">
      <div className="flex items-center gap-3">
        {onMenuToggle && (
          <button
            onClick={onMenuToggle}
            className="md:hidden p-2 rounded-lg hover:bg-surface-hover text-content-secondary hover:text-content transition-colors"
            aria-label="Toggle menu"
          >
            <Menu size={16} />
          </button>
        )}
        <h1 className="text-lg font-semibold">{title}</h1>
      </div>
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
