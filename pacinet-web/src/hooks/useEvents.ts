import { useState, useEffect, useCallback } from 'react';
import { getApiKey } from '../api/client';
import type { NodeEventJson, CounterEventJson, FsmEventJson, PersistentEventJson } from '../types/api';

const MAX_EVENTS = 100;

function sseUrl(path: string, params?: Record<string, string>): string {
  const url = new URL(path, window.location.origin);
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      if (v) url.searchParams.set(k, v);
    }
  }
  const key = getApiKey();
  if (key) url.searchParams.set('token', key);
  return url.toString();
}

export function useNodeEvents(label?: string) {
  const [events, setEvents] = useState<NodeEventJson[]>([]);

  useEffect(() => {
    const source = new EventSource(sseUrl('/api/events/nodes', label ? { label } : undefined));
    source.onmessage = (e) => {
      const event: NodeEventJson = JSON.parse(e.data);
      setEvents((prev) => [event, ...prev].slice(0, MAX_EVENTS));
    };
    return () => source.close();
  }, [label]);

  return events;
}

export function useCounterEvents(nodeId?: string) {
  const [events, setEvents] = useState<CounterEventJson[]>([]);

  useEffect(() => {
    const source = new EventSource(sseUrl('/api/events/counters', nodeId ? { node: nodeId } : undefined));
    source.onmessage = (e) => {
      const event: CounterEventJson = JSON.parse(e.data);
      setEvents((prev) => [event, ...prev].slice(0, MAX_EVENTS));
    };
    return () => source.close();
  }, [nodeId]);

  return events;
}

export function useFsmEvents(instanceId?: string) {
  const [events, setEvents] = useState<FsmEventJson[]>([]);

  useEffect(() => {
    const source = new EventSource(sseUrl('/api/events/fsm', instanceId ? { instance: instanceId } : undefined));
    source.onmessage = (e) => {
      const event: FsmEventJson = JSON.parse(e.data);
      setEvents((prev) => [event, ...prev].slice(0, MAX_EVENTS));
    };
    return () => source.close();
  }, [instanceId]);

  return events;
}

export function useEventHistory(params?: {
  type?: string;
  source?: string;
  since?: string;
  until?: string;
  limit?: number;
}) {
  const [events, setEvents] = useState<PersistentEventJson[]>([]);
  const [loading, setLoading] = useState(false);

  const fetchHistory = useCallback(async () => {
    setLoading(true);
    try {
      const url = new URL('/api/events/history', window.location.origin);
      if (params?.type) url.searchParams.set('type', params.type);
      if (params?.source) url.searchParams.set('source', params.source);
      if (params?.since) url.searchParams.set('since', params.since);
      if (params?.until) url.searchParams.set('until', params.until);
      if (params?.limit) url.searchParams.set('limit', String(params.limit));

      const headers: Record<string, string> = {};
      const key = getApiKey();
      if (key) headers['Authorization'] = `Bearer ${key}`;

      const res = await fetch(url, { headers });
      if (res.ok) {
        setEvents(await res.json());
      }
    } finally {
      setLoading(false);
    }
  }, [params?.type, params?.source, params?.since, params?.until, params?.limit]);

  useEffect(() => {
    fetchHistory();
  }, [fetchHistory]);

  return { events, loading, refetch: fetchHistory };
}
