import { useQuery } from '@tanstack/react-query';
import { apiFetch } from '../api/client';
import type { CounterJson, NodeCounterSetJson } from '../types/api';

export function useNodeCounters(nodeId: string) {
  return useQuery({
    queryKey: ['counters', nodeId],
    queryFn: () => apiFetch<CounterJson>(`/api/nodes/${nodeId}/counters`),
    enabled: !!nodeId,
  });
}

export function useAggregateCounters(label?: string) {
  return useQuery({
    queryKey: ['counters', 'aggregate', label],
    queryFn: () => {
      const params = label ? `?label=${encodeURIComponent(label)}` : '';
      return apiFetch<NodeCounterSetJson[]>(`/api/counters${params}`);
    },
  });
}
