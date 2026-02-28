import type { NodeJson } from '../../types/api';
import Badge from '../ui/Badge';
import { stateColorClass, formatAge, shortId } from '../../lib/utils';

interface NodeGridProps {
  nodes: NodeJson[];
  onSelect: (nodeId: string) => void;
}

export default function NodeGrid({ nodes, onSelect }: NodeGridProps) {
  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
      {nodes.map((node) => (
        <div
          key={node.node_id}
          onClick={() => onSelect(node.node_id)}
          className="bg-surface-alt border border-edge rounded-xl p-4 cursor-pointer hover:border-accent/50 transition-colors"
        >
          <div className="flex items-center justify-between mb-2">
            <span className="font-medium text-sm truncate">{node.hostname}</span>
            <Badge className={stateColorClass(node.state)}>{node.state}</Badge>
          </div>
          <div className="space-y-1 text-xs text-content-secondary">
            <div className="flex justify-between">
              <span>Heartbeat</span>
              <span>{formatAge(node.last_heartbeat_age_seconds)}</span>
            </div>
            <div className="flex justify-between">
              <span>Policy</span>
              <span className="font-mono">{node.policy_hash ? node.policy_hash.slice(0, 8) : 'none'}</span>
            </div>
            <div className="flex justify-between">
              <span>Uptime</span>
              <span>{Math.floor(node.uptime_seconds / 3600)}h {Math.floor((node.uptime_seconds % 3600) / 60)}m</span>
            </div>
            <div className="flex justify-between">
              <span>ID</span>
              <span className="font-mono">{shortId(node.node_id)}</span>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
