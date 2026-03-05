import { useFleet } from '../../hooks/useFleet';
import Card from '../ui/Card';
import Spinner from '../ui/Spinner';
import StatusChart from './StatusChart';
import RecentEvents from './RecentEvents';
import FsmSummary from './FsmSummary';
import { stateDotColors } from '../../lib/utils';

const metricStates = ['online', 'deploying', 'active', 'error', 'offline'] as const;

export default function DashboardPage() {
  const { data: fleet, isLoading } = useFleet();

  if (isLoading) return <Spinner />;

  const nodesByState = fleet?.nodes_by_state || {};
  const total = fleet?.total_nodes || 0;
  const online = nodesByState.online || 0;
  const active = nodesByState.active || 0;
  const error = nodesByState.error || 0;
  const healthyRatio = total > 0 ? Math.round(((online + active) / total) * 100) : 0;
  const healthScore = Math.max(0, healthyRatio - error * 4);

  return (
    <div className="space-y-6 animate-fade-in">
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <div className="lg:col-span-2 rounded-2xl border border-edge/80 bg-[linear-gradient(135deg,rgba(20,167,189,0.22),rgba(0,0,0,0))] p-5 md:p-6">
          <div className="text-[11px] uppercase tracking-[0.18em] text-content-muted">Fleet Operations</div>
          <h2 className="mt-2 text-2xl md:text-3xl font-semibold tracking-tight">Control Center Overview</h2>
          <p className="mt-2 text-sm md:text-base text-content-secondary">
            Real-time visibility across node health, rollout state, and orchestration activity.
          </p>
        </div>
        <div className="rounded-2xl border border-edge/80 bg-surface-alt/90 p-5">
          <div className="text-[11px] uppercase tracking-[0.14em] text-content-muted">Health Score</div>
          <div className="mt-2 text-4xl font-semibold tracking-tight">{healthScore}</div>
          <div className="mt-2 h-2 w-full rounded-full bg-surface-hover">
            <div
              className="h-2 rounded-full bg-[linear-gradient(90deg,#16b6b7,#20d790)] transition-all"
              style={{ width: `${Math.min(100, Math.max(0, healthScore))}%` }}
            />
          </div>
          <div className="mt-2 text-xs text-content-muted">{healthyRatio}% of fleet online/active</div>
        </div>
      </div>

      {/* Metric cards */}
      <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
        <MetricCard label="Total Nodes" value={total} color="#e2e8f0" />
        {metricStates.map((state) => (
          <MetricCard
            key={state}
            label={state.charAt(0).toUpperCase() + state.slice(1)}
            value={nodesByState[state] || 0}
            color={stateDotColors[state]}
          />
        ))}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <Card title="Node Distribution" className="lg:col-span-1">
          <StatusChart nodesByState={nodesByState} total={total} />
        </Card>
        <Card title="Recent Events" className="lg:col-span-1">
          <RecentEvents />
        </Card>
        <Card title="FSM Instances" className="lg:col-span-1">
          <FsmSummary />
        </Card>
      </div>
    </div>
  );
}

function MetricCard({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <div className="rounded-2xl border border-edge/80 bg-surface-alt/90 p-4 shadow-[0_8px_24px_rgba(0,0,0,0.14)]">
      <div className="flex items-center gap-2 mb-1">
        <div className="w-2 h-2 rounded-full" style={{ backgroundColor: color }} />
        <span className="text-[11px] text-content-muted uppercase tracking-[0.14em]">{label}</span>
      </div>
      <div className="text-3xl font-semibold tracking-tight">{value}</div>
    </div>
  );
}
