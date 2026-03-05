import { useEffect, useMemo, useState } from 'react';
import { Link } from 'react-router-dom';
import { Activity, ArrowLeft, Pause, Play, Server, Workflow, Zap } from 'lucide-react';
import { useFleet } from '../../hooks/useFleet';
import { useFsmInstances } from '../../hooks/useFsm';
import { useNodeEvents, useCounterEvents, useFsmEvents } from '../../hooks/useEvents';
import { formatTimestamp } from '../../lib/utils';

type ViewMode = 'overview' | 'events' | 'orchestration';

const VIEW_ORDER: ViewMode[] = ['overview', 'events', 'orchestration'];
const ROTATE_MS = 12000;

function StatTile({
  label,
  value,
  note,
  icon,
}: {
  label: string;
  value: string | number;
  note: string;
  icon: React.ReactNode;
}) {
  return (
    <div className="rounded-2xl border border-edge/80 bg-surface/80 p-5 shadow-sm">
      <div className="flex items-center justify-between">
        <div className="text-xs uppercase tracking-[0.14em] text-content-muted">{label}</div>
        <div className="text-content-secondary">{icon}</div>
      </div>
      <div className="mt-3 text-3xl font-semibold tracking-tight text-content">{value}</div>
      <div className="mt-1 text-sm text-content-secondary">{note}</div>
    </div>
  );
}

export default function WallboardPage() {
  const [viewIndex, setViewIndex] = useState(0);
  const [autoRotate, setAutoRotate] = useState(true);
  const currentView = VIEW_ORDER[viewIndex] || 'overview';

  const { data: fleet } = useFleet();
  const { data: instances } = useFsmInstances();
  const nodeEvents = useNodeEvents();
  const counterEvents = useCounterEvents();
  const fsmEvents = useFsmEvents();

  useEffect(() => {
    if (!autoRotate) return;
    const timer = window.setInterval(() => {
      setViewIndex((prev) => (prev + 1) % VIEW_ORDER.length);
    }, ROTATE_MS);
    return () => window.clearInterval(timer);
  }, [autoRotate]);
  const nodesByState = fleet?.nodes_by_state ?? {};
  const totalNodes = fleet?.total_nodes ?? 0;
  const onlineNodes = (nodesByState.online ?? 0) + (nodesByState.active ?? 0);
  const unhealthyNodes = (nodesByState.error ?? 0) + (nodesByState.offline ?? 0);
  const coverage = totalNodes > 0 ? Math.round((onlineNodes / totalNodes) * 100) : 0;

  const fsmStatusCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const inst of instances ?? []) {
      counts[inst.status] = (counts[inst.status] ?? 0) + 1;
    }
    return counts;
  }, [instances]);

  const recentStream = useMemo(() => {
    const combined = [
      ...nodeEvents.map((e) => ({
        ts: e.timestamp,
        kind: 'node',
        summary: `${e.hostname}: ${e.event_type}${e.new_state ? ` -> ${e.new_state}` : ''}`,
      })),
      ...counterEvents.map((e) => ({
        ts: e.collected_at,
        kind: 'counter',
        summary: `${e.node_id.slice(0, 8)}: ${e.counters.length} counter groups collected`,
      })),
      ...fsmEvents.map((e) => ({
        ts: e.timestamp,
        kind: 'fsm',
        summary: `${e.definition_name}: ${e.event_type}${e.to_state ? ` -> ${e.to_state}` : ''}`,
      })),
    ];

    return combined
      .sort((a, b) => new Date(b.ts).getTime() - new Date(a.ts).getTime())
      .slice(0, 14);
  }, [nodeEvents, counterEvents, fsmEvents]);

  return (
    <div className="min-h-screen bg-[radial-gradient(circle_at_20%_10%,rgba(31,169,166,0.22),transparent_42%),radial-gradient(circle_at_84%_18%,rgba(26,86,219,0.2),transparent_36%),linear-gradient(140deg,var(--color-bg),#050c10_60%)] text-content">
      <div className="mx-auto max-w-[1700px] px-4 py-5 sm:px-6 lg:px-8">
        <div className="mb-5 flex flex-wrap items-center gap-3 rounded-2xl border border-edge/80 bg-surface-alt/80 px-4 py-3 backdrop-blur-sm">
          <Link
            to="/"
            className="inline-flex items-center gap-2 rounded-lg border border-edge bg-surface px-3 py-1.5 text-sm text-content-secondary hover:text-content"
          >
            <ArrowLeft size={14} />
            Exit Wallboard
          </Link>
          <div>
            <div className="text-[11px] uppercase tracking-[0.16em] text-content-muted">NOC Wallboard</div>
            <div className="text-lg font-semibold">PaciNet Fleet Operations</div>
          </div>
          <div className="ml-auto flex items-center gap-2">
            {VIEW_ORDER.map((mode, idx) => (
              <button
                key={mode}
                onClick={() => setViewIndex(idx)}
                className={`rounded-lg px-3 py-1.5 text-sm transition-colors ${
                  currentView === mode
                    ? 'bg-accent text-white'
                    : 'bg-surface text-content-secondary hover:text-content'
                }`}
              >
                {mode}
              </button>
            ))}
            <button
              onClick={() => setAutoRotate((v) => !v)}
              className="inline-flex items-center gap-2 rounded-lg border border-edge bg-surface px-3 py-1.5 text-sm text-content-secondary hover:text-content"
            >
              {autoRotate ? <Pause size={14} /> : <Play size={14} />}
              {autoRotate ? 'Pause rotate' : 'Resume rotate'}
            </button>
          </div>
        </div>

        <div className="mb-5 grid gap-4 md:grid-cols-2 xl:grid-cols-4">
          <StatTile
            label="Fleet Size"
            value={totalNodes}
            note={`${onlineNodes} healthy / ${unhealthyNodes} degraded`}
            icon={<Server size={16} />}
          />
          <StatTile
            label="Coverage"
            value={`${coverage}%`}
            note="Nodes online or active"
            icon={<Activity size={16} />}
          />
          <StatTile
            label="FSM Running"
            value={fsmStatusCounts.running ?? 0}
            note={`${instances?.length ?? 0} total instances`}
            icon={<Workflow size={16} />}
          />
          <StatTile
            label="Live Throughput"
            value={counterEvents.length}
            note="Recent counter event packets"
            icon={<Zap size={16} />}
          />
        </div>

        <div className="grid gap-4 xl:grid-cols-5">
          {(currentView === 'overview' || currentView === 'orchestration') && (
            <div className="rounded-2xl border border-edge/80 bg-surface-alt/70 p-4 xl:col-span-3">
              <div className="mb-3 text-sm font-medium text-content">Fleet State Distribution</div>
              <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                {Object.entries(nodesByState).length === 0 ? (
                  <div className="text-sm text-content-muted">No fleet data yet.</div>
                ) : (
                  Object.entries(nodesByState).map(([state, count]) => {
                    const pct = totalNodes > 0 ? Math.round((count / totalNodes) * 100) : 0;
                    return (
                      <div key={state} className="rounded-xl border border-edge bg-surface p-3">
                        <div className="mb-1 flex items-center justify-between text-sm">
                          <span className="capitalize text-content-secondary">{state}</span>
                          <span className="font-medium text-content">{count}</span>
                        </div>
                        <div className="h-2 rounded-full bg-surface-alt">
                          <div className="h-full rounded-full bg-accent" style={{ width: `${pct}%` }} />
                        </div>
                        <div className="mt-1 text-xs text-content-muted">{pct}% of fleet</div>
                      </div>
                    );
                  })
                )}
              </div>
            </div>
          )}

          {(currentView === 'overview' || currentView === 'events') && (
            <div className="rounded-2xl border border-edge/80 bg-surface-alt/70 p-4 xl:col-span-2">
              <div className="mb-3 flex items-center justify-between">
                <div className="text-sm font-medium text-content">Live Event Stream</div>
                <span className="text-xs text-content-muted">{recentStream.length} recent</span>
              </div>
              <div className="max-h-[48vh] overflow-y-auto rounded-xl border border-edge bg-surface">
                {recentStream.length === 0 ? (
                  <div className="px-4 py-8 text-center text-sm text-content-muted">Awaiting events...</div>
                ) : (
                  <div className="divide-y divide-edge">
                    {recentStream.map((event, idx) => (
                      <div key={`${event.kind}-${idx}`} className="flex items-start gap-3 px-4 py-3 text-sm">
                        <span className="mt-0.5 inline-flex rounded-full bg-accent/15 px-2 py-0.5 text-[11px] uppercase tracking-[0.1em] text-accent">
                          {event.kind}
                        </span>
                        <div className="min-w-0 flex-1">
                          <div className="truncate text-content">{event.summary}</div>
                          <div className="text-xs text-content-muted">{formatTimestamp(event.ts)}</div>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}

          {currentView === 'orchestration' && (
            <div className="rounded-2xl border border-edge/80 bg-surface-alt/70 p-4 xl:col-span-2">
              <div className="mb-3 text-sm font-medium text-content">FSM Status</div>
              <div className="grid gap-3 sm:grid-cols-2">
                {['running', 'completed', 'failed', 'cancelled'].map((status) => (
                  <div key={status} className="rounded-xl border border-edge bg-surface p-3">
                    <div className="text-xs uppercase tracking-[0.1em] text-content-muted">{status}</div>
                    <div className="mt-1 text-2xl font-semibold">{fsmStatusCounts[status] ?? 0}</div>
                  </div>
                ))}
              </div>

              <div className="mt-4 rounded-xl border border-edge bg-surface p-3">
                <div className="mb-2 text-xs uppercase tracking-[0.1em] text-content-muted">Active Instances</div>
                <div className="max-h-60 space-y-2 overflow-y-auto">
                  {(instances ?? []).filter((inst) => inst.status === 'running').slice(0, 10).map((inst) => (
                    <div key={inst.instance_id} className="rounded-lg border border-edge px-3 py-2 text-sm">
                      <div className="font-medium text-content">{inst.definition_name}</div>
                      <div className="text-xs text-content-secondary">
                        {inst.current_state} • {inst.deployed_nodes}/{inst.target_nodes} deployed
                      </div>
                    </div>
                  ))}
                  {(instances ?? []).filter((inst) => inst.status === 'running').length === 0 && (
                    <div className="text-sm text-content-muted">No running instances.</div>
                  )}
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
