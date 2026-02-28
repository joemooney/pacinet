import { useQuery } from '@tanstack/react-query';
import { apiFetch } from '../api/client';
import type { FleetStatusJson } from '../types/api';

export function useFleet(label?: string) {
  return useQuery({
    queryKey: ['fleet', label],
    queryFn: () => {
      const params = label ? `?label=${encodeURIComponent(label)}` : '';
      return apiFetch<FleetStatusJson>(`/api/fleet${params}`);
    },
  });
}
