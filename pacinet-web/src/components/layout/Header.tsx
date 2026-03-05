import { useLocation, useNavigate } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import { Menu, RefreshCw, Sun, Moon, Keyboard, Monitor, Minimize2, Maximize2, Search } from 'lucide-react';
import { useState, useCallback, useEffect } from 'react';

const THEME_KEY = 'pacinet_theme';

const titles: Record<string, string> = {
  '/': 'Dashboard',
  '/nodes': 'Nodes',
  '/deploy': 'Deploy',
  '/counters': 'Counters',
  '/fsm': 'FSM',
  '/watch': 'Watch',
  '/wallboard': 'Wallboard',
  '/audit': 'Audit Log',
  '/templates': 'Templates',
};

interface HeaderProps {
  compact: boolean;
  onToggleCompact: () => void;
  onMenuToggle: () => void;
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName.toLowerCase();
  return tag === 'input' || tag === 'textarea' || tag === 'select' || target.isContentEditable;
}

export default function Header({ compact, onToggleCompact, onMenuToggle }: HeaderProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [showShortcuts, setShowShortcuts] = useState(false);
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

  const refreshAll = useCallback(() => {
    queryClient.invalidateQueries();
  }, [queryClient]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && showShortcuts) {
        setShowShortcuts(false);
        return;
      }

      if (isTypingTarget(event.target)) return;

      if (event.key === '?') {
        event.preventDefault();
        setShowShortcuts((v) => !v);
        return;
      }

      if (!(event.ctrlKey && event.shiftKey)) return;

      const key = event.key.toLowerCase();

      if (key === 'r') {
        event.preventDefault();
        refreshAll();
      } else if (key === 'w') {
        event.preventDefault();
        navigate('/wallboard');
      } else if (key === 'd') {
        event.preventDefault();
        onToggleCompact();
      } else if (key === 'h') {
        event.preventDefault();
        navigate('/');
      } else if (key === 'n') {
        event.preventDefault();
        navigate('/nodes');
      } else if (key === 'f') {
        event.preventDefault();
        navigate('/fsm');
      }
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [navigate, onToggleCompact, refreshAll, showShortcuts]);

  const title = titles[location.pathname] || 'PaciNet';

  return (
    <>
      <header
        className={`${compact ? 'h-14' : 'h-16'} border-b border-edge/80 flex items-center justify-between px-3 md:px-6 bg-surface-alt/90 backdrop-blur-md`}
      >
        <div className="flex items-center gap-3">
          <button
            onClick={onMenuToggle}
            className="md:hidden p-2 rounded-lg hover:bg-surface-hover text-content-secondary hover:text-content transition-colors"
            aria-label="Toggle menu"
          >
            <Menu size={16} />
          </button>
          <div>
            <h1 className={`${compact ? 'text-base' : 'text-lg'} font-semibold tracking-tight`}>{title}</h1>
            <p className="text-xs text-content-muted">Manage and monitor your PacGate fleet</p>
          </div>
        </div>
        <div className="flex items-center gap-1.5 md:gap-2">
          <button
            onClick={() => window.dispatchEvent(new CustomEvent('pacinet:open-command-palette'))}
            className="p-2.5 rounded-xl hover:bg-surface-hover text-content-secondary hover:text-content transition-colors"
            title="Command palette (Ctrl/Cmd+K)"
          >
            <Search size={16} />
          </button>
          <button
            onClick={() => navigate('/wallboard')}
            className="p-2.5 rounded-xl hover:bg-surface-hover text-content-secondary hover:text-content transition-colors"
            title="Wallboard (Ctrl+Shift+W)"
          >
            <Monitor size={16} />
          </button>
          <button
            onClick={() => setShowShortcuts(true)}
            className="p-2.5 rounded-xl hover:bg-surface-hover text-content-secondary hover:text-content transition-colors"
            title="Keyboard shortcuts (?)"
          >
            <Keyboard size={16} />
          </button>
          <button
            onClick={onToggleCompact}
            className="p-2.5 rounded-xl hover:bg-surface-hover text-content-secondary hover:text-content transition-colors"
            title="Toggle compact mode (Ctrl+Shift+D)"
          >
            {compact ? <Maximize2 size={16} /> : <Minimize2 size={16} />}
          </button>
          <button
            onClick={refreshAll}
            className="p-2.5 rounded-xl hover:bg-surface-hover text-content-secondary hover:text-content transition-colors"
            title="Refresh (Ctrl+Shift+R)"
          >
            <RefreshCw size={16} />
          </button>
          <button
            onClick={toggleTheme}
            className="p-2.5 rounded-xl hover:bg-surface-hover text-content-secondary hover:text-content transition-colors"
            title="Toggle theme"
          >
            {dark ? <Sun size={16} /> : <Moon size={16} />}
          </button>
        </div>
      </header>

      {showShortcuts && (
        <div className="fixed inset-0 z-50 bg-black/50 backdrop-blur-sm p-4" onClick={() => setShowShortcuts(false)}>
          <div
            className="mx-auto mt-16 w-full max-w-lg rounded-2xl border border-edge bg-surface-alt p-5 shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="mb-3 text-sm uppercase tracking-[0.14em] text-content-muted">Operator Shortcuts</div>
            <div className="space-y-2 text-sm">
              <div className="flex items-center justify-between rounded-lg border border-edge bg-surface px-3 py-2">
                <span>Open command palette</span>
                <code className="text-xs">Ctrl/Cmd+K</code>
              </div>
              <div className="flex items-center justify-between rounded-lg border border-edge bg-surface px-3 py-2">
                <span>Refresh all data</span>
                <code className="text-xs">Ctrl+Shift+R</code>
              </div>
              <div className="flex items-center justify-between rounded-lg border border-edge bg-surface px-3 py-2">
                <span>Toggle compact mode</span>
                <code className="text-xs">Ctrl+Shift+D</code>
              </div>
              <div className="flex items-center justify-between rounded-lg border border-edge bg-surface px-3 py-2">
                <span>Open wallboard</span>
                <code className="text-xs">Ctrl+Shift+W</code>
              </div>
              <div className="flex items-center justify-between rounded-lg border border-edge bg-surface px-3 py-2">
                <span>Go dashboard / nodes / FSM</span>
                <code className="text-xs">Ctrl+Shift+H/N/F</code>
              </div>
              <div className="flex items-center justify-between rounded-lg border border-edge bg-surface px-3 py-2">
                <span>Open or close this help</span>
                <code className="text-xs">?</code>
              </div>
            </div>
            <button
              onClick={() => setShowShortcuts(false)}
              className="mt-4 w-full rounded-lg bg-accent px-3 py-2 text-sm font-medium text-white hover:bg-accent-hover"
            >
              Close
            </button>
          </div>
        </div>
      )}
    </>
  );
}
