import { useState } from 'react';
import { useNodes } from '../../hooks/useNodes';
import Table from '../ui/Table';
import Spinner from '../ui/Spinner';
import NodeRow from './NodeRow';
import NodeDetail from './NodeDetail';

export default function NodesPage() {
  const [labelFilter, setLabelFilter] = useState('');
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const { data: nodes, isLoading } = useNodes(labelFilter || undefined);

  return (
    <div className="animate-fade-in">
      <div className="flex items-center gap-4 mb-4">
        <input
          type="text"
          placeholder="Filter by label (e.g. env=prod)"
          value={labelFilter}
          onChange={(e) => setLabelFilter(e.target.value)}
          className="px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted w-72 focus:outline-none focus:border-accent"
        />
        <span className="text-sm text-content-muted">
          {nodes?.length ?? 0} nodes
        </span>
      </div>

      {isLoading ? (
        <Spinner />
      ) : !nodes || nodes.length === 0 ? (
        <div className="text-content-muted text-sm py-8 text-center">
          No nodes registered. Start an agent to see nodes appear.
        </div>
      ) : (
        <div className="bg-surface-alt border border-edge rounded-xl overflow-hidden">
          <Table headers={['Hostname', 'State', 'Labels', 'Policy', 'Heartbeat', 'ID']}>
            {nodes.map((node) => (
              <NodeRow
                key={node.node_id}
                node={node}
                onClick={() => setSelectedNode(node.node_id)}
              />
            ))}
          </Table>
        </div>
      )}

      {selectedNode && (
        <NodeDetail nodeId={selectedNode} onClose={() => setSelectedNode(null)} />
      )}
    </div>
  );
}
