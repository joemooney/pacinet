import { useFsmInstances } from '../../hooks/useFsm';
import { shortId, statusColorClass } from '../../lib/utils';
import Badge from '../ui/Badge';
import Spinner from '../ui/Spinner';

export default function FsmSummary() {
  const { data: instances, isLoading } = useFsmInstances();

  if (isLoading) return <Spinner />;

  const running = instances?.filter((i) => i.status === 'running') || [];
  const recent = instances?.slice(0, 5) || [];

  return (
    <div>
      <div className="flex items-end justify-between">
        <div>
          <div className="text-3xl font-semibold tracking-tight">{running.length}</div>
          <div className="text-sm text-content-muted">Running instances</div>
        </div>
        <Badge className="bg-emerald-500/20 text-emerald-400">{instances?.length ?? 0} total</Badge>
      </div>
      {recent.length > 0 && (
        <div className="space-y-2 mt-4">
          {recent.map((inst) => (
            <div key={inst.instance_id} className="flex items-center gap-2 text-sm rounded-lg px-2 py-1.5 hover:bg-surface-hover/70 transition-colors">
              <Badge className={statusColorClass(inst.status)}>{inst.status}</Badge>
              <span className="text-content-secondary">{inst.definition_name}</span>
              <span className="text-content-muted">({shortId(inst.instance_id)})</span>
              <span className="text-content-muted ml-auto">{inst.current_state}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
