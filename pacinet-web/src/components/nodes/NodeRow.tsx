import type { NodeJson } from '../../types/api';
import { stateColorClass, formatAge, shortId } from '../../lib/utils';
import Badge from '../ui/Badge';

interface NodeRowProps {
  node: NodeJson;
  onClick: () => void;
  selected: boolean;
  onSelectToggle: (nodeId: string, checked: boolean) => void;
}

export default function NodeRow({ node, onClick, selected, onSelectToggle }: NodeRowProps) {
  const labels = Object.entries(node.labels);

  return (
    <tr
      className="hover:bg-surface-hover cursor-pointer transition-colors"
      onClick={onClick}
    >
      <td className="px-4 py-3" onClick={(e) => e.stopPropagation()}>
        <input
          type="checkbox"
          checked={selected}
          onChange={(e) => onSelectToggle(node.node_id, e.target.checked)}
          className="h-4 w-4 rounded border-edge bg-surface accent-accent"
          aria-label={`Select node ${node.hostname}`}
        />
      </td>
      <td className="px-4 py-3 font-medium">{node.hostname}</td>
      <td className="px-4 py-3">
        <Badge className={stateColorClass(node.state)}>{node.state}</Badge>
      </td>
      <td className="px-4 py-3">
        <div className="flex gap-1 flex-wrap">
          {labels.map(([k, v]) => (
            <span key={k} className="text-xs bg-surface-hover px-1.5 py-0.5 rounded text-content-secondary">
              {k}={v}
            </span>
          ))}
        </div>
      </td>
      <td className="px-4 py-3 font-mono text-xs text-content-muted">
        {node.policy_hash ? shortId(node.policy_hash) : '-'}
      </td>
      <td className="px-4 py-3 text-content-secondary">
        {formatAge(node.last_heartbeat_age_seconds)}
      </td>
      <td className="px-4 py-3 text-content-secondary font-mono text-xs">
        {shortId(node.node_id)}
      </td>
    </tr>
  );
}
