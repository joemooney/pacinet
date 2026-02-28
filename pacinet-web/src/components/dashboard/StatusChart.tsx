import { stateDotColors } from '../../lib/utils';

interface StatusChartProps {
  nodesByState: Record<string, number>;
  total: number;
}

export default function StatusChart({ nodesByState, total }: StatusChartProps) {
  if (total === 0) {
    return (
      <div className="flex items-center justify-center h-40 text-content-muted text-sm">
        No nodes registered
      </div>
    );
  }

  // Build conic gradient
  const entries = Object.entries(nodesByState).filter(([, v]) => v > 0);
  let offset = 0;
  const stops: string[] = [];
  for (const [state, count] of entries) {
    const pct = (count / total) * 100;
    const color = stateDotColors[state] || '#64748b';
    stops.push(`${color} ${offset}% ${offset + pct}%`);
    offset += pct;
  }

  const gradient = `conic-gradient(${stops.join(', ')})`;

  return (
    <div className="flex items-center gap-6">
      <div
        className="w-28 h-28 rounded-full flex-shrink-0"
        style={{
          background: gradient,
          mask: 'radial-gradient(farthest-side, transparent 60%, black 61%)',
          WebkitMask: 'radial-gradient(farthest-side, transparent 60%, black 61%)',
        }}
      />
      <div className="flex flex-col gap-1.5">
        {entries.map(([state, count]) => (
          <div key={state} className="flex items-center gap-2 text-sm">
            <div
              className="w-2.5 h-2.5 rounded-full"
              style={{ backgroundColor: stateDotColors[state] || '#64748b' }}
            />
            <span className="text-content-secondary capitalize">{state}</span>
            <span className="text-content font-medium">{count}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
