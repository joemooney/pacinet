import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { apiFetch } from '../../api/client';
import { useRemoveNode } from '../../hooks/useNodes';
import { useSetAnnotations } from '../../hooks/useAnnotations';
import { formatTimestamp, formatDuration, stateColorClass, statusColorClass, shortId } from '../../lib/utils';
import type { NodeJson, PolicyJson, CounterJson, DeploymentJson } from '../../types/api';
import Badge from '../ui/Badge';
import Button from '../ui/Button';
import Card from '../ui/Card';
import Spinner from '../ui/Spinner';
import Table from '../ui/Table';

interface NodeDetailProps {
  nodeId: string;
  onClose: () => void;
}

export default function NodeDetail({ nodeId, onClose }: NodeDetailProps) {
  const { data: node, isLoading } = useQuery({
    queryKey: ['node', nodeId],
    queryFn: () => apiFetch<NodeJson>(`/api/nodes/${nodeId}`),
  });

  const { data: policy } = useQuery({
    queryKey: ['node-policy', nodeId],
    queryFn: () => apiFetch<PolicyJson>(`/api/nodes/${nodeId}/policy`).catch(() => null),
  });

  const { data: counters } = useQuery({
    queryKey: ['node-counters', nodeId],
    queryFn: () => apiFetch<CounterJson>(`/api/nodes/${nodeId}/counters`),
  });

  const { data: deployments } = useQuery({
    queryKey: ['node-deployments', nodeId],
    queryFn: () => apiFetch<DeploymentJson[]>(`/api/nodes/${nodeId}/deploy/history?limit=10`),
  });

  const removeNode = useRemoveNode();

  const handleRemove = () => {
    if (confirm(`Remove node ${node?.hostname || nodeId}?`)) {
      removeNode.mutate(nodeId, { onSuccess: onClose });
    }
  };

  if (isLoading) return <DetailPanel onClose={onClose}><Spinner /></DetailPanel>;
  if (!node) return <DetailPanel onClose={onClose}><p>Node not found</p></DetailPanel>;

  return (
    <DetailPanel onClose={onClose}>
      <div className="space-y-4">
        {/* Node info */}
        <Card title="Node Info">
          <div className="grid grid-cols-2 gap-3 text-sm">
            <Field label="Hostname" value={node.hostname} />
            <Field label="State">
              <Badge className={stateColorClass(node.state)}>{node.state}</Badge>
            </Field>
            <Field label="Node ID" value={node.node_id} mono />
            <Field label="Agent" value={node.agent_address} mono />
            <Field label="PacGate" value={node.pacgate_version} />
            <Field label="Uptime" value={formatDuration(node.uptime_seconds)} />
            <Field label="Registered" value={formatTimestamp(node.registered_at)} />
            <Field label="Last Heartbeat" value={formatTimestamp(node.last_heartbeat)} />
          </div>
          {Object.keys(node.labels).length > 0 && (
            <div className="mt-3">
              <span className="text-xs text-content-muted">Labels:</span>
              <div className="flex gap-1 mt-1 flex-wrap">
                {Object.entries(node.labels).map(([k, v]) => (
                  <span key={k} className="text-xs bg-surface-hover px-2 py-0.5 rounded">
                    {k}={v}
                  </span>
                ))}
              </div>
            </div>
          )}
        </Card>

        {/* Annotations */}
        <AnnotationsSection nodeId={node.node_id} annotations={node.annotations || {}} />

        {/* Active policy */}
        {policy && (
          <Card title="Active Policy">
            <div className="text-xs text-content-muted mb-2">
              Hash: <span className="font-mono">{policy.policy_hash}</span> | Deployed: {formatTimestamp(policy.deployed_at)}
            </div>
            <pre className="bg-surface p-3 rounded-lg text-xs font-mono overflow-x-auto max-h-48 text-content-secondary">
              {policy.rules_yaml}
            </pre>
          </Card>
        )}

        {/* Counters */}
        {counters && counters.counters.length > 0 && (
          <Card title="Counters">
            <Table headers={['Rule', 'Matches', 'Bytes']}>
              {counters.counters.map((c) => (
                <tr key={c.rule_name}>
                  <td className="px-4 py-2 font-mono text-xs">{c.rule_name}</td>
                  <td className="px-4 py-2 text-right">{c.match_count.toLocaleString()}</td>
                  <td className="px-4 py-2 text-right">{c.byte_count.toLocaleString()}</td>
                </tr>
              ))}
            </Table>
          </Card>
        )}

        {/* Deploy history */}
        {deployments && deployments.length > 0 && (
          <Card title="Deploy History">
            <div className="space-y-2">
              {deployments.map((d) => (
                <div key={d.id} className="flex items-center gap-2 text-sm py-1">
                  <Badge className={statusColorClass(d.result)}>{d.result}</Badge>
                  <span className="text-content-muted text-xs">v{d.policy_version}</span>
                  <span className="font-mono text-xs text-content-muted">{shortId(d.policy_hash)}</span>
                  <span className="ml-auto text-xs text-content-muted">{formatTimestamp(d.deployed_at)}</span>
                </div>
              ))}
            </div>
          </Card>
        )}

        <Button variant="danger" onClick={handleRemove} disabled={removeNode.isPending}>
          {removeNode.isPending ? 'Removing...' : 'Remove Node'}
        </Button>
      </div>
    </DetailPanel>
  );
}

function DetailPanel({ children, onClose }: { children: React.ReactNode; onClose: () => void }) {
  return (
    <div className="fixed inset-y-0 right-0 w-[480px] bg-surface-alt border-l border-edge shadow-xl overflow-y-auto animate-slide-in-right z-50">
      <div className="flex items-center justify-between p-4 border-b border-edge">
        <h2 className="text-sm font-semibold">Node Details</h2>
        <button onClick={onClose} className="text-content-muted hover:text-content text-lg">&times;</button>
      </div>
      <div className="p-4">{children}</div>
    </div>
  );
}

function Field({ label, value, mono, children }: { label: string; value?: string; mono?: boolean; children?: React.ReactNode }) {
  return (
    <div>
      <div className="text-xs text-content-muted mb-0.5">{label}</div>
      {children || (
        <div className={`text-sm ${mono ? 'font-mono text-xs' : ''}`}>{value || '-'}</div>
      )}
    </div>
  );
}

function AnnotationsSection({ nodeId, annotations }: { nodeId: string; annotations: Record<string, string> }) {
  const [editing, setEditing] = useState(false);
  const [newKey, setNewKey] = useState('');
  const [newValue, setNewValue] = useState('');
  const setAnnotations = useSetAnnotations();

  const handleAdd = () => {
    if (!newKey) return;
    setAnnotations.mutate(
      { nodeId, annotations: { [newKey]: newValue }, remove_keys: [] },
      { onSuccess: () => { setNewKey(''); setNewValue(''); } },
    );
  };

  const handleRemove = (key: string) => {
    setAnnotations.mutate({ nodeId, annotations: {}, remove_keys: [key] });
  };

  const entries = Object.entries(annotations);

  return (
    <Card title="Annotations">
      {entries.length === 0 && !editing && (
        <p className="text-xs text-content-muted">No annotations</p>
      )}
      {entries.length > 0 && (
        <div className="flex flex-wrap gap-1 mb-2">
          {entries.map(([k, v]) => (
            <span key={k} className="inline-flex items-center gap-1 text-xs bg-surface-hover px-2 py-0.5 rounded">
              <span className="font-mono">{k}={v}</span>
              {editing && (
                <button onClick={() => handleRemove(k)} className="text-red-400 hover:text-red-300 ml-1">&times;</button>
              )}
            </span>
          ))}
        </div>
      )}
      {editing ? (
        <div className="flex gap-2 items-end mt-2">
          <input
            type="text"
            value={newKey}
            onChange={(e) => setNewKey(e.target.value)}
            placeholder="key"
            className="px-2 py-1 bg-surface border border-edge rounded text-xs text-content w-24 focus:outline-none focus:border-accent"
          />
          <input
            type="text"
            value={newValue}
            onChange={(e) => setNewValue(e.target.value)}
            placeholder="value"
            className="px-2 py-1 bg-surface border border-edge rounded text-xs text-content w-32 focus:outline-none focus:border-accent"
          />
          <button onClick={handleAdd} disabled={!newKey || setAnnotations.isPending} className="text-xs text-accent hover:underline">
            Add
          </button>
          <button onClick={() => setEditing(false)} className="text-xs text-content-muted hover:underline">
            Done
          </button>
        </div>
      ) : (
        <button onClick={() => setEditing(true)} className="text-xs text-accent hover:underline mt-1">
          Edit
        </button>
      )}
    </Card>
  );
}
