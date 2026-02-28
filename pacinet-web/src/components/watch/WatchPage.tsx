import { useState, useRef, useEffect } from 'react';
import { useNodeEvents, useCounterEvents, useFsmEvents } from '../../hooks/useEvents';
import { formatTimestamp } from '../../lib/utils';
import Badge from '../ui/Badge';

type EventType = 'nodes' | 'counters' | 'fsm';
type UnifiedEvent = {
  type: EventType;
  timestamp: string;
  summary: string;
  detail?: string;
};

export default function WatchPage() {
  const [filters, setFilters] = useState<Record<EventType, boolean>>({
    nodes: true,
    counters: true,
    fsm: true,
  });
  const [textFilter, setTextFilter] = useState('');
  const [paused, setPaused] = useState(false);
  const feedRef = useRef<HTMLDivElement>(null);

  const nodeEvents = useNodeEvents();
  const counterEvents = useCounterEvents();
  const fsmEvents = useFsmEvents();

  // Combine and sort events
  const unified: UnifiedEvent[] = [];

  if (filters.nodes) {
    for (const e of nodeEvents) {
      unified.push({
        type: 'nodes',
        timestamp: e.timestamp,
        summary: `[NODE] ${e.hostname}: ${e.event_type}`,
        detail: e.new_state ? `-> ${e.new_state}` : undefined,
      });
    }
  }

  if (filters.counters) {
    for (const e of counterEvents) {
      const ruleCount = e.counters.length;
      unified.push({
        type: 'counters',
        timestamp: e.collected_at,
        summary: `[COUNTER] ${e.node_id.slice(0, 8)}: ${ruleCount} rules reported`,
      });
    }
  }

  if (filters.fsm) {
    for (const e of fsmEvents) {
      unified.push({
        type: 'fsm',
        timestamp: e.timestamp,
        summary: `[FSM] ${e.definition_name}: ${e.event_type}`,
        detail: e.to_state ? `-> ${e.to_state}` : e.final_status || undefined,
      });
    }
  }

  // Sort by timestamp descending
  unified.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());

  // Apply text filter
  const filtered = textFilter
    ? unified.filter(
        (e) =>
          e.summary.toLowerCase().includes(textFilter.toLowerCase()) ||
          e.detail?.toLowerCase().includes(textFilter.toLowerCase())
      )
    : unified;

  // Auto-scroll
  useEffect(() => {
    if (!paused && feedRef.current) {
      feedRef.current.scrollTop = 0;
    }
  }, [filtered.length, paused]);

  const typeColors: Record<EventType, string> = {
    nodes: 'bg-blue-500/20 text-blue-400',
    counters: 'bg-emerald-500/20 text-emerald-400',
    fsm: 'bg-purple-500/20 text-purple-400',
  };

  return (
    <div className="h-full flex flex-col animate-fade-in">
      {/* Filters */}
      <div className="flex items-center gap-4 mb-4 flex-shrink-0">
        {(['nodes', 'counters', 'fsm'] as EventType[]).map((type) => (
          <label key={type} className="flex items-center gap-2 text-sm text-content-secondary">
            <input
              type="checkbox"
              checked={filters[type]}
              onChange={(e) => setFilters((f) => ({ ...f, [type]: e.target.checked }))}
              className="accent-accent"
            />
            {type.charAt(0).toUpperCase() + type.slice(1)}
          </label>
        ))}
        <input
          type="text"
          value={textFilter}
          onChange={(e) => setTextFilter(e.target.value)}
          placeholder="Filter events..."
          className="px-3 py-1.5 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent w-48"
        />
        <span className="text-xs text-content-muted ml-auto">{filtered.length} events</span>
      </div>

      {/* Event feed */}
      <div
        ref={feedRef}
        className="flex-1 overflow-y-auto bg-surface-alt border border-edge rounded-xl"
        onMouseEnter={() => setPaused(true)}
        onMouseLeave={() => setPaused(false)}
      >
        {filtered.length === 0 ? (
          <div className="text-sm text-content-muted py-8 text-center">
            No events. Events appear here in real time via SSE.
          </div>
        ) : (
          <div className="divide-y divide-edge">
            {filtered.map((e, i) => (
              <div key={i} className="flex items-start gap-3 px-4 py-2.5 text-sm">
                <Badge className={typeColors[e.type]}>{e.type}</Badge>
                <span className="flex-1">
                  {e.summary}
                  {e.detail && <span className="text-content-muted ml-2">{e.detail}</span>}
                </span>
                <span className="text-xs text-content-muted whitespace-nowrap">
                  {formatTimestamp(e.timestamp)}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {paused && (
        <div className="text-center text-xs text-content-muted mt-2">
          Auto-scroll paused (hover)
        </div>
      )}
    </div>
  );
}
