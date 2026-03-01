import { useQuery } from '@tanstack/react-query';
import { apiFetch } from '../api/client';
import type { WebhookDeliveryJson } from '../types/api';

export function useWebhookDeliveries(instanceId?: string, limit = 50) {
  return useQuery({
    queryKey: ['webhook-deliveries', instanceId, limit],
    queryFn: () => {
      const params = new URLSearchParams();
      if (instanceId) params.set('instance_id', instanceId);
      params.set('limit', limit.toString());
      return apiFetch<WebhookDeliveryJson[]>(`/api/webhooks/history?${params}`);
    },
    enabled: !!instanceId,
  });
}
