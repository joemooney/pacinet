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
      <div className="text-2xl font-semibold mb-3">{running.length}</div>
      <div className="text-sm text-content-muted mb-4">Running instances</div>
      {recent.length > 0 && (
        <div className="space-y-2">
          {recent.map((inst) => (
            <div key={inst.instance_id} className="flex items-center gap-2 text-sm">
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
