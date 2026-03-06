import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../api/client';
import type { NodeJson, SuccessResponse, RollbackResponse } from '../types/api';

export function useNodes(label?: string) {
  return useQuery({
    queryKey: ['nodes', label],
    queryFn: () => {
      const params = label ? `?label=${encodeURIComponent(label)}` : '';
      return apiFetch<NodeJson[]>(`/api/nodes${params}`);
    },
  });
}

export function useNode(id: string) {
  return useQuery({
    queryKey: ['node', id],
    queryFn: () => apiFetch<NodeJson>(`/api/nodes/${id}`),
    enabled: !!id,
  });
}

export function useRemoveNode() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => apiFetch<SuccessResponse>(`/api/nodes/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['nodes'] });
      queryClient.invalidateQueries({ queryKey: ['fleet'] });
    },
  });
}

export function useRollbackPolicy() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ nodeId, targetVersion }: { nodeId: string; targetVersion?: number }) =>
      apiFetch<RollbackResponse>(`/api/nodes/${nodeId}/policy/rollback`, {
        method: 'POST',
        body: JSON.stringify({
          target_version: targetVersion ?? 0,
        }),
      }),
    onSuccess: (_data, vars) => {
      queryClient.invalidateQueries({ queryKey: ['node', vars.nodeId] });
      queryClient.invalidateQueries({ queryKey: ['node-policy', vars.nodeId] });
      queryClient.invalidateQueries({ queryKey: ['node-deployments', vars.nodeId] });
      queryClient.invalidateQueries({ queryKey: ['nodes'] });
      queryClient.invalidateQueries({ queryKey: ['fleet'] });
    },
  });
}
