import { useMemo } from 'react';
import { PieChart, Pie, Cell, Tooltip, ResponsiveContainer } from 'recharts';
import { stateDotColors } from '../../lib/utils';

interface StatusChartProps {
  nodesByState: Record<string, number>;
  total: number;
}

export default function StatusChart({ nodesByState, total }: StatusChartProps) {
  const data = useMemo(
    () =>
      Object.entries(nodesByState)
        .filter(([, v]) => v > 0)
        .map(([state, count]) => ({
          name: state,
          value: count,
          color: stateDotColors[state] || '#64748b',
        })),
    [nodesByState]
  );

  if (total === 0) {
    return (
      <div className="flex items-center justify-center h-40 text-content-muted text-sm">
        No nodes registered
      </div>
    );
  }

  return (
    <div className="flex items-center gap-6">
      <div className="w-28 h-28 flex-shrink-0">
        <ResponsiveContainer width="100%" height="100%">
          <PieChart>
            <Pie
              data={data}
              cx="50%"
              cy="50%"
              innerRadius={28}
              outerRadius={52}
              dataKey="value"
              isAnimationActive={false}
            >
              {data.map((entry) => (
                <Cell key={entry.name} fill={entry.color} />
              ))}
            </Pie>
            <Tooltip
              contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px', fontSize: 12 }}
              formatter={(value, name) => [`${value} node${value !== 1 ? 's' : ''}`, String(name)]}
            />
          </PieChart>
        </ResponsiveContainer>
      </div>
      <div className="flex flex-col gap-1.5">
        {data.map((entry) => (
          <div key={entry.name} className="flex items-center gap-2 text-sm">
            <div
              className="w-2.5 h-2.5 rounded-full"
              style={{ backgroundColor: entry.color }}
            />
            <span className="text-content-secondary capitalize">{entry.name}</span>
            <span className="text-content font-medium">{entry.value}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
