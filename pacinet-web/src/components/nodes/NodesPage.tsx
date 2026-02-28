import { useState, useMemo, useCallback } from 'react';
import { LayoutGrid, List } from 'lucide-react';
import { useNodes } from '../../hooks/useNodes';
import Table from '../ui/Table';
import Spinner from '../ui/Spinner';
import NodeRow from './NodeRow';
import NodeDetail from './NodeDetail';
import NodeGrid from './NodeGrid';
import type { NodeJson } from '../../types/api';

type ViewMode = 'table' | 'grid';

export default function NodesPage() {
  const [labelFilter, setLabelFilter] = useState('');
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>('table');
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('asc');
  const { data: nodes, isLoading } = useNodes(labelFilter || undefined);

  const handleSort = useCallback((column: string, direction: 'asc' | 'desc') => {
    setSortCol(column);
    setSortDir(direction);
  }, []);

  const sortedNodes = useMemo(() => {
    if (!nodes || !sortCol) return nodes;
    const colMap: Record<string, keyof NodeJson> = {
      Hostname: 'hostname',
      State: 'state',
      Policy: 'policy_hash',
      Heartbeat: 'last_heartbeat_age_seconds',
      ID: 'node_id',
    };
    const key = colMap[sortCol];
    if (!key) return nodes;

    return [...nodes].sort((a, b) => {
      const av = a[key];
      const bv = b[key];
      if (typeof av === 'number' && typeof bv === 'number') {
        return sortDir === 'asc' ? av - bv : bv - av;
      }
      const as = String(av);
      const bs = String(bv);
      return sortDir === 'asc' ? as.localeCompare(bs) : bs.localeCompare(as);
    });
  }, [nodes, sortCol, sortDir]);

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
        <div className="ml-auto flex items-center gap-1">
          <button
            onClick={() => setViewMode('table')}
            className={`p-2 rounded-lg transition-colors ${viewMode === 'table' ? 'bg-surface-hover text-content' : 'text-content-muted hover:text-content'}`}
            title="Table view"
          >
            <List size={16} />
          </button>
          <button
            onClick={() => setViewMode('grid')}
            className={`p-2 rounded-lg transition-colors ${viewMode === 'grid' ? 'bg-surface-hover text-content' : 'text-content-muted hover:text-content'}`}
            title="Grid view"
          >
            <LayoutGrid size={16} />
          </button>
        </div>
      </div>

      {isLoading ? (
        <Spinner />
      ) : !sortedNodes || sortedNodes.length === 0 ? (
        <div className="text-content-muted text-sm py-8 text-center">
          No nodes registered. Start an agent to see nodes appear.
        </div>
      ) : viewMode === 'table' ? (
        <div className="bg-surface-alt border border-edge rounded-xl overflow-hidden">
          <Table
            headers={['Hostname', 'State', 'Labels', 'Policy', 'Heartbeat', 'ID']}
            sortable
            onSort={handleSort}
          >
            {sortedNodes.map((node) => (
              <NodeRow
                key={node.node_id}
                node={node}
                onClick={() => setSelectedNode(node.node_id)}
              />
            ))}
          </Table>
        </div>
      ) : (
        <NodeGrid nodes={sortedNodes} onSelect={(id) => setSelectedNode(id)} />
      )}

      {selectedNode && (
        <NodeDetail nodeId={selectedNode} onClose={() => setSelectedNode(null)} />
      )}
    </div>
  );
}
