import { useState } from 'react';
import { useFsmInstances, useAdvanceFsm, useCancelFsm } from '../../hooks/useFsm';
import { useFilterPresets } from '../../hooks/useFilterPresets';
import { shortId, statusColorClass, formatTimestamp } from '../../lib/utils';
import Table from '../ui/Table';
import Badge from '../ui/Badge';
import Button from '../ui/Button';
import FilterPresetManager from '../ui/FilterPresetManager';
import Spinner from '../ui/Spinner';
import InstanceDetail from './InstanceDetail';

interface FsmPreset {
  definitionFilter: string;
  statusFilter: string;
}

export default function InstanceList() {
  const [selectedInstance, setSelectedInstance] = useState<string | null>(null);
  const [statusFilter, setStatusFilter] = useState('');
  const [definitionFilter, setDefinitionFilter] = useState('');
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const { data: instances, isLoading } = useFsmInstances(definitionFilter || undefined, statusFilter || undefined);
  const advanceFsm = useAdvanceFsm();
  const cancelFsm = useCancelFsm();

  const presets = useFilterPresets<FsmPreset>(
    'pacinet_filters_fsm_instances',
    { definitionFilter, statusFilter },
    (preset) => {
      setDefinitionFilter(preset.definitionFilter ?? '');
      setStatusFilter(preset.statusFilter ?? '');
    }
  );

  if (isLoading) return <Spinner />;

  return (
    <div className="space-y-4">
      <div className="rounded-xl border border-edge bg-surface p-3 flex flex-col gap-3 md:flex-row md:items-center">
        <span className="text-sm text-content-muted">{instances?.length || 0} instances</span>
        <input
          value={definitionFilter}
          onChange={(e) => setDefinitionFilter(e.target.value)}
          placeholder="Filter by definition..."
          className="px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent"
        />
        <select
          value={statusFilter}
          onChange={(e) => setStatusFilter(e.target.value)}
          className="px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm text-content focus:outline-none focus:border-accent"
        >
          <option value="">All statuses</option>
          <option value="running">Running</option>
          <option value="completed">Completed</option>
          <option value="failed">Failed</option>
          <option value="cancelled">Cancelled</option>
        </select>
        <div className="md:ml-auto flex items-center gap-2">
          <Button
            variant="danger"
            size="sm"
            disabled={selectedIds.length === 0 || cancelFsm.isPending}
            onClick={() => {
              if (!confirm(`Cancel ${selectedIds.length} selected instance(s)?`)) return;
              selectedIds.forEach((id) => cancelFsm.mutate({ id, reason: 'Bulk cancel' }));
              setSelectedIds([]);
            }}
          >
            Cancel Selected
          </Button>
          {selectedIds.length > 0 && (
            <span className="text-xs text-content-muted">{selectedIds.length} selected</span>
          )}
        </div>
      </div>

      <FilterPresetManager manager={presets} exportFilePrefix="pacinet-fsm-filters" />

      {!instances || instances.length === 0 ? (
        <div className="text-sm text-content-muted py-4 text-center">No FSM instances</div>
      ) : (
        <div className="bg-surface-alt border border-edge rounded-xl overflow-hidden">
          <Table headers={['', 'ID', 'Definition', 'State', 'Status', 'Progress', 'Updated', '']}>
            {instances.map((inst) => (
              <tr
                key={inst.instance_id}
                className="hover:bg-surface-hover cursor-pointer"
                onClick={() => setSelectedInstance(inst.instance_id)}
              >
                <td className="px-4 py-3" onClick={(e) => e.stopPropagation()}>
                  <input
                    type="checkbox"
                    checked={selectedIds.includes(inst.instance_id)}
                    onChange={(e) =>
                      setSelectedIds((prev) =>
                        e.target.checked
                          ? [...new Set([...prev, inst.instance_id])]
                          : prev.filter((id) => id !== inst.instance_id)
                      )
                    }
                    className="h-4 w-4 rounded border-edge bg-surface accent-accent"
                    aria-label={`Select instance ${inst.instance_id}`}
                  />
                </td>
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
