import { NavLink } from 'react-router-dom';
import {
  LayoutDashboard,
  Server,
  Upload,
  BarChart3,
  GitBranch,
  Radio,
  ClipboardList,
  FileText,
  Monitor,
  X,
} from 'lucide-react';

const navItems = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/nodes', icon: Server, label: 'Nodes' },
  { to: '/deploy', icon: Upload, label: 'Deploy' },
  { to: '/counters', icon: BarChart3, label: 'Counters' },
  { to: '/fsm', icon: GitBranch, label: 'FSM' },
  { to: '/watch', icon: Radio, label: 'Watch' },
  { to: '/wallboard', icon: Monitor, label: 'Wallboard' },
  { to: '/audit', icon: ClipboardList, label: 'Audit' },
  { to: '/templates', icon: FileText, label: 'Templates' },
];

interface SidebarProps {
  compact: boolean;
  open: boolean;
  onClose: () => void;
}

export default function Sidebar({ compact, open, onClose }: SidebarProps) {
  return (
    <>
      <div
        className={`fixed inset-0 z-30 bg-black/45 backdrop-blur-[1px] transition-opacity md:hidden ${
          open ? 'opacity-100' : 'pointer-events-none opacity-0'
        }`}
        onClick={onClose}
      />
      <aside
        className={`fixed inset-y-0 left-0 z-40 ${compact ? 'w-64 md:w-56' : 'w-72 md:w-64'} bg-surface-alt/95 backdrop-blur-md border-r border-edge flex flex-col transition-transform md:static md:translate-x-0 ${
          open ? 'translate-x-0' : '-translate-x-full'
        }`}
      >
        <div className={`${compact ? 'h-14' : 'h-16'} flex items-center justify-between px-5 border-b border-edge`}>
          <div>
            <div className="text-[11px] uppercase tracking-[0.18em] text-content-muted">Control Plane</div>
            <span className={`${compact ? 'text-lg' : 'text-xl'} font-semibold text-content`}>PaciNet</span>
          </div>
          <button
            className="md:hidden rounded-lg p-2 text-content-secondary hover:text-content hover:bg-surface-hover"
            onClick={onClose}
            aria-label="Close menu"
          >
            <X size={16} />
          </button>
        </div>

        <nav className="flex-1 py-3">
          {navItems.map(({ to, icon: Icon, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              onClick={onClose}
              className={({ isActive }) =>
                `mx-2 flex items-center gap-3 rounded-xl ${compact ? 'px-3 py-2' : 'px-4 py-2.5'} text-sm transition-colors ${
                  isActive
                    ? 'text-content bg-accent/20 ring-1 ring-accent/30'
                    : 'text-content-secondary hover:text-content hover:bg-surface-hover'
                }`
              }
            >
              <Icon size={18} />
              {label}
            </NavLink>
          ))}
        </nav>

        <div className="m-3 rounded-xl border border-edge bg-surface px-4 py-3">
          <div className="text-[11px] uppercase tracking-[0.12em] text-content-muted">System</div>
          <div className="mt-1 text-sm text-content-secondary">Fleet controller active</div>
        </div>
      </aside>
    </>
  );
}
