import { useState } from 'react';
import { useFsmInstances, useAdvanceFsm, useCancelFsm } from '../../hooks/useFsm';
import { shortId, statusColorClass, formatTimestamp } from '../../lib/utils';
import Table from '../ui/Table';
import Badge from '../ui/Badge';
import Button from '../ui/Button';
import Spinner from '../ui/Spinner';
import InstanceDetail from './InstanceDetail';

export default function InstanceList() {
  const [selectedInstance, setSelectedInstance] = useState<string | null>(null);
  const { data: instances, isLoading } = useFsmInstances();
  const advanceFsm = useAdvanceFsm();
  const cancelFsm = useCancelFsm();

  if (isLoading) return <Spinner />;

  return (
    <div className="space-y-4">
      <span className="text-sm text-content-muted">{instances?.length || 0} instances</span>

      {!instances || instances.length === 0 ? (
        <div className="text-sm text-content-muted py-4 text-center">No FSM instances</div>
      ) : (
        <div className="bg-surface-alt border border-edge rounded-xl overflow-hidden">
          <Table headers={['ID', 'Definition', 'State', 'Status', 'Progress', 'Updated', '']}>
            {instances.map((inst) => (
              <tr
                key={inst.instance_id}
                className="hover:bg-surface-hover cursor-pointer"
                onClick={() => setSelectedInstance(inst.instance_id)}
              >
                <td className="px-4 py-3 font-mono text-xs">{shortId(inst.instance_id)}</td>
                <td className="px-4 py-3 font-medium">{inst.definition_name}</td>
                <td className="px-4 py-3 font-mono text-xs">{inst.current_state}</td>
                <td className="px-4 py-3">
                  <Badge className={statusColorClass(inst.status)}>{inst.status}</Badge>
                </td>
                <td className="px-4 py-3 text-xs text-content-secondary">
                  {inst.target_nodes > 0
                    ? `${inst.deployed_nodes}/${inst.target_nodes} deployed`
                    : '-'}
                </td>
                <td className="px-4 py-3 text-xs text-content-muted">{formatTimestamp(inst.updated_at)}</td>
                <td className="px-4 py-3">
                  {inst.status === 'running' && (
                    <div className="flex gap-1" onClick={(e) => e.stopPropagation()}>
                      <Button
                        variant="secondary"
                        size="sm"
                        onClick={() => advanceFsm.mutate({ id: inst.instance_id })}
                        disabled={advanceFsm.isPending}
                      >
                        Advance
                      </Button>
                      <Button
                        variant="danger"
                        size="sm"
                        onClick={() => cancelFsm.mutate({ id: inst.instance_id, reason: 'Manual cancel' })}
                        disabled={cancelFsm.isPending}
                      >
                        Cancel
                      </Button>
                    </div>
                  )}
                </td>
              </tr>
            ))}
          </Table>
        </div>
      )}

      {selectedInstance && (
        <InstanceDetail instanceId={selectedInstance} onClose={() => setSelectedInstance(null)} />
      )}
    </div>
  );
}
