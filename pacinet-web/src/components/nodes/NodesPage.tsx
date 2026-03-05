import { useState, useMemo, useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { LayoutGrid, List } from 'lucide-react';
import { useNodes, useRemoveNode } from '../../hooks/useNodes';
import { useFilterPresets } from '../../hooks/useFilterPresets';
import Table from '../ui/Table';
import Spinner from '../ui/Spinner';
import NodeRow from './NodeRow';
import NodeDetail from './NodeDetail';
import NodeGrid from './NodeGrid';
import Button from '../ui/Button';
import FilterPresetManager from '../ui/FilterPresetManager';
import type { NodeJson } from '../../types/api';
import { apiFetch } from '../../api/client';

type ViewMode = 'table' | 'grid';

interface NodesPreset {
  labelFilter: string;
  statusFilter: string;
  viewMode: ViewMode;
}

export default function NodesPage() {
  const [labelFilter, setLabelFilter] = useState('');
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>('table');
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('asc');
  const [statusFilter, setStatusFilter] = useState('all');
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const queryClient = useQueryClient();
  const removeNode = useRemoveNode();
  const { data: nodes, isLoading } = useNodes(labelFilter || undefined);

  const presets = useFilterPresets<NodesPreset>(
    'pacinet_filters_nodes',
    { labelFilter, statusFilter, viewMode },
    (preset) => {
      setLabelFilter(preset.labelFilter ?? '');
      setStatusFilter(preset.statusFilter ?? 'all');
      setViewMode(preset.viewMode ?? 'table');
    }
  );

  const handleSort = useCallback((column: string, direction: 'asc' | 'desc') => {
    setSortCol(column);
    setSortDir(direction);
  }, []);

  const filteredNodes = useMemo(() => {
    if (!nodes) return nodes;
    if (statusFilter === 'all') return nodes;
    return nodes.filter((n) => n.state.toLowerCase() === statusFilter);
  }, [nodes, statusFilter]);

  const sortedNodes = useMemo(() => {
    if (!filteredNodes || !sortCol) return filteredNodes;
    const colMap: Record<string, keyof NodeJson> = {
      Hostname: 'hostname',
      State: 'state',
      Policy: 'policy_hash',
      Heartbeat: 'last_heartbeat_age_seconds',
      ID: 'node_id',
    };
    const key = colMap[sortCol];
    if (!key) return filteredNodes;

    return [...filteredNodes].sort((a, b) => {
      const av = a[key];
      const bv = b[key];
      if (typeof av === 'number' && typeof bv === 'number') {
        return sortDir === 'asc' ? av - bv : bv - av;
      }
      const as = String(av);
      const bs = String(bv);
      return sortDir === 'asc' ? as.localeCompare(bs) : bs.localeCompare(as);
    });
  }, [filteredNodes, sortCol, sortDir]);

  const selectedCount = selectedIds.length;
  const visibleIds = sortedNodes?.map((n) => n.node_id) ?? [];
  const allVisibleSelected =
    visibleIds.length > 0 && visibleIds.every((id) => selectedIds.includes(id));

  const handleToggleSelect = useCallback((nodeId: string, checked: boolean) => {
    setSelectedIds((prev) =>
      checked ? [...new Set([...prev, nodeId])] : prev.filter((id) => id !== nodeId)
    );
  }, []);

  const handleToggleSelectAll = useCallback(
    (checked: boolean) => {
      if (checked) {
        setSelectedIds((prev) => [...new Set([...prev, ...visibleIds])]);
        return;
      }
      setSelectedIds((prev) => prev.filter((id) => !visibleIds.includes(id)));
    },
    [visibleIds]
  );

  const handleBulkRemove = useCallback(async () => {
    if (selectedIds.length === 0) return;
    if (!confirm(`Remove ${selectedIds.length} selected node(s)?`)) return;

    const results = await Promise.allSettled(
      selectedIds.map((id) => apiFetch(`/api/nodes/${id}`, { method: 'DELETE' }))
    );

    const failed = results.filter((r) => r.status === 'rejected').length;
    setSelectedIds([]);
    await queryClient.invalidateQueries({ queryKey: ['nodes'] });
    await queryClient.invalidateQueries({ queryKey: ['fleet'] });

    if (failed > 0) {
      window.alert(`${failed} node removal request(s) failed.`);
    }
  }, [queryClient, selectedIds]);

  return (
    <div className="animate-fade-in">
      <div className="mb-4 rounded-2xl border border-edge/80 bg-surface-alt/80 p-3 md:p-4 md:sticky md:top-2 md:z-20">
        <div className="flex flex-col gap-3 md:flex-row md:items-center md:gap-4">
          <input
            type="text"
            placeholder="Filter by label (e.g. env=prod)"
            value={labelFilter}
            onChange={(e) => setLabelFilter(e.target.value)}
            className="px-3 py-2.5 bg-surface border border-edge rounded-xl text-sm text-content placeholder:text-content-muted w-full md:w-80 focus:outline-none focus:border-accent"
          />
          <span className="text-sm text-content-muted md:ml-1">
            {nodes?.length ?? 0} nodes
          </span>
          <select
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value)}
            className="px-3 py-2.5 bg-surface border border-edge rounded-xl text-sm text-content focus:outline-none focus:border-accent"
          >
            <option value="all">All states</option>
            <option value="online">Online</option>
            <option value="active">Active</option>
            <option value="deploying">Deploying</option>
            <option value="error">Error</option>
            <option value="offline">Offline</option>
            <option value="registered">Registered</option>
          </select>
          <div className="md:ml-auto flex items-center gap-1 rounded-xl border border-edge bg-surface p-1">
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

        <FilterPresetManager manager={presets} exportFilePrefix="pacinet-nodes-filters" />

        {selectedCount > 0 && (
          <div className="mt-3 flex items-center gap-3 rounded-xl border border-edge bg-surface px-3 py-2">
            <span className="text-sm text-content-secondary">{selectedCount} selected</span>
            <Button variant="danger" size="sm" onClick={handleBulkRemove} disabled={removeNode.isPending}>
              Remove Selected
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setSelectedIds([])}>
              Clear
            </Button>
          </div>
        )}
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
            headers={['', 'Hostname', 'State', 'Labels', 'Policy', 'Heartbeat', 'ID']}
            sortable
            onSort={handleSort}
          >
            <tr className="border-b border-edge bg-surface-hover/40">
              <td className="px-4 py-2.5">
                <input
                  type="checkbox"
                  checked={allVisibleSelected}
                  onChange={(e) => handleToggleSelectAll(e.target.checked)}
                  className="h-4 w-4 rounded border-edge bg-surface accent-accent"
                  aria-label="Select all visible nodes"
                />
              </td>
              <td colSpan={6} className="px-4 py-2.5 text-xs text-content-muted uppercase tracking-[0.12em]">
                Visible nodes
              </td>
            </tr>
            {sortedNodes.map((node) => (
              <NodeRow
                key={node.node_id}
                node={node}
                onClick={() => setSelectedNode(node.node_id)}
                selected={selectedIds.includes(node.node_id)}
                onSelectToggle={handleToggleSelect}
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
