import { useState } from 'react';
import { useAuditLog } from '../../hooks/useAudit';
import { formatTimestamp } from '../../lib/utils';
import Card from '../ui/Card';
import Spinner from '../ui/Spinner';
import Table from '../ui/Table';
import Badge from '../ui/Badge';

export default function AuditPage() {
  const [action, setAction] = useState('');
  const [resourceType, setResourceType] = useState('');
  const [limit, setLimit] = useState(50);

  const { data: entries, isLoading } = useAuditLog({
    action: action || undefined,
    resource_type: resourceType || undefined,
    limit,
  });

  return (
    <div className="animate-fade-in space-y-6">
      <Card title="Filters">
        <div className="flex flex-wrap gap-4 items-end">
          <div>
            <label className="block text-xs text-content-muted mb-1">Action</label>
            <select
              value={action}
              onChange={(e) => setAction(e.target.value)}
              className="px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content focus:outline-none focus:border-accent"
            >
              <option value="">All</option>
              <option value="deploy">deploy</option>
              <option value="batch_deploy">batch_deploy</option>
              <option value="remove_node">remove_node</option>
              <option value="set_annotations">set_annotations</option>
              <option value="create_fsm_definition">create_fsm_definition</option>
              <option value="delete_fsm_definition">delete_fsm_definition</option>
              <option value="start_fsm">start_fsm</option>
              <option value="advance_fsm">advance_fsm</option>
              <option value="cancel_fsm">cancel_fsm</option>
              <option value="create_template">create_template</option>
              <option value="delete_template">delete_template</option>
              <option value="rollback_policy">rollback_policy</option>
            </select>
          </div>
          <div>
            <label className="block text-xs text-content-muted mb-1">Resource Type</label>
            <select
              value={resourceType}
              onChange={(e) => setResourceType(e.target.value)}
              className="px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content focus:outline-none focus:border-accent"
            >
              <option value="">All</option>
              <option value="node">node</option>
              <option value="policy">policy</option>
              <option value="fsm_definition">fsm_definition</option>
              <option value="fsm_instance">fsm_instance</option>
              <option value="template">template</option>
            </select>
          </div>
          <div>
            <label className="block text-xs text-content-muted mb-1">Limit</label>
            <select
              value={limit}
              onChange={(e) => setLimit(Number(e.target.value))}
              className="px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content focus:outline-none focus:border-accent"
            >
              <option value={25}>25</option>
              <option value={50}>50</option>
              <option value={100}>100</option>
            </select>
          </div>
        </div>
      </Card>

      <Card title={`Audit Log${entries ? ` (${entries.length})` : ''}`}>
        {isLoading ? (
          <Spinner />
        ) : !entries || entries.length === 0 ? (
          <p className="text-sm text-content-muted">No audit entries found</p>
        ) : (
          <div className="overflow-x-auto">
            <Table headers={['Timestamp', 'Actor', 'Action', 'Type', 'Resource ID', 'Details']}>
              {entries.map((e) => (
                <tr key={e.id}>
                  <td className="px-4 py-2 text-xs text-content-muted whitespace-nowrap">{formatTimestamp(e.timestamp)}</td>
                  <td className="px-4 py-2 text-xs">
                    <Badge className="bg-surface-hover text-content-secondary">{e.actor}</Badge>
                  </td>
                  <td className="px-4 py-2 text-xs font-mono">{e.action}</td>
                  <td className="px-4 py-2 text-xs">{e.resource_type}</td>
                  <td className="px-4 py-2 text-xs font-mono text-content-muted">{e.resource_id.length > 12 ? e.resource_id.slice(0, 12) + '...' : e.resource_id}</td>
                  <td className="px-4 py-2 text-xs text-content-muted max-w-[200px] truncate">{e.details}</td>
                </tr>
              ))}
            </Table>
          </div>
        )}
      </Card>
    </div>
  );
}
