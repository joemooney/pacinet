import { useState } from 'react';
import {
  useFsmDefinitions,
  useCreateFsmDefinition,
  useDeleteFsmDefinition,
  useStartFsm,
} from '../../hooks/useFsm';
import Table from '../ui/Table';
import Button from '../ui/Button';
import Badge from '../ui/Badge';
import Spinner from '../ui/Spinner';

export default function DefinitionList() {
  const [showCreate, setShowCreate] = useState(false);
  const [search, setSearch] = useState('');
  const [startTarget, setStartTarget] = useState<string | null>(null);
  const [startRules, setStartRules] = useState('');
  const [startLabel, setStartLabel] = useState('');
  const [startAxi, setStartAxi] = useState(false);
  const [startPorts, setStartPorts] = useState(1);
  const [startCompileTarget, setStartCompileTarget] = useState('standalone');
  const [startDynamic, setStartDynamic] = useState(false);
  const [startDynamicEntries, setStartDynamicEntries] = useState(16);
  const [startWidth, setStartWidth] = useState(8);
  const [startPtp, setStartPtp] = useState(false);
  const [startRss, setStartRss] = useState(false);
  const [startRssQueues, setStartRssQueues] = useState(4);
  const [startIntEnabled, setStartIntEnabled] = useState(false);
  const [startIntSwitchId, setStartIntSwitchId] = useState(0);
  const [yaml, setYaml] = useState('');
  const { data: defs, isLoading } = useFsmDefinitions();
  const createDef = useCreateFsmDefinition();
  const deleteDef = useDeleteFsmDefinition();
  const startFsm = useStartFsm();

  const filteredDefs =
    defs?.filter(
      (d) =>
        d.name.toLowerCase().includes(search.toLowerCase()) ||
        d.kind.toLowerCase().includes(search.toLowerCase()) ||
        d.description.toLowerCase().includes(search.toLowerCase())
    ) ?? [];

  const handleCreate = () => {
    if (!yaml) return;
    createDef.mutate(yaml, {
      onSuccess: () => {
        setYaml('');
        setShowCreate(false);
      },
    });
  };

  const handleStart = () => {
    if (!startTarget) return;
    const target_label_filter =
      startLabel && startLabel.includes('=')
        ? { [startLabel.split('=')[0].trim()]: startLabel.split('=').slice(1).join('=').trim() }
        : undefined;
    startFsm.mutate(
      {
        definition_name: startTarget,
        rules_yaml: startRules || undefined,
        target_label_filter,
        axi: startAxi,
        ports: startPorts,
        target: startCompileTarget,
        dynamic: startDynamic,
        dynamic_entries: startDynamicEntries,
        width: startWidth,
        ptp: startPtp,
        rss: startRss,
        rss_queues: startRssQueues,
        int: startIntEnabled,
        int_switch_id: startIntSwitchId,
      },
      {
        onSuccess: () => {
          setStartTarget(null);
          setStartRules('');
          setStartLabel('');
        },
      }
    );
  };

  if (isLoading) return <Spinner />;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="text-sm text-content-muted">{filteredDefs.length} definitions</span>
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search definitions..."
            className="px-3 py-1.5 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent w-56"
          />
        </div>
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

      {startTarget && (
        <div className="bg-surface border border-edge rounded-xl p-4 space-y-3">
          <div className="text-sm text-content-secondary">
            Start instance from <span className="text-content font-medium">{startTarget}</span>
          </div>
          <input
            value={startLabel}
            onChange={(e) => setStartLabel(e.target.value)}
            placeholder="Target label (optional, e.g. env=prod)"
            className="w-full px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent"
          />
          <textarea
            value={startRules}
            onChange={(e) => setStartRules(e.target.value)}
            rows={8}
            placeholder="Optional rules YAML for deployment FSMs"
            className="w-full px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm font-mono text-content placeholder:text-content-muted focus:outline-none focus:border-accent resize-y"
          />
          <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
            <label className="text-xs text-content-muted">
              <div className="mb-1">Ports</div>
              <input
                type="number"
                min={1}
                value={startPorts}
                onChange={(e) => setStartPorts(Math.max(1, Number(e.target.value) || 1))}
                className="w-full px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm text-content"
              />
            </label>
            <label className="text-xs text-content-muted">
              <div className="mb-1">Target</div>
              <select
                value={startCompileTarget}
                onChange={(e) => setStartCompileTarget(e.target.value)}
                className="w-full px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm text-content"
              >
                <option value="standalone">standalone</option>
                <option value="opennic">opennic</option>
                <option value="corundum">corundum</option>
              </select>
            </label>
            <label className="text-xs text-content-muted">
              <div className="mb-1">Dynamic Entries</div>
              <input
                type="number"
                min={1}
                value={startDynamicEntries}
                onChange={(e) => setStartDynamicEntries(Math.max(1, Number(e.target.value) || 1))}
                className="w-full px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm text-content"
              />
            </label>
            <label className="text-xs text-content-muted">
              <div className="mb-1">Width (bits)</div>
              <input
                type="number"
                min={8}
                step={8}
                value={startWidth}
                onChange={(e) => setStartWidth(Math.max(8, Number(e.target.value) || 8))}
                className="w-full px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm text-content"
              />
            </label>
            <label className="text-xs text-content-muted">
              <div className="mb-1">RSS Queues</div>
              <input
                type="number"
                min={1}
                max={16}
                value={startRssQueues}
                onChange={(e) => setStartRssQueues(Math.min(16, Math.max(1, Number(e.target.value) || 1)))}
                className="w-full px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm text-content"
              />
            </label>
            <label className="text-xs text-content-muted">
              <div className="mb-1">INT Switch ID</div>
              <input
                type="number"
                min={0}
                max={65535}
                value={startIntSwitchId}
                onChange={(e) => setStartIntSwitchId(Math.min(65535, Math.max(0, Number(e.target.value) || 0)))}
                className="w-full px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm text-content"
              />
            </label>
          </div>
          <div className="flex gap-4">
            <label className="flex items-center gap-2 text-sm text-content-secondary">
              <input type="checkbox" checked={startAxi} onChange={(e) => setStartAxi(e.target.checked)} className="accent-accent" />
              AXI
            </label>
            <label className="flex items-center gap-2 text-sm text-content-secondary">
              <input type="checkbox" checked={startDynamic} onChange={(e) => setStartDynamic(e.target.checked)} className="accent-accent" />
              Dynamic
            </label>
            <label className="flex items-center gap-2 text-sm text-content-secondary">
              <input type="checkbox" checked={startPtp} onChange={(e) => setStartPtp(e.target.checked)} className="accent-accent" />
              PTP
            </label>
            <label className="flex items-center gap-2 text-sm text-content-secondary">
              <input type="checkbox" checked={startRss} onChange={(e) => setStartRss(e.target.checked)} className="accent-accent" />
              RSS
            </label>
            <label className="flex items-center gap-2 text-sm text-content-secondary">
              <input type="checkbox" checked={startIntEnabled} onChange={(e) => setStartIntEnabled(e.target.checked)} className="accent-accent" />
              INT
            </label>
          </div>
          <div className="flex gap-2">
            <Button size="sm" onClick={handleStart} disabled={startFsm.isPending}>
              {startFsm.isPending ? 'Starting...' : 'Start Instance'}
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setStartTarget(null)}>
              Close
            </Button>
            {startFsm.error && (
              <span className="text-red-400 text-xs self-center">{(startFsm.error as Error).message}</span>
            )}
          </div>
        </div>
      )}

      {filteredDefs.length === 0 ? (
        <div className="text-sm text-content-muted py-4 text-center">No FSM definitions</div>
      ) : (
        <div className="bg-surface-alt border border-edge rounded-xl overflow-hidden">
          <Table headers={['Name', 'Kind', 'Description', 'States', 'Initial', 'Actions']}>
            {filteredDefs.map((d) => (
              <tr key={d.name} className="hover:bg-surface-hover">
                <td className="px-4 py-3 font-medium">{d.name}</td>
                <td className="px-4 py-3">
                  <Badge className="bg-cyan-500/20 text-cyan-400">{d.kind}</Badge>
                </td>
                <td className="px-4 py-3 text-content-secondary text-sm">{d.description}</td>
                <td className="px-4 py-3 text-center">{d.state_count}</td>
                <td className="px-4 py-3 font-mono text-xs">{d.initial_state}</td>
                <td className="px-4 py-3">
                  <div className="flex gap-2">
                    <Button variant="secondary" size="sm" onClick={() => setStartTarget(d.name)}>
                      Start
                    </Button>
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
                  </div>
                </td>
              </tr>
            ))}
          </Table>
        </div>
      )}
    </div>
  );
}
