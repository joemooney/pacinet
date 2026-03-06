import { useEffect, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNodes } from '../../hooks/useNodes';
import { useDeployPolicy, useBatchDeploy, useDryRunDeploy } from '../../hooks/useDeploy';
import { useCreateTemplate, useTemplates } from '../../hooks/useTemplates';
import { apiFetch } from '../../api/client';
import Card from '../ui/Card';
import Button from '../ui/Button';
import Badge from '../ui/Badge';
import DryRunPreview from './DryRunPreview';
import { formatTimestamp, shortId, statusColorClass } from '../../lib/utils';
import type { NodeJson, PolicyJson, PolicyTemplateJson, PolicyVersionJson } from '../../types/api';

type Mode = 'single' | 'batch';

type DiffLine = {
  kind: 'context' | 'added' | 'removed';
  text: string;
};

type ExtractedRule = {
  block: string;
  name: string;
  priority: string;
};

type ComposeWarning = {
  code: string;
  message: string;
};

function parseLabelFilter(input: string): Record<string, string> {
  const filter: Record<string, string> = {};
  for (const pair of input.split(',')) {
    const [k, v] = pair.split('=');
    if (k && v) filter[k.trim()] = v.trim();
  }
  return filter;
}

function nodeMatchesLabelFilter(node: NodeJson, filter: Record<string, string>): boolean {
  return Object.entries(filter).every(([k, v]) => node.labels[k] === v);
}

function buildSimpleLineDiff(oldText: string, newText: string): DiffLine[] {
  const oldLines = oldText.split('\n');
  const newLines = newText.split('\n');
  const max = Math.max(oldLines.length, newLines.length);
  const out: DiffLine[] = [];

  for (let i = 0; i < max; i += 1) {
    const oldLine = oldLines[i];
    const newLine = newLines[i];

    if (oldLine === newLine) {
      if (oldLine !== undefined) out.push({ kind: 'context', text: oldLine });
      continue;
    }
    if (oldLine !== undefined) out.push({ kind: 'removed', text: oldLine });
    if (newLine !== undefined) out.push({ kind: 'added', text: newLine });
  }

  return out;
}

function parseTemplateVariables(raw: string): Record<string, string> {
  const vars: Record<string, string> = {};
  for (const line of raw.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const idx = trimmed.indexOf('=');
    if (idx <= 0) continue;
    const key = trimmed.slice(0, idx).trim();
    const value = trimmed.slice(idx + 1).trim();
    if (key) vars[key] = value;
  }
  return vars;
}

function substituteVars(input: string, vars: Record<string, string>): { output: string; missing: string[] } {
  const missing = new Set<string>();
  const output = input.replace(/\$\{([A-Za-z0-9_]+)\}/g, (_, name: string) => {
    if (Object.prototype.hasOwnProperty.call(vars, name)) return vars[name];
    missing.add(name);
    return `\${${name}}`;
  });
  return { output, missing: Array.from(missing) };
}

function leadingSpaces(line: string): number {
  const m = line.match(/^ */);
  return m ? m[0].length : 0;
}

function extractDefaultsAction(yaml: string): string | null {
  const lines = yaml.split('\n');
  let inDefaults = false;
  let defaultsIndent = -1;
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const indent = leadingSpaces(line);
    if (trimmed === 'defaults:') {
      inDefaults = true;
      defaultsIndent = indent;
      continue;
    }
    if (inDefaults) {
      if (indent <= defaultsIndent) {
        inDefaults = false;
        continue;
      }
      const m = trimmed.match(/^action:\s*(.+)$/);
      if (m) return m[1].trim().replace(/^['"]|['"]$/g, '');
    }
  }
  return null;
}

function extractRules(yaml: string): ExtractedRule[] {
  const lines = yaml.split('\n');
  const idx = lines.findIndex((l) => l.trim() === 'rules:');
  if (idx < 0) return [];

  const rulesIndent = leadingSpaces(lines[idx]);
  const out: ExtractedRule[] = [];
  let i = idx + 1;

  while (i < lines.length) {
    const line = lines[i];
    const trimmed = line.trim();
    const indent = leadingSpaces(line);

    if (!trimmed) {
      i += 1;
      continue;
    }
    if (indent <= rulesIndent) break;

    if (trimmed.startsWith('- ')) {
      const itemIndent = indent;
      const blockLines: string[] = [line];
      let j = i + 1;
      while (j < lines.length) {
        const next = lines[j];
        const nextTrimmed = next.trim();
        const nextIndent = leadingSpaces(next);
        if (!nextTrimmed) {
          blockLines.push(next);
          j += 1;
          continue;
        }
        if (nextIndent <= rulesIndent) break;
        if (nextIndent === itemIndent && nextTrimmed.startsWith('- ')) break;
        blockLines.push(next);
        j += 1;
      }

      const normalized = blockLines
        .map((l) => {
          if (!l.trim()) return '';
          const withoutIndent = l.slice(Math.min(itemIndent, l.length));
          return `    ${withoutIndent}`;
        })
        .join('\n');
      const nameMatch = normalized.match(/^\s*-\s*name:\s*([^\n#]+)/m);
      const priorityMatch = normalized.match(/^\s*priority:\s*([0-9]+)/m);
      out.push({
        block: normalized,
        name: nameMatch ? nameMatch[1].trim() : '(unnamed)',
        priority: priorityMatch ? priorityMatch[1] : '(none)',
      });
      i = j;
      continue;
    }

    i += 1;
  }

  return out;
}

export default function DeployPage() {
  const [mode, setMode] = useState<Mode>('single');
  const [nodeId, setNodeId] = useState('');
  const [labelFilter, setLabelFilter] = useState('');
  const [rulesYaml, setRulesYaml] = useState('');
  const [loadingDraft, setLoadingDraft] = useState(false);
  const [historyBaseVersion, setHistoryBaseVersion] = useState('');
  const [historyCompareVersion, setHistoryCompareVersion] = useState('');
  const [selectedTemplate, setSelectedTemplate] = useState('');
  const [selectedTemplates, setSelectedTemplates] = useState<string[]>([]);
  const [templateVars, setTemplateVars] = useState('');
  const [composeWarnings, setComposeWarnings] = useState<ComposeWarning[]>([]);
  const [showSaveTemplate, setShowSaveTemplate] = useState(false);
  const [saveTemplateName, setSaveTemplateName] = useState('');
  const [saveTemplateDescription, setSaveTemplateDescription] = useState('');
  const [saveTemplateTags, setSaveTemplateTags] = useState('');
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
  const { data: templates } = useTemplates();
  const createTemplate = useCreateTemplate();
  const deployPolicy = useDeployPolicy();
  const batchDeploy = useBatchDeploy();
  const dryRunDeploy = useDryRunDeploy();
  const { refetch: refetchActivePolicy } = useQuery({
    queryKey: ['deploy-node-policy', nodeId],
    queryFn: () => apiFetch<PolicyJson>(`/api/nodes/${nodeId}/policy`),
    enabled: mode === 'single' && !!nodeId,
  });
  const { data: policyHistory } = useQuery({
    queryKey: ['deploy-node-policy-history', nodeId],
    queryFn: () => apiFetch<PolicyVersionJson[]>(`/api/nodes/${nodeId}/policy/history?limit=20`),
    enabled: mode === 'single' && !!nodeId,
  });

  useEffect(() => {
    setHistoryBaseVersion('');
    setHistoryCompareVersion('');
  }, [nodeId]);

  useEffect(() => {
    if (!policyHistory || policyHistory.length === 0) return;
    if (!historyBaseVersion) {
      setHistoryBaseVersion(String(policyHistory[0].version));
    }
    if (!historyCompareVersion && policyHistory.length > 1) {
      setHistoryCompareVersion(String(policyHistory[1].version));
    }
  }, [policyHistory, historyBaseVersion, historyCompareVersion]);

  const historyByVersion = useMemo(() => {
    const map = new Map<string, PolicyVersionJson>();
    for (const v of policyHistory || []) {
      map.set(String(v.version), v);
    }
    return map;
  }, [policyHistory]);

  const selectedBase = historyByVersion.get(historyBaseVersion);
  const selectedCompare = historyByVersion.get(historyCompareVersion);
  const historyDiff = selectedBase && selectedCompare
    ? buildSimpleLineDiff(selectedCompare.rules_yaml, selectedBase.rules_yaml)
    : [];
  const availableTemplateOptions = (templates || []).filter(
    (t) => !selectedTemplates.includes(t.name)
  );

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
      const filter = parseLabelFilter(labelFilter);
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

  const handleLoadActiveNodePolicy = async () => {
    if (!nodeId) return;
    setLoadingDraft(true);
    try {
      const res = await refetchActivePolicy();
      if (res.data?.rules_yaml) {
        setRulesYaml(res.data.rules_yaml);
      } else {
        window.alert('No active policy found for this node.');
      }
    } finally {
      setLoadingDraft(false);
    }
  };

  const handleLoadFleetPolicy = async () => {
    if (!nodes || nodes.length === 0) {
      window.alert('No nodes available.');
      return;
    }

    const filter = parseLabelFilter(labelFilter);
    const targets =
      Object.keys(filter).length > 0
        ? nodes.filter((n) => nodeMatchesLabelFilter(n, filter))
        : nodes;

    if (targets.length === 0) {
      window.alert('No nodes matched the label filter.');
      return;
    }

    const hashes = new Set(targets.map((n) => n.policy_hash).filter(Boolean));
    if (hashes.size !== 1) {
      window.alert('Matched nodes have mixed active policies. Pick a single node to load YAML.');
      return;
    }

    setLoadingDraft(true);
    try {
      const policy = await apiFetch<PolicyJson>(`/api/nodes/${targets[0].node_id}/policy`);
      setRulesYaml(policy.rules_yaml);
    } finally {
      setLoadingDraft(false);
    }
  };

  const moveTemplate = (name: string, direction: -1 | 1) => {
    const idx = selectedTemplates.indexOf(name);
    if (idx < 0) return;
    const next = idx + direction;
    if (next < 0 || next >= selectedTemplates.length) return;
    const updated = [...selectedTemplates];
    const [item] = updated.splice(idx, 1);
    updated.splice(next, 0, item);
    setSelectedTemplates(updated);
  };

  const addTemplateToSelection = () => {
    if (!selectedTemplate) return;
    if (selectedTemplates.includes(selectedTemplate)) return;
    setSelectedTemplates([...selectedTemplates, selectedTemplate]);
    setSelectedTemplate('');
  };

  const removeTemplateFromSelection = (name: string) => {
    setSelectedTemplates(selectedTemplates.filter((n) => n !== name));
  };

  const composeFromTemplates = async () => {
    if (selectedTemplates.length === 0) {
      window.alert('Select at least one template.');
      return;
    }
    setLoadingDraft(true);
    try {
      const vars = parseTemplateVariables(templateVars);
      const details = await Promise.all(
        selectedTemplates.map((name) =>
          apiFetch<PolicyTemplateJson>(`/api/templates/${encodeURIComponent(name)}`)
        )
      );

      const warnings: ComposeWarning[] = [];
      const mergedRules: ExtractedRule[] = [];
      const seenNames = new Set<string>();
      const seenPriorities = new Set<string>();
      let defaultsAction: string | null = null;

      for (const tpl of details) {
        const sub = substituteVars(tpl.rules_yaml, vars);
        if (sub.missing.length > 0) {
          warnings.push({
            code: 'MISSING_VAR',
            message: `${tpl.name}: missing variables ${sub.missing.join(', ')}`,
          });
        }

        if (!defaultsAction) {
          defaultsAction = extractDefaultsAction(sub.output);
        }
        const rules = extractRules(sub.output);
        if (rules.length === 0) {
          warnings.push({
            code: 'NO_RULES',
            message: `${tpl.name}: no rules block detected`,
          });
        }
        for (const rule of rules) {
          if (seenNames.has(rule.name)) {
            warnings.push({
              code: 'DUP_RULE_NAME',
              message: `Duplicate rule name '${rule.name}' from template '${tpl.name}'`,
            });
          }
          if (rule.priority !== '(none)' && seenPriorities.has(rule.priority)) {
            warnings.push({
              code: 'DUP_PRIORITY',
              message: `Duplicate priority '${rule.priority}' (template '${tpl.name}')`,
            });
          }
          seenNames.add(rule.name);
          if (rule.priority !== '(none)') seenPriorities.add(rule.priority);
          mergedRules.push(rule);
        }
      }

      const composedYaml =
        `pacgate:\n` +
        `  version: "1.0"\n` +
        `  defaults:\n` +
        `    action: ${defaultsAction || 'drop'}\n` +
        `  rules:\n` +
        (mergedRules.length > 0
          ? `${mergedRules.map((r) => r.block).join('\n')}\n`
          : `    []\n`);

      setRulesYaml(composedYaml);
      setComposeWarnings(warnings);
    } finally {
      setLoadingDraft(false);
    }
  };

  const handleSaveDraftAsTemplate = () => {
    if (!rulesYaml.trim()) {
      window.alert('Rules YAML draft is empty.');
      return;
    }
    if (!saveTemplateName.trim()) {
      window.alert('Template name is required.');
      return;
    }

    const tags = saveTemplateTags
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);

    createTemplate.mutate(
      {
        name: saveTemplateName.trim(),
        description: saveTemplateDescription.trim(),
        rules_yaml: rulesYaml,
        tags,
      },
      {
        onSuccess: () => {
          setShowSaveTemplate(false);
          setSaveTemplateName('');
          setSaveTemplateDescription('');
          setSaveTemplateTags('');
        },
      }
    );
  };

  const isPending = deployPolicy.isPending || batchDeploy.isPending || dryRunDeploy.isPending;
  const result = mode === 'single' ? deployPolicy.data : undefined;
  const batchResult = mode === 'batch' ? batchDeploy.data : undefined;
  const dryRunResult = dryRunDeploy.data?.dry_run_result;
  const error = deployPolicy.error || batchDeploy.error || dryRunDeploy.error;

  return (
    <div className="max-w-2xl animate-fade-in space-y-6">
      <Card title="Deploy Policy">
        <div className="mb-4 rounded-lg border border-edge bg-surface p-3 space-y-3">
          <div className="text-sm font-medium">Template Composer</div>
          <div className="text-xs text-content-muted">
            Build a deploy draft by composing reusable templates in order. Later templates appear later in the rule list.
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <select
              value={selectedTemplate}
              onChange={(e) => setSelectedTemplate(e.target.value)}
              className="min-w-56 px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm text-content"
            >
              <option value="">Add template...</option>
              {availableTemplateOptions.map((t) => (
                <option key={t.name} value={t.name}>
                  {t.name}
                </option>
              ))}
            </select>
            <Button size="sm" variant="ghost" onClick={addTemplateToSelection} disabled={!selectedTemplate}>
              Add
            </Button>
            <Button
              size="sm"
              onClick={composeFromTemplates}
              disabled={selectedTemplates.length === 0 || loadingDraft}
            >
              {loadingDraft ? 'Composing...' : 'Compose To Draft'}
            </Button>
          </div>
          <textarea
            value={templateVars}
            onChange={(e) => setTemplateVars(e.target.value)}
            rows={3}
            placeholder={'Template variables (optional), one per line:\nCIDR=10.0.0.0/24\nALLOW_PORT=443'}
            className="w-full px-3 py-2 bg-surface-alt border border-edge rounded-lg text-xs font-mono text-content placeholder:text-content-muted"
          />
          {selectedTemplates.length > 0 && (
            <div className="space-y-1">
              {selectedTemplates.map((name, idx) => {
                const tpl = (templates || []).find((t) => t.name === name);
                return (
                  <div key={name} className="flex items-center gap-2 text-xs bg-surface-alt rounded px-2 py-1">
                    <span className="text-content-muted">{idx + 1}.</span>
                    <span className="font-mono">{name}</span>
                    <div className="flex gap-1">
                      {(tpl?.tags || []).map((tag) => (
                        <Badge key={`${name}-${tag}`} className="bg-accent/20 text-accent text-[10px]">
                          {tag}
                        </Badge>
                      ))}
                    </div>
                    <div className="ml-auto flex gap-1">
                      <Button size="sm" variant="ghost" onClick={() => moveTemplate(name, -1)} disabled={idx === 0}>
                        Up
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => moveTemplate(name, 1)}
                        disabled={idx === selectedTemplates.length - 1}
                      >
                        Down
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => removeTemplateFromSelection(name)}>
                        Remove
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
          {composeWarnings.length > 0 && (
            <div className="text-xs text-amber-400 space-y-1">
              {composeWarnings.map((w, i) => (
                <div key={`${w.code}-${i}`}>[{w.code}] {w.message}</div>
              ))}
            </div>
          )}
        </div>

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
          <div className="mb-2 flex flex-wrap items-center gap-2">
            {mode === 'single' ? (
              <Button
                size="sm"
                variant="ghost"
                onClick={handleLoadActiveNodePolicy}
                disabled={!nodeId || loadingDraft}
              >
                {loadingDraft ? 'Loading...' : 'Load Active Policy'}
              </Button>
            ) : (
              <Button
                size="sm"
                variant="ghost"
                onClick={handleLoadFleetPolicy}
                disabled={loadingDraft}
              >
                {loadingDraft ? 'Loading...' : 'Load Fleet Policy'}
              </Button>
            )}
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setShowSaveTemplate((v) => !v)}
              disabled={!rulesYaml.trim()}
            >
              {showSaveTemplate ? 'Cancel Save Template' : 'Save Draft As Template'}
            </Button>
            <span className="text-xs text-content-muted">
              Draft YAML to deploy. Deploy replaces the active policy on target node(s).
            </span>
          </div>
          {showSaveTemplate && (
            <div className="mb-2 rounded-lg border border-edge bg-surface-alt p-3 space-y-2">
              <div className="text-xs text-content-muted">
                Save current draft to template library for reuse in Template Composer.
              </div>
              <div className="grid grid-cols-1 md:grid-cols-3 gap-2">
                <input
                  type="text"
                  value={saveTemplateName}
                  onChange={(e) => setSaveTemplateName(e.target.value)}
                  placeholder="Template name"
                  className="px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted"
                />
                <input
                  type="text"
                  value={saveTemplateDescription}
                  onChange={(e) => setSaveTemplateDescription(e.target.value)}
                  placeholder="Description (optional)"
                  className="px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted"
                />
                <input
                  type="text"
                  value={saveTemplateTags}
                  onChange={(e) => setSaveTemplateTags(e.target.value)}
                  placeholder="tags,comma,separated"
                  className="px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted"
                />
              </div>
              <div className="flex items-center gap-2">
                <Button
                  size="sm"
                  onClick={handleSaveDraftAsTemplate}
                  disabled={createTemplate.isPending || !saveTemplateName.trim() || !rulesYaml.trim()}
                >
                  {createTemplate.isPending ? 'Saving...' : 'Save Template'}
                </Button>
                {createTemplate.error && (
                  <span className="text-xs text-red-400">{(createTemplate.error as Error).message}</span>
                )}
              </div>
            </div>
          )}
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

      {mode === 'single' && nodeId && (
        <Card title="Single Node Policy History">
          {!policyHistory || policyHistory.length === 0 ? (
            <div className="text-sm text-content-muted">No policy history for selected node.</div>
          ) : (
            <div className="space-y-4">
              <div className="space-y-2">
                {policyHistory.map((v) => (
                  <div key={v.version} className="flex items-center gap-2 text-xs">
                    <span className="text-content-secondary">v{v.version}</span>
                    <span className="font-mono text-content-muted">{shortId(v.policy_hash)}</span>
                    <span className="text-content-muted">{formatTimestamp(v.deployed_at)}</span>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => setRulesYaml(v.rules_yaml)}
                    >
                      Load To Draft
                    </Button>
                  </div>
                ))}
              </div>

              <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                <label className="text-xs text-content-muted">
                  Compare version (older)
                  <select
                    value={historyCompareVersion}
                    onChange={(e) => setHistoryCompareVersion(e.target.value)}
                    className="mt-1 w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content"
                  >
                    <option value="">Select version...</option>
                    {policyHistory.map((v) => (
                      <option key={`cmp-${v.version}`} value={v.version}>
                        v{v.version}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="text-xs text-content-muted">
                  Against version (newer)
                  <select
                    value={historyBaseVersion}
                    onChange={(e) => setHistoryBaseVersion(e.target.value)}
                    className="mt-1 w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content"
                  >
                    <option value="">Select version...</option>
                    {policyHistory.map((v) => (
                      <option key={`base-${v.version}`} value={v.version}>
                        v{v.version}
                      </option>
                    ))}
                  </select>
                </label>
              </div>

              {selectedBase && selectedCompare && (
                <pre className="bg-surface p-3 rounded-lg text-xs font-mono overflow-x-auto max-h-64">
                  {historyDiff.map((line, idx) => (
                    <div
                      key={idx}
                      className={
                        line.kind === 'added'
                          ? 'text-emerald-400'
                          : line.kind === 'removed'
                            ? 'text-red-400'
                            : 'text-content-secondary'
                      }
                    >
                      {line.kind === 'added' ? '+' : line.kind === 'removed' ? '-' : ' '}
                      {line.text}
                    </div>
                  ))}
                </pre>
              )}
            </div>
          )}
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
