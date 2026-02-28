import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../api/client';
import type {
  FsmDefSummaryJson,
  FsmDefJson,
  FsmInstanceJson,
  CreateFsmDefResponse,
  StartFsmResponse,
  AdvanceFsmResponse,
  SuccessResponse,
} from '../types/api';

export function useFsmDefinitions(kind?: string) {
  return useQuery({
    queryKey: ['fsm-definitions', kind],
    queryFn: () => {
      const params = kind ? `?kind=${encodeURIComponent(kind)}` : '';
      return apiFetch<FsmDefSummaryJson[]>(`/api/fsm/definitions${params}`);
    },
  });
}

export function useFsmDefinition(name: string) {
  return useQuery({
    queryKey: ['fsm-definition', name],
    queryFn: () => apiFetch<FsmDefJson>(`/api/fsm/definitions/${encodeURIComponent(name)}`),
    enabled: !!name,
  });
}

export function useCreateFsmDefinition() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (yaml: string) =>
      apiFetch<CreateFsmDefResponse>('/api/fsm/definitions', {
        method: 'POST',
        body: JSON.stringify({ definition_yaml: yaml }),
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['fsm-definitions'] }),
  });
}

export function useDeleteFsmDefinition() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (name: string) =>
      apiFetch<SuccessResponse>(`/api/fsm/definitions/${encodeURIComponent(name)}`, {
        method: 'DELETE',
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['fsm-definitions'] }),
  });
}

export function useFsmInstances(definition?: string, status?: string) {
  return useQuery({
    queryKey: ['fsm-instances', definition, status],
    queryFn: () => {
      const params = new URLSearchParams();
      if (definition) params.set('definition', definition);
      if (status) params.set('status', status);
      const q = params.toString();
      return apiFetch<FsmInstanceJson[]>(`/api/fsm/instances${q ? `?${q}` : ''}`);
    },
  });
}

export function useFsmInstance(id: string) {
  return useQuery({
    queryKey: ['fsm-instance', id],
    queryFn: () => apiFetch<FsmInstanceJson>(`/api/fsm/instances/${id}`),
    enabled: !!id,
  });
}

export function useStartFsm() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      definition_name: string;
      rules_yaml?: string;
      counters?: boolean;
      rate_limit?: boolean;
      conntrack?: boolean;
      target_label_filter?: Record<string, string>;
    }) =>
      apiFetch<StartFsmResponse>('/api/fsm/instances', {
        method: 'POST',
        body: JSON.stringify(params),
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['fsm-instances'] }),
  });
}

export function useAdvanceFsm() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, target_state }: { id: string; target_state?: string }) =>
      apiFetch<AdvanceFsmResponse>(`/api/fsm/instances/${id}/advance`, {
        method: 'POST',
        body: JSON.stringify({ target_state }),
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['fsm-instances'] }),
  });
}

export function useCancelFsm() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, reason }: { id: string; reason: string }) =>
      apiFetch<SuccessResponse>(`/api/fsm/instances/${id}/cancel`, {
        method: 'POST',
        body: JSON.stringify({ reason }),
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['fsm-instances'] }),
  });
}
