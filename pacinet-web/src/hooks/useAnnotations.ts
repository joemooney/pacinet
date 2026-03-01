import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../api/client';
import type { SuccessResponse } from '../types/api';

export function useSetAnnotations() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: { nodeId: string; annotations: Record<string, string>; remove_keys: string[] }) =>
      apiFetch<SuccessResponse>(`/api/nodes/${params.nodeId}/annotations`, {
        method: 'PUT',
        body: JSON.stringify({ annotations: params.annotations, remove_keys: params.remove_keys }),
      }),
    onSuccess: (_data, vars) => {
      queryClient.invalidateQueries({ queryKey: ['node', vars.nodeId] });
      queryClient.invalidateQueries({ queryKey: ['nodes'] });
    },
  });
}
