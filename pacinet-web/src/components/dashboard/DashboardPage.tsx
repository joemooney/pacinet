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

  return (
    <div className="space-y-6 animate-fade-in">
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
    <div className="bg-surface-alt border border-edge rounded-xl p-4">
      <div className="flex items-center gap-2 mb-1">
        <div className="w-2 h-2 rounded-full" style={{ backgroundColor: color }} />
        <span className="text-xs text-content-muted uppercase tracking-wider">{label}</span>
      </div>
      <div className="text-2xl font-semibold">{value}</div>
    </div>
  );
}
