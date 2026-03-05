import { useState } from 'react';
import { useNodes } from '../../hooks/useNodes';
import { useDeployPolicy, useBatchDeploy, useDryRunDeploy } from '../../hooks/useDeploy';
import Card from '../ui/Card';
import Button from '../ui/Button';
import Badge from '../ui/Badge';
import DryRunPreview from './DryRunPreview';
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
  const [axi, setAxi] = useState(false);
  const [ports, setPorts] = useState(1);
  const [target, setTarget] = useState('standalone');
  const [dynamic, setDynamic] = useState(false);
  const [dynamicEntries, setDynamicEntries] = useState(16);
  const [width, setWidth] = useState(8);
  const [ptp, setPtp] = useState(false);
  const [rss, setRss] = useState(false);
  const [rssQueues, setRssQueues] = useState(4);
  const [intEnabled, setIntEnabled] = useState(false);
  const [intSwitchId, setIntSwitchId] = useState(0);

  const { data: nodes } = useNodes();
  const deployPolicy = useDeployPolicy();
  const batchDeploy = useBatchDeploy();
  const dryRunDeploy = useDryRunDeploy();

  const handleDeploy = () => {
    if (mode === 'single') {
      if (!nodeId || !rulesYaml) return;
      deployPolicy.mutate({
        node_id: nodeId,
        rules_yaml: rulesYaml,
        counters,
        rate_limit: rateLimit,
        conntrack,
        axi,
        ports,
        target,
        dynamic,
        dynamic_entries: dynamicEntries,
        width,
        ptp,
        rss,
        rss_queues: rssQueues,
        int: intEnabled,
        int_switch_id: intSwitchId,
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
        axi,
        ports,
        target,
        dynamic,
        dynamic_entries: dynamicEntries,
        width,
        ptp,
        rss,
        rss_queues: rssQueues,
        int: intEnabled,
        int_switch_id: intSwitchId,
      });
    }
  };

  const handleDryRun = () => {
    if (mode === 'single' && nodeId && rulesYaml) {
      dryRunDeploy.mutate({
        node_id: nodeId,
        rules_yaml: rulesYaml,
        counters,
        rate_limit: rateLimit,
        conntrack,
        axi,
        ports,
        target,
        dynamic,
        dynamic_entries: dynamicEntries,
        width,
        ptp,
        rss,
        rss_queues: rssQueues,
        int: intEnabled,
        int_switch_id: intSwitchId,
      });
    }
  };

  const isPending = deployPolicy.isPending || batchDeploy.isPending || dryRunDeploy.isPending;
  const result = mode === 'single' ? deployPolicy.data : undefined;
  const batchResult = mode === 'batch' ? batchDeploy.data : undefined;
  const dryRunResult = dryRunDeploy.data?.dry_run_result;
  const error = deployPolicy.error || batchDeploy.error || dryRunDeploy.error;

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
          <label className="flex items-center gap-2 text-sm text-content-secondary">
            <input type="checkbox" checked={axi} onChange={(e) => setAxi(e.target.checked)} className="accent-accent" />
            AXI
          </label>
          <label className="flex items-center gap-2 text-sm text-content-secondary">
            <input type="checkbox" checked={dynamic} onChange={(e) => setDynamic(e.target.checked)} className="accent-accent" />
            Dynamic
          </label>
          <label className="flex items-center gap-2 text-sm text-content-secondary">
            <input type="checkbox" checked={ptp} onChange={(e) => setPtp(e.target.checked)} className="accent-accent" />
            PTP
          </label>
          <label className="flex items-center gap-2 text-sm text-content-secondary">
            <input type="checkbox" checked={rss} onChange={(e) => setRss(e.target.checked)} className="accent-accent" />
            RSS
          </label>
          <label className="flex items-center gap-2 text-sm text-content-secondary">
            <input type="checkbox" checked={intEnabled} onChange={(e) => setIntEnabled(e.target.checked)} className="accent-accent" />
            INT
          </label>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-4">
          <label className="text-sm text-content-secondary">
            <div className="text-xs text-content-muted mb-1">Ports</div>
            <input
              type="number"
              min={1}
              max={256}
              value={ports}
              onChange={(e) => setPorts(Math.max(1, Number(e.target.value) || 1))}
              className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content"
            />
          </label>
          <label className="text-sm text-content-secondary">
            <div className="text-xs text-content-muted mb-1">Target</div>
            <select
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content"
            >
              <option value="standalone">standalone</option>
              <option value="opennic">opennic</option>
              <option value="corundum">corundum</option>
            </select>
          </label>
          <label className="text-sm text-content-secondary">
            <div className="text-xs text-content-muted mb-1">Dynamic Entries</div>
            <input
              type="number"
              min={1}
              max={256}
              value={dynamicEntries}
              onChange={(e) => setDynamicEntries(Math.max(1, Number(e.target.value) || 1))}
              className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content"
            />
          </label>
          <label className="text-sm text-content-secondary">
            <div className="text-xs text-content-muted mb-1">Width (bits)</div>
            <input
              type="number"
              min={8}
              step={8}
              value={width}
              onChange={(e) => setWidth(Math.max(8, Number(e.target.value) || 8))}
              className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content"
            />
          </label>
          <label className="text-sm text-content-secondary">
            <div className="text-xs text-content-muted mb-1">RSS Queues</div>
            <input
              type="number"
              min={1}
              max={16}
              value={rssQueues}
              onChange={(e) => setRssQueues(Math.min(16, Math.max(1, Number(e.target.value) || 1)))}
              className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content"
            />
          </label>
          <label className="text-sm text-content-secondary">
            <div className="text-xs text-content-muted mb-1">INT Switch ID</div>
            <input
              type="number"
              min={0}
              max={65535}
              value={intSwitchId}
              onChange={(e) => setIntSwitchId(Math.min(65535, Math.max(0, Number(e.target.value) || 0)))}
              className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content"
            />
          </label>
        </div>

        <div className="flex gap-2">
          <Button onClick={handleDeploy} disabled={isPending || !rulesYaml}>
            {deployPolicy.isPending || batchDeploy.isPending ? 'Deploying...' : 'Deploy'}
          </Button>
          {mode === 'single' && (
            <button
              onClick={handleDryRun}
              disabled={isPending || !rulesYaml || !nodeId}
              className="px-4 py-2 text-sm rounded-lg border border-edge text-content-secondary hover:text-content hover:bg-surface-hover transition-colors disabled:opacity-50"
            >
              {dryRunDeploy.isPending ? 'Previewing...' : 'Preview (Dry Run)'}
            </button>
          )}
        </div>
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

      {dryRunResult && <DryRunPreview result={dryRunResult} />}

      {error && (
        <Card title="Error">
          <div className="text-red-400 text-sm">{(error as Error).message}</div>
        </Card>
      )}
    </div>
  );
}
