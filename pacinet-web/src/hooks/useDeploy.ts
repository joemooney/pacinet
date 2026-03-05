import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../api/client';
import type { DeployResponse, BatchDeployResultJson, DryRunDeployResponse } from '../types/api';

interface DeployParams {
  node_id: string;
  rules_yaml: string;
  counters: boolean;
  rate_limit: boolean;
  conntrack: boolean;
  axi: boolean;
  ports: number;
  target: string;
  dynamic: boolean;
  dynamic_entries: number;
  width: number;
  ptp: boolean;
  rss: boolean;
  rss_queues: number;
  int: boolean;
  int_switch_id: number;
}

interface BatchDeployParams {
  label_filter: Record<string, string>;
  rules_yaml: string;
  counters: boolean;
  rate_limit: boolean;
  conntrack: boolean;
  axi: boolean;
  ports: number;
  target: string;
  dynamic: boolean;
  dynamic_entries: number;
  width: number;
  ptp: boolean;
  rss: boolean;
  rss_queues: number;
  int: boolean;
  int_switch_id: number;
}

export function useDeployPolicy() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: DeployParams) =>
      apiFetch<DeployResponse>('/api/deploy', {
        method: 'POST',
        body: JSON.stringify(params),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['nodes'] });
      queryClient.invalidateQueries({ queryKey: ['fleet'] });
    },
  });
}

export function useBatchDeploy() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: BatchDeployParams) =>
      apiFetch<BatchDeployResultJson>('/api/deploy/batch', {
        method: 'POST',
        body: JSON.stringify(params),
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['nodes'] });
      queryClient.invalidateQueries({ queryKey: ['fleet'] });
    },
  });
}

export function useDryRunDeploy() {
  return useMutation({
    mutationFn: (params: DeployParams) =>
      apiFetch<DryRunDeployResponse>('/api/deploy', {
        method: 'POST',
        body: JSON.stringify({ ...params, dry_run: true }),
      }),
  });
}
