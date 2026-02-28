import { NavLink } from 'react-router-dom';
import {
  LayoutDashboard,
  Server,
  Upload,
  BarChart3,
  GitBranch,
  Radio,
} from 'lucide-react';

const navItems = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/nodes', icon: Server, label: 'Nodes' },
  { to: '/deploy', icon: Upload, label: 'Deploy' },
  { to: '/counters', icon: BarChart3, label: 'Counters' },
  { to: '/fsm', icon: GitBranch, label: 'FSM' },
  { to: '/watch', icon: Radio, label: 'Watch' },
];

export default function Sidebar() {
  return (
    <aside className="w-56 bg-surface-alt border-r border-edge flex flex-col">
      <div className="h-14 flex items-center px-4 border-b border-edge">
        <span className="text-lg font-semibold text-accent">PaciNet</span>
      </div>
      <nav className="flex-1 py-2">
        {navItems.map(({ to, icon: Icon, label }) => (
          <NavLink
            key={to}
            to={to}
            end={to === '/'}
            className={({ isActive }) =>
              `flex items-center gap-3 px-4 py-2.5 text-sm transition-colors ${
                isActive
                  ? 'text-accent bg-accent/10 border-r-2 border-accent'
                  : 'text-content-secondary hover:text-content hover:bg-surface-hover'
              }`
            }
          >
            <Icon size={18} />
            {label}
          </NavLink>
        ))}
      </nav>
      <div className="px-4 py-3 border-t border-edge text-xs text-content-muted">
        PaciNet SDN Controller
      </div>
    </aside>
  );
}
