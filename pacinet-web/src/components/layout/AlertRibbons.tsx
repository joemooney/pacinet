import { Link } from 'react-router-dom';
import { AlertTriangle, Info, ShieldAlert } from 'lucide-react';
import { useFleet } from '../../hooks/useFleet';
import { useFsmInstances } from '../../hooks/useFsm';

type Severity = 'critical' | 'warning' | 'info';

interface AlertRibbonsProps {
  compact: boolean;
}

interface AlertItem {
  id: string;
  severity: Severity;
  message: string;
  ctaLabel?: string;
  ctaPath?: string;
}

export default function AlertRibbons({ compact }: AlertRibbonsProps) {
  const { data: fleet } = useFleet();
  const { data: instances } = useFsmInstances();

  const nodesByState = fleet?.nodes_by_state ?? {};
  const totalNodes = fleet?.total_nodes ?? 0;
  const unhealthyNodes = (nodesByState.error ?? 0) + (nodesByState.offline ?? 0);
  const deployingNodes = nodesByState.deploying ?? 0;
  const failedInstances = (instances ?? []).filter((i) => i.status === 'failed').length;

  const alerts: AlertItem[] = [];

  if (totalNodes === 0) {
    alerts.push({
      id: 'no-nodes',
      severity: 'info',
      message: 'No nodes are currently registered. Fleet operations are idle until agents register.',
      ctaLabel: 'Open Nodes',
      ctaPath: '/nodes',
    });
  }

  if (unhealthyNodes > 0) {
    alerts.push({
      id: 'unhealthy-nodes',
      severity: 'critical',
      message: `${unhealthyNodes} node(s) are in offline or error state. Immediate operator review is recommended.`,
      ctaLabel: 'Review Nodes',
      ctaPath: '/nodes',
    });
  }

  if (failedInstances > 0) {
    alerts.push({
      id: 'failed-fsm',
      severity: 'warning',
      message: `${failedInstances} FSM instance(s) failed. Check orchestration history and retry strategy.`,
      ctaLabel: 'Review FSM',
      ctaPath: '/fsm',
    });
  }

  if (deployingNodes > 0) {
    alerts.push({
      id: 'deploying',
      severity: 'info',
      message: `${deployingNodes} node(s) are actively deploying. Monitor for completion before follow-up actions.`,
      ctaLabel: 'Open Dashboard',
      ctaPath: '/',
    });
  }

  if (alerts.length === 0) return null;

  const severityClass: Record<Severity, string> = {
    critical: 'border-red-500/40 bg-red-500/10 text-red-200',
    warning: 'border-amber-500/45 bg-amber-500/10 text-amber-100',
    info: 'border-cyan-500/35 bg-cyan-500/10 text-cyan-100',
  };

  const severityIcon: Record<Severity, React.ReactNode> = {
    critical: <ShieldAlert size={16} />,
    warning: <AlertTriangle size={16} />,
    info: <Info size={16} />,
  };

  return (
    <div className={`px-3 md:px-4 lg:px-6 ${compact ? 'pt-1.5' : 'pt-2'}`}>
      <div className="space-y-2">
        {alerts.map((alert) => (
          <div
            key={alert.id}
            className={`flex flex-wrap items-center gap-2 rounded-xl border px-3 ${compact ? 'py-1.5 text-xs' : 'py-2 text-sm'} ${severityClass[alert.severity]}`}
            role="status"
          >
            <span className="inline-flex items-center">{severityIcon[alert.severity]}</span>
            <span className="font-medium tracking-wide uppercase text-[11px]">{alert.severity}</span>
            <span className="opacity-95">{alert.message}</span>
            {alert.ctaLabel && alert.ctaPath && (
              <Link
                to={alert.ctaPath}
                className="ml-auto rounded-md border border-current/35 px-2 py-1 text-[11px] uppercase tracking-[0.08em] hover:bg-white/10"
              >
                {alert.ctaLabel}
              </Link>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
