import { useState } from 'react';
import { useNodes } from '../../hooks/useNodes';
import { useDeployPolicy, useBatchDeploy } from '../../hooks/useDeploy';
import Card from '../ui/Card';
import Button from '../ui/Button';
import Badge from '../ui/Badge';
import { statusColorClass } from '../../lib/utils';

type Mode = 'single' | 'batch';

export default function DeployPage() {
  const [mode, setMode] = useState<Mode>('single');
  const [nodeId, setNodeId] = useState('');
  const [labelFilter, setLabelFilter] = useState('');
  const [rulesYaml, setRulesYaml] = useState('');
  const [counters, setCounters] = useState(false);
  const [rateLimit, setRateLimit] = useState(false);
  const [conntrack, setConntrack] = useState(false);

  const { data: nodes } = useNodes();
  const deployPolicy = useDeployPolicy();
  const batchDeploy = useBatchDeploy();

  const handleDeploy = () => {
    if (mode === 'single') {
      if (!nodeId || !rulesYaml) return;
      deployPolicy.mutate({
        node_id: nodeId,
        rules_yaml: rulesYaml,
        counters,
        rate_limit: rateLimit,
        conntrack,
      });
    } else {
      if (!rulesYaml) return;
      const filter: Record<string, string> = {};
      for (const pair of labelFilter.split(',')) {
        const [k, v] = pair.split('=');
        if (k && v) filter[k.trim()] = v.trim();
      }
      batchDeploy.mutate({
        label_filter: filter,
        rules_yaml: rulesYaml,
        counters,
        rate_limit: rateLimit,
        conntrack,
      });
    }
  };

  const isPending = deployPolicy.isPending || batchDeploy.isPending;
  const result = mode === 'single' ? deployPolicy.data : undefined;
  const batchResult = mode === 'batch' ? batchDeploy.data : undefined;
  const error = deployPolicy.error || batchDeploy.error;

  return (
    <div className="max-w-2xl animate-fade-in space-y-6">
      <Card title="Deploy Policy">
        {/* Mode toggle */}
        <div className="flex gap-2 mb-4">
          <button
            onClick={() => setMode('single')}
            className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
              mode === 'single' ? 'bg-accent text-white' : 'bg-surface-hover text-content-secondary'
            }`}
          >
            Single Node
          </button>
          <button
            onClick={() => setMode('batch')}
            className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
              mode === 'batch' ? 'bg-accent text-white' : 'bg-surface-hover text-content-secondary'
            }`}
          >
            Batch (by label)
          </button>
        </div>

        {/* Target selection */}
        {mode === 'single' ? (
          <div className="mb-4">
            <label className="block text-xs text-content-muted mb-1">Target Node</label>
            <select
              value={nodeId}
              onChange={(e) => setNodeId(e.target.value)}
              className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content focus:outline-none focus:border-accent"
            >
              <option value="">Select a node...</option>
              {nodes?.map((n) => (
                <option key={n.node_id} value={n.node_id}>
                  {n.hostname} ({n.state})
                </option>
              ))}
            </select>
          </div>
        ) : (
          <div className="mb-4">
            <label className="block text-xs text-content-muted mb-1">Label Filter (e.g. env=prod,tier=web)</label>
            <input
              type="text"
              value={labelFilter}
              onChange={(e) => setLabelFilter(e.target.value)}
              placeholder="env=prod"
              className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent"
            />
          </div>
        )}

        {/* YAML textarea */}
        <div className="mb-4">
          <label className="block text-xs text-content-muted mb-1">Rules YAML</label>
          <textarea
            value={rulesYaml}
            onChange={(e) => setRulesYaml(e.target.value)}
            rows={12}
            placeholder="rules:&#10;  - name: drop_ssh&#10;    protocol: tcp&#10;    dst_port: 22&#10;    action: drop"
            className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm font-mono text-content placeholder:text-content-muted focus:outline-none focus:border-accent resize-y"
          />
        </div>

        {/* Compile options */}
        <div className="flex gap-4 mb-4">
          <label className="flex items-center gap-2 text-sm text-content-secondary">
            <input type="checkbox" checked={counters} onChange={(e) => setCounters(e.target.checked)} className="accent-accent" />
            Counters
          </label>
          <label className="flex items-center gap-2 text-sm text-content-secondary">
            <input type="checkbox" checked={rateLimit} onChange={(e) => setRateLimit(e.target.checked)} className="accent-accent" />
            Rate Limit
          </label>
          <label className="flex items-center gap-2 text-sm text-content-secondary">
            <input type="checkbox" checked={conntrack} onChange={(e) => setConntrack(e.target.checked)} className="accent-accent" />
            Conntrack
          </label>
        </div>

        <Button onClick={handleDeploy} disabled={isPending || !rulesYaml}>
          {isPending ? 'Deploying...' : 'Deploy'}
        </Button>
      </Card>

      {/* Result display */}
      {result && (
        <Card title="Deploy Result">
          <div className="flex items-center gap-2 mb-2">
            <Badge className={result.success ? 'bg-emerald-500/20 text-emerald-400' : 'bg-red-500/20 text-red-400'}>
              {result.success ? 'Success' : 'Failed'}
            </Badge>
            <span className="text-sm">{result.message}</span>
          </div>
          {result.warnings.length > 0 && (
            <div className="text-xs text-amber-400 mt-2">
              {result.warnings.map((w, i) => <div key={i}>{w}</div>)}
            </div>
          )}
        </Card>
      )}

      {batchResult && (
        <Card title="Batch Deploy Result">
          <div className="flex gap-4 mb-3 text-sm">
            <span>Total: {batchResult.total_nodes}</span>
            <span className="text-emerald-400">Succeeded: {batchResult.succeeded}</span>
            <span className="text-red-400">Failed: {batchResult.failed}</span>
          </div>
          <div className="space-y-1">
            {batchResult.results.map((r) => (
              <div key={r.node_id} className="flex items-center gap-2 text-sm">
                <Badge className={statusColorClass(r.success ? 'success' : 'agent_failure')}>
                  {r.success ? 'OK' : 'FAIL'}
                </Badge>
                <span>{r.hostname}</span>
                <span className="text-content-muted text-xs">{r.message}</span>
              </div>
            ))}
          </div>
        </Card>
      )}

      {error && (
        <Card title="Error">
          <div className="text-red-400 text-sm">{(error as Error).message}</div>
        </Card>
      )}
    </div>
  );
}
