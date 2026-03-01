import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../api/client';
import type { PolicyTemplateSummaryJson, PolicyTemplateJson, CreateTemplateResponse, SuccessResponse } from '../types/api';

export function useTemplates(tag?: string) {
  return useQuery({
    queryKey: ['templates', tag],
    queryFn: () => {
      const params = tag ? `?tag=${encodeURIComponent(tag)}` : '';
      return apiFetch<PolicyTemplateSummaryJson[]>(`/api/templates${params}`);
    },
  });
}

export function useTemplate(name: string) {
  return useQuery({
    queryKey: ['template', name],
    queryFn: () => apiFetch<PolicyTemplateJson>(`/api/templates/${encodeURIComponent(name)}`),
    enabled: !!name,
  });
}

export function useCreateTemplate() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: { name: string; description: string; rules_yaml: string; tags: string[] }) =>
      apiFetch<CreateTemplateResponse>('/api/templates', {
        method: 'POST',
        body: JSON.stringify(params),
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['templates'] }),
  });
}

export function useDeleteTemplate() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (name: string) =>
      apiFetch<SuccessResponse>(`/api/templates/${encodeURIComponent(name)}`, { method: 'DELETE' }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['templates'] }),
  });
}
