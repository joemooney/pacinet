import { useState, useRef, useEffect } from 'react';
import { useNodeEvents, useCounterEvents, useFsmEvents, useEventHistory } from '../../hooks/useEvents';
import { useFilterPresets } from '../../hooks/useFilterPresets';
import { formatTimestamp } from '../../lib/utils';
import Badge from '../ui/Badge';
import FilterPresetManager from '../ui/FilterPresetManager';

type EventType = 'nodes' | 'counters' | 'fsm';
type UnifiedEvent = {
  type: EventType;
  timestamp: string;
  summary: string;
  detail?: string;
};

type Tab = 'live' | 'history';

interface WatchPreset {
  tab: Tab;
  filters: Record<EventType, boolean>;
  textFilter: string;
  historyType: string;
  historySource: string;
  historyLimit: number;
}

export default function WatchPage() {
  const [tab, setTab] = useState<Tab>('live');
  const [filters, setFilters] = useState<Record<EventType, boolean>>({
    nodes: true,
    counters: true,
    fsm: true,
  });
  const [textFilter, setTextFilter] = useState('');
  const [paused, setPaused] = useState(false);
  const feedRef = useRef<HTMLDivElement>(null);

  const [historyType, setHistoryType] = useState('');
  const [historySource, setHistorySource] = useState('');
  const [historyLimit, setHistoryLimit] = useState(50);

  const presets = useFilterPresets<WatchPreset>(
    'pacinet_filters_watch',
    { tab, filters, textFilter, historyType, historySource, historyLimit },
    (preset) => {
      setTab(preset.tab === 'history' ? 'history' : 'live');
      setFilters({
        nodes: Boolean(preset.filters?.nodes),
        counters: Boolean(preset.filters?.counters),
        fsm: Boolean(preset.filters?.fsm),
      });
      setTextFilter(preset.textFilter ?? '');
      setHistoryType(preset.historyType ?? '');
      setHistorySource(preset.historySource ?? '');
      setHistoryLimit([25, 50, 100, 200].includes(preset.historyLimit) ? preset.historyLimit : 50);
    }
  );

  const nodeEvents = useNodeEvents();
  const counterEvents = useCounterEvents();
  const fsmEvents = useFsmEvents();
  const { events: historyEvents, loading: historyLoading, refetch } = useEventHistory({
    type: historyType || undefined,
    source: historySource || undefined,
    limit: historyLimit,
  });

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

  unified.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());

  const filtered = textFilter
    ? unified.filter(
        (e) =>
          e.summary.toLowerCase().includes(textFilter.toLowerCase()) ||
          e.detail?.toLowerCase().includes(textFilter.toLowerCase())
      )
    : unified;

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

  const eventTypeColor = (et: string): string => {
    if (et.startsWith('node')) return typeColors.nodes;
    if (et.startsWith('counter')) return typeColors.counters;
    if (et.startsWith('fsm')) return typeColors.fsm;
    return 'bg-gray-500/20 text-gray-400';
  };

  return (
    <div className="h-full flex flex-col animate-fade-in">
      <div className="flex flex-col gap-2 mb-4">
        <div className="flex items-center gap-2">
          <button
            onClick={() => setTab('live')}
            className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
              tab === 'live' ? 'bg-accent text-white' : 'text-content-secondary hover:text-content hover:bg-surface-hover'
            }`}
          >
            Live
          </button>
          <button
            onClick={() => setTab('history')}
            className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
              tab === 'history' ? 'bg-accent text-white' : 'text-content-secondary hover:text-content hover:bg-surface-hover'
            }`}
          >
            History
          </button>
        </div>

        <FilterPresetManager manager={presets} exportFilePrefix="pacinet-watch-filters" />
      </div>

      {tab === 'live' ? (
        <>
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
        </>
      ) : (
        <>
          <div className="flex items-center gap-4 mb-4 flex-shrink-0">
            <div>
              <label className="block text-xs text-content-muted mb-1">Event Type</label>
              <select
                value={historyType}
                onChange={(e) => setHistoryType(e.target.value)}
                className="px-3 py-1.5 bg-surface border border-edge rounded-lg text-sm text-content focus:outline-none focus:border-accent"
              >
                <option value="">All</option>
                <option value="node">Node</option>
                <option value="fsm">FSM</option>
                <option value="counter">Counter</option>
              </select>
            </div>
            <div>
              <label className="block text-xs text-content-muted mb-1">Source</label>
              <input
                type="text"
                value={historySource}
                onChange={(e) => setHistorySource(e.target.value)}
                placeholder="node ID or instance ID"
                className="px-3 py-1.5 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent w-48"
              />
            </div>
            <div>
              <label className="block text-xs text-content-muted mb-1">Limit</label>
              <select
                value={historyLimit}
                onChange={(e) => setHistoryLimit(Number(e.target.value))}
                className="px-3 py-1.5 bg-surface border border-edge rounded-lg text-sm text-content focus:outline-none focus:border-accent"
              >
                <option value={25}>25</option>
                <option value={50}>50</option>
                <option value={100}>100</option>
                <option value={200}>200</option>
              </select>
            </div>
            <button
              onClick={refetch}
              className="mt-4 px-3 py-1.5 text-sm bg-accent text-white rounded-lg hover:bg-accent/90 transition-colors"
            >
              Refresh
            </button>
            <span className="text-xs text-content-muted ml-auto mt-4">{historyEvents.length} events</span>
          </div>

          <div className="flex-1 overflow-y-auto bg-surface-alt border border-edge rounded-xl">
            {historyLoading ? (
              <div className="text-sm text-content-muted py-8 text-center">Loading...</div>
            ) : historyEvents.length === 0 ? (
              <div className="text-sm text-content-muted py-8 text-center">
                No historical events found.
              </div>
            ) : (
              <div className="divide-y divide-edge">
                {historyEvents.map((e) => (
                  <div key={e.id} className="flex items-start gap-3 px-4 py-2.5 text-sm">
                    <Badge className={eventTypeColor(e.event_type)}>{e.event_type}</Badge>
                    <span className="flex-1 font-mono text-xs truncate" title={e.payload}>
                      {e.source.slice(0, 8)}
                    </span>
                    <span className="text-xs text-content-muted whitespace-nowrap">
                      {formatTimestamp(e.timestamp)}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
}
