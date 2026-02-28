import { useMemo } from 'react';
import { LineChart, Line, XAxis, YAxis, Tooltip, Legend, ResponsiveContainer } from 'recharts';
import type { CounterEventJson } from '../../types/api';

interface CounterRateChartProps {
  events: CounterEventJson[];
}

const COLORS = ['#3b82f6', '#10b981', '#f59e0b', '#ef4444', '#8b5cf6', '#ec4899', '#06b6d4', '#84cc16'];

export default function CounterRateChart({ events }: CounterRateChartProps) {
  const { data, ruleNames } = useMemo(() => {
    const names = new Set<string>();
    // Events arrive newest-first; reverse for chronological chart
    const chronological = [...events].reverse().slice(-100);

    const points = chronological.map((e) => {
      const point: Record<string, number | string> = {
        time: new Date(e.collected_at).toLocaleTimeString(),
      };
      for (const c of e.counters) {
        names.add(c.rule_name);
        point[c.rule_name] = Math.round(c.matches_per_second * 10) / 10;
      }
      return point;
    });

    return { data: points, ruleNames: Array.from(names) };
  }, [events]);

  if (data.length < 2) {
    return (
      <div className="text-sm text-content-muted py-4 text-center">
        Waiting for counter data...
      </div>
    );
  }

  return (
    <div className="h-48">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
          <XAxis dataKey="time" tick={{ fontSize: 10, fill: '#94a3b8' }} />
          <YAxis tick={{ fontSize: 10, fill: '#94a3b8' }} />
          <Tooltip
            contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px', fontSize: 12 }}
            labelStyle={{ color: '#94a3b8' }}
          />
          <Legend wrapperStyle={{ fontSize: 11 }} />
          {ruleNames.map((name, i) => (
            <Line
              key={name}
              type="monotone"
              dataKey={name}
              stroke={COLORS[i % COLORS.length]}
              strokeWidth={1.5}
              dot={false}
              isAnimationActive={false}
            />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
