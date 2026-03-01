import { useFsmInstance } from '../../hooks/useFsm';
import { useWebhookDeliveries } from '../../hooks/useWebhooks';
import { formatTimestamp, statusColorClass } from '../../lib/utils';
import Badge from '../ui/Badge';
import Card from '../ui/Card';
import Spinner from '../ui/Spinner';
import Table from '../ui/Table';

interface InstanceDetailProps {
  instanceId: string;
  onClose: () => void;
}

export default function InstanceDetail({ instanceId, onClose }: InstanceDetailProps) {
  const { data: instance, isLoading } = useFsmInstance(instanceId);
  const { data: webhookDeliveries } = useWebhookDeliveries(instanceId);

  return (
    <div className="fixed inset-y-0 right-0 w-[480px] bg-surface-alt border-l border-edge shadow-xl overflow-y-auto animate-slide-in-right z-50">
      <div className="flex items-center justify-between p-4 border-b border-edge">
        <h2 className="text-sm font-semibold">FSM Instance</h2>
        <button onClick={onClose} className="text-content-muted hover:text-content text-lg">&times;</button>
      </div>
      <div className="p-4">
        {isLoading ? (
          <Spinner />
        ) : !instance ? (
          <p className="text-content-muted text-sm">Instance not found</p>
        ) : (
          <div className="space-y-4">
            <Card title="Instance Info">
              <div className="grid grid-cols-2 gap-3 text-sm">
                <div>
                  <div className="text-xs text-content-muted">ID</div>
                  <div className="font-mono text-xs">{instance.instance_id}</div>
                </div>
                <div>
                  <div className="text-xs text-content-muted">Definition</div>
                  <div>{instance.definition_name}</div>
                </div>
                <div>
                  <div className="text-xs text-content-muted">Current State</div>
                  <div className="font-mono">{instance.current_state}</div>
                </div>
                <div>
                  <div className="text-xs text-content-muted">Status</div>
                  <Badge className={statusColorClass(instance.status)}>{instance.status}</Badge>
                </div>
                <div>
                  <div className="text-xs text-content-muted">Created</div>
                  <div className="text-xs">{formatTimestamp(instance.created_at)}</div>
                </div>
                <div>
                  <div className="text-xs text-content-muted">Updated</div>
                  <div className="text-xs">{formatTimestamp(instance.updated_at)}</div>
                </div>
              </div>
              {instance.target_nodes > 0 && (
                <div className="mt-3 text-sm">
                  <span className="text-content-muted">Progress: </span>
                  <span className="text-emerald-400">{instance.deployed_nodes} deployed</span>
                  {instance.failed_nodes > 0 && (
                    <span className="text-red-400"> / {instance.failed_nodes} failed</span>
                  )}
                  <span className="text-content-muted"> of {instance.target_nodes}</span>
                </div>
              )}
            </Card>

            <Card title="Transition History">
              <div className="relative">
                {instance.history.map((t, i) => (
                  <div key={i} className="flex gap-3 pb-4 relative">
                    {/* Timeline line */}
                    {i < instance.history.length - 1 && (
                      <div className="absolute left-[7px] top-4 bottom-0 w-px bg-edge" />
                    )}
                    {/* Dot */}
                    <div className="w-4 h-4 rounded-full bg-accent/30 border-2 border-accent flex-shrink-0 mt-0.5" />
                    <div className="flex-1">
                      <div className="flex items-center gap-2 text-sm">
                        {t.from_state ? (
                          <>
                            <span className="font-mono text-content-muted">{t.from_state}</span>
                            <span className="text-content-muted">&rarr;</span>
                            <span className="font-mono font-medium">{t.to_state}</span>
                          </>
                        ) : (
                          <span className="font-mono font-medium">{t.to_state}</span>
                        )}
                        <Badge className="bg-surface-hover text-content-muted">{t.trigger}</Badge>
                      </div>
                      {t.message && (
                        <div className="text-xs text-content-secondary mt-0.5">{t.message}</div>
                      )}
                      <div className="text-xs text-content-muted mt-0.5">{formatTimestamp(t.timestamp)}</div>
                    </div>
                  </div>
                ))}
              </div>
            </Card>

            {webhookDeliveries && webhookDeliveries.length > 0 && (
              <Card title="Webhook Deliveries">
                <div className="overflow-x-auto">
                  <Table headers={['Time', 'URL', 'Status', 'Duration', 'Result']}>
                    {webhookDeliveries.map((d) => (
                      <tr key={d.id}>
                        <td className="px-3 py-2 text-xs text-content-muted whitespace-nowrap">{formatTimestamp(d.timestamp)}</td>
                        <td className="px-3 py-2 text-xs font-mono max-w-[150px] truncate">{d.url}</td>
                        <td className="px-3 py-2 text-xs">{d.status_code || '-'}</td>
                        <td className="px-3 py-2 text-xs text-right">{d.duration_ms}ms</td>
                        <td className="px-3 py-2">
                          <Badge className={d.success ? 'bg-emerald-500/20 text-emerald-400' : 'bg-red-500/20 text-red-400'}>
                            {d.success ? 'OK' : d.error || 'Failed'}
                          </Badge>
                        </td>
                      </tr>
                    ))}
                  </Table>
                </div>
              </Card>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
