import { useNodeEvents, useFsmEvents } from '../../hooks/useEvents';
import { formatTimestamp } from '../../lib/utils';
import Badge from '../ui/Badge';

export default function RecentEvents() {
  const nodeEvents = useNodeEvents();
  const fsmEvents = useFsmEvents();

  type UnifiedEvent = { type: 'node' | 'fsm'; timestamp: string; summary: string; badge: string };

  const unified: UnifiedEvent[] = [
    ...nodeEvents.map((e) => ({
      type: 'node' as const,
      timestamp: e.timestamp,
      summary: `${e.hostname}: ${e.event_type}${e.new_state ? ` -> ${e.new_state}` : ''}`,
      badge: e.event_type,
    })),
    ...fsmEvents.map((e) => ({
      type: 'fsm' as const,
      timestamp: e.timestamp,
      summary: `${e.definition_name}: ${e.event_type}${e.to_state ? ` -> ${e.to_state}` : ''}`,
      badge: e.event_type,
    })),
  ]
    .sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime())
    .slice(0, 20);

  if (unified.length === 0) {
    return <div className="text-sm text-content-muted py-4">No events yet. Live telemetry appears here when node/FSM streams are active.</div>;
  }

  return (
    <div className="space-y-2 max-h-80 overflow-y-auto pr-1">
      {unified.map((e, i) => (
        <div key={i} className="flex items-start gap-3 text-sm py-2 px-2 rounded-lg hover:bg-surface-hover/70 transition-colors">
          <Badge className={e.type === 'node' ? 'bg-cyan-500/20 text-cyan-400' : 'bg-amber-500/20 text-amber-400'}>
            {e.type}
          </Badge>
          <span className="text-content flex-1">{e.summary}</span>
          <span className="text-content-muted text-xs whitespace-nowrap">{formatTimestamp(e.timestamp)}</span>
        </div>
      ))}
    </div>
  );
}
