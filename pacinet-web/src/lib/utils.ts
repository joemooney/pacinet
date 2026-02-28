export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`;
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}

export function formatAge(seconds: number): string {
  if (seconds < 5) return 'just now';
  if (seconds < 60) return `${Math.round(seconds)}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  return `${Math.floor(seconds / 3600)}h ago`;
}

export function formatTimestamp(iso: string): string {
  return new Date(iso).toLocaleString();
}

export function shortId(id: string): string {
  return id.slice(0, 8);
}

export const stateColors: Record<string, string> = {
  registered: 'gray',
  online: 'emerald',
  deploying: 'amber',
  active: 'blue',
  error: 'red',
  offline: 'slate',
};

export function stateColorClass(state: string): string {
  const color = stateColors[state] || 'gray';
  const map: Record<string, string> = {
    gray: 'bg-gray-500/20 text-gray-400',
    emerald: 'bg-emerald-500/20 text-emerald-400',
    amber: 'bg-amber-500/20 text-amber-400',
    blue: 'bg-blue-500/20 text-blue-400',
    red: 'bg-red-500/20 text-red-400',
    slate: 'bg-slate-500/20 text-slate-400',
  };
  return map[color] || map.gray;
}

export function statusColorClass(status: string): string {
  const map: Record<string, string> = {
    running: 'bg-blue-500/20 text-blue-400',
    completed: 'bg-emerald-500/20 text-emerald-400',
    failed: 'bg-red-500/20 text-red-400',
    cancelled: 'bg-slate-500/20 text-slate-400',
    success: 'bg-emerald-500/20 text-emerald-400',
    agent_failure: 'bg-red-500/20 text-red-400',
    agent_unreachable: 'bg-amber-500/20 text-amber-400',
    timeout: 'bg-amber-500/20 text-amber-400',
  };
  return map[status] || 'bg-gray-500/20 text-gray-400';
}

export const stateDotColors: Record<string, string> = {
  registered: '#9ca3af',
  online: '#10b981',
  deploying: '#f59e0b',
  active: '#3b82f6',
  error: '#ef4444',
  offline: '#64748b',
};
