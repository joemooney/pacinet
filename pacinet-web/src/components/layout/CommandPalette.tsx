import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import { Command, Gauge, Keyboard, Search } from 'lucide-react';

interface CommandPaletteProps {
  compact: boolean;
  onToggleCompact: () => void;
}

interface PaletteCommand {
  id: string;
  label: string;
  hint?: string;
  keywords: string;
  run: () => void;
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName.toLowerCase();
  return tag === 'input' || tag === 'textarea' || tag === 'select' || target.isContentEditable;
}

export default function CommandPalette({ compact, onToggleCompact }: CommandPaletteProps) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);

  const commands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'go-dashboard',
        label: 'Go to Dashboard',
        hint: 'Ctrl+Shift+H',
        keywords: 'dashboard home overview',
        run: () => navigate('/'),
      },
      {
        id: 'go-nodes',
        label: 'Go to Nodes',
        hint: 'Ctrl+Shift+N',
        keywords: 'nodes fleet agents',
        run: () => navigate('/nodes'),
      },
      {
        id: 'go-fsm',
        label: 'Go to FSM',
        hint: 'Ctrl+Shift+F',
        keywords: 'fsm orchestration rollout',
        run: () => navigate('/fsm'),
      },
      {
        id: 'go-watch',
        label: 'Go to Watch',
        keywords: 'watch events stream',
        run: () => navigate('/watch'),
      },
      {
        id: 'go-wallboard',
        label: 'Open Wallboard',
        hint: 'Ctrl+Shift+W',
        keywords: 'wallboard noc monitor',
        run: () => navigate('/wallboard'),
      },
      {
        id: 'refresh',
        label: 'Refresh All Data',
        hint: 'Ctrl+Shift+R',
        keywords: 'refresh reload queries sync',
        run: () => {
          queryClient.invalidateQueries();
        },
      },
      {
        id: 'density',
        label: compact ? 'Switch to Comfortable Density' : 'Switch to Compact Density',
        hint: 'Ctrl+Shift+D',
        keywords: 'density compact comfortable spacing',
        run: () => onToggleCompact(),
      },
      {
        id: 'go-audit',
        label: 'Go to Audit Log',
        keywords: 'audit log compliance',
        run: () => navigate('/audit'),
      },
      {
        id: 'go-templates',
        label: 'Go to Templates',
        keywords: 'templates policy snippets',
        run: () => navigate('/templates'),
      },
      {
        id: 'go-deploy',
        label: 'Go to Deploy',
        keywords: 'deploy rollout policy',
        run: () => navigate('/deploy'),
      },
      {
        id: 'go-counters',
        label: 'Go to Counters',
        keywords: 'counters rates telemetry',
        run: () => navigate('/counters'),
      },
    ],
    [compact, navigate, onToggleCompact, queryClient]
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter((cmd) => {
      const hay = `${cmd.label} ${cmd.keywords}`.toLowerCase();
      return hay.includes(q);
    });
  }, [commands, query]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query, open]);

  useEffect(() => {
    const handleOpen = () => setOpen(true);
    window.addEventListener('pacinet:open-command-palette', handleOpen as EventListener);
    return () => window.removeEventListener('pacinet:open-command-palette', handleOpen as EventListener);
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setOpen((v) => !v);
        return;
      }

      if (!open) return;

      if (event.key === 'Escape') {
        event.preventDefault();
        setOpen(false);
        return;
      }

      if (isTypingTarget(event.target) && event.key !== 'ArrowDown' && event.key !== 'ArrowUp' && event.key !== 'Enter') {
        return;
      }

      if (event.key === 'ArrowDown') {
        event.preventDefault();
        setActiveIndex((prev) => (filtered.length === 0 ? 0 : (prev + 1) % filtered.length));
      } else if (event.key === 'ArrowUp') {
        event.preventDefault();
        setActiveIndex((prev) => (filtered.length === 0 ? 0 : (prev - 1 + filtered.length) % filtered.length));
      } else if (event.key === 'Enter') {
        event.preventDefault();
        const cmd = filtered[activeIndex];
        if (!cmd) return;
        cmd.run();
        setOpen(false);
      }
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [activeIndex, filtered, open]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[60] bg-black/50 backdrop-blur-sm p-4" onClick={() => setOpen(false)}>
      <div
        className="mx-auto mt-16 w-full max-w-2xl rounded-2xl border border-edge bg-surface-alt shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="border-b border-edge p-3 md:p-4">
          <div className="flex items-center gap-2">
            <Search size={16} className="text-content-muted" />
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search commands, pages, and actions..."
              className="w-full bg-transparent text-sm text-content placeholder:text-content-muted focus:outline-none"
            />
            <span className="inline-flex items-center gap-1 rounded-md border border-edge px-2 py-1 text-[11px] text-content-muted">
              <Command size={12} />K
            </span>
          </div>
        </div>

        <div className="max-h-[60vh] overflow-y-auto p-2">
          {filtered.length === 0 ? (
            <div className="px-3 py-6 text-center text-sm text-content-muted">No commands matched.</div>
          ) : (
            filtered.map((cmd, idx) => (
              <button
                key={cmd.id}
                onClick={() => {
                  cmd.run();
                  setOpen(false);
                }}
                className={`flex w-full items-center gap-3 rounded-xl px-3 py-2 text-left text-sm transition-colors ${
                  idx === activeIndex ? 'bg-accent/20 text-content ring-1 ring-accent/30' : 'hover:bg-surface'
                }`}
              >
                <span className="inline-flex h-7 w-7 items-center justify-center rounded-md border border-edge bg-surface text-content-secondary">
                  {cmd.id.includes('go-') ? <Gauge size={14} /> : <Keyboard size={14} />}
                </span>
                <span className="flex-1">{cmd.label}</span>
                {cmd.hint && <span className="text-xs text-content-muted">{cmd.hint}</span>}
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
