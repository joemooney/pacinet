import { useQuery } from '@tanstack/react-query';
import { apiFetch } from '../api/client';
import type { AuditEntryJson } from '../types/api';

export function useAuditLog(params?: { action?: string; resource_type?: string; limit?: number }) {
  const action = params?.action || '';
  const resource_type = params?.resource_type || '';
  const limit = params?.limit || 50;

  return useQuery({
    queryKey: ['audit', action, resource_type, limit],
    queryFn: () => {
      const searchParams = new URLSearchParams();
      if (action) searchParams.set('action', action);
      if (resource_type) searchParams.set('resource_type', resource_type);
      searchParams.set('limit', limit.toString());
      return apiFetch<AuditEntryJson[]>(`/api/audit?${searchParams}`);
    },
  });
}
