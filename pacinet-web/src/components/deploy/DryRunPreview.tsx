import type { DryRunResultJson } from '../../types/api';
import { shortId } from '../../lib/utils';
import Card from '../ui/Card';
import Badge from '../ui/Badge';
import Table from '../ui/Table';

interface DryRunPreviewProps {
  result: DryRunResultJson;
}

export default function DryRunPreview({ result }: DryRunPreviewProps) {
  return (
    <Card title="Dry-Run Preview">
      <div className="flex items-center gap-2 mb-3">
        <Badge className={result.valid ? 'bg-emerald-500/20 text-emerald-400' : 'bg-red-500/20 text-red-400'}>
          {result.valid ? 'Valid' : 'Invalid'}
        </Badge>
        <span className="text-sm text-content-secondary">
          {result.target_nodes.length} target node{result.target_nodes.length !== 1 ? 's' : ''}
        </span>
      </div>

      {result.validation_errors.length > 0 && (
        <div className="mb-3 p-2 rounded-lg bg-red-500/10 border border-red-500/20">
          <div className="text-xs text-red-400 font-medium mb-1">Validation Errors:</div>
          {result.validation_errors.map((err, i) => (
            <div key={i} className="text-xs text-red-300">{err}</div>
          ))}
        </div>
      )}

      {result.target_nodes.length > 0 && (
        <div className="overflow-x-auto">
          <Table headers={['Node', 'Hostname', 'Current Hash', 'New Hash', 'Changed']}>
            {result.target_nodes.map((n) => (
              <tr key={n.node_id}>
                <td className="px-4 py-2 font-mono text-xs">{shortId(n.node_id)}</td>
                <td className="px-4 py-2 text-sm">{n.hostname}</td>
                <td className="px-4 py-2 font-mono text-xs text-content-muted">{n.current_policy_hash ? shortId(n.current_policy_hash) : '-'}</td>
                <td className="px-4 py-2 font-mono text-xs">{shortId(n.new_policy_hash)}</td>
                <td className="px-4 py-2">
                  <Badge className={n.policy_changed ? 'bg-amber-500/20 text-amber-400' : 'bg-surface-hover text-content-muted'}>
                    {n.policy_changed ? 'changed' : 'unchanged'}
                  </Badge>
                </td>
              </tr>
            ))}
          </Table>
        </div>
      )}
    </Card>
  );
}
