import { useState } from 'react';
import { useFsmDefinitions, useCreateFsmDefinition, useDeleteFsmDefinition } from '../../hooks/useFsm';
import Table from '../ui/Table';
import Button from '../ui/Button';
import Badge from '../ui/Badge';
import Spinner from '../ui/Spinner';

export default function DefinitionList() {
  const [showCreate, setShowCreate] = useState(false);
  const [yaml, setYaml] = useState('');
  const { data: defs, isLoading } = useFsmDefinitions();
  const createDef = useCreateFsmDefinition();
  const deleteDef = useDeleteFsmDefinition();

  const handleCreate = () => {
    if (!yaml) return;
    createDef.mutate(yaml, {
      onSuccess: () => {
        setYaml('');
        setShowCreate(false);
      },
    });
  };

  if (isLoading) return <Spinner />;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <span className="text-sm text-content-muted">{defs?.length || 0} definitions</span>
        <Button size="sm" onClick={() => setShowCreate(!showCreate)}>
          {showCreate ? 'Cancel' : 'Create Definition'}
        </Button>
      </div>

      {showCreate && (
        <div className="bg-surface border border-edge rounded-xl p-4 space-y-3">
          <textarea
            value={yaml}
            onChange={(e) => setYaml(e.target.value)}
            rows={12}
            placeholder="name: canary-rollout&#10;kind: deployment&#10;description: Canary then staged rollout&#10;initial: canary&#10;states:&#10;  canary:&#10;    ..."
            className="w-full px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm font-mono text-content placeholder:text-content-muted focus:outline-none focus:border-accent resize-y"
          />
          <div className="flex gap-2">
            <Button size="sm" onClick={handleCreate} disabled={createDef.isPending || !yaml}>
              {createDef.isPending ? 'Creating...' : 'Create'}
            </Button>
            {createDef.error && (
              <span className="text-red-400 text-xs self-center">{(createDef.error as Error).message}</span>
            )}
          </div>
        </div>
      )}

      {!defs || defs.length === 0 ? (
        <div className="text-sm text-content-muted py-4 text-center">No FSM definitions</div>
      ) : (
        <div className="bg-surface-alt border border-edge rounded-xl overflow-hidden">
          <Table headers={['Name', 'Kind', 'Description', 'States', 'Initial', '']}>
            {defs.map((d) => (
              <tr key={d.name} className="hover:bg-surface-hover">
                <td className="px-4 py-3 font-medium">{d.name}</td>
                <td className="px-4 py-3">
                  <Badge className="bg-purple-500/20 text-purple-400">{d.kind}</Badge>
                </td>
                <td className="px-4 py-3 text-content-secondary text-sm">{d.description}</td>
                <td className="px-4 py-3 text-center">{d.state_count}</td>
                <td className="px-4 py-3 font-mono text-xs">{d.initial_state}</td>
                <td className="px-4 py-3">
                  <Button
                    variant="danger"
                    size="sm"
                    onClick={() => {
                      if (confirm(`Delete definition "${d.name}"?`)) {
                        deleteDef.mutate(d.name);
                      }
                    }}
                    disabled={deleteDef.isPending}
                  >
                    Delete
                  </Button>
                </td>
              </tr>
            ))}
          </Table>
        </div>
      )}
    </div>
  );
}
