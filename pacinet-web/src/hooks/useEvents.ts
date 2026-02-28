import { useState, useEffect } from 'react';
import type { NodeEventJson, CounterEventJson, FsmEventJson } from '../types/api';

const MAX_EVENTS = 100;

export function useNodeEvents(label?: string) {
  const [events, setEvents] = useState<NodeEventJson[]>([]);

  useEffect(() => {
    const url = new URL('/api/events/nodes', window.location.origin);
    if (label) url.searchParams.set('label', label);

    const source = new EventSource(url);
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
    const url = new URL('/api/events/counters', window.location.origin);
    if (nodeId) url.searchParams.set('node', nodeId);

    const source = new EventSource(url);
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
    const url = new URL('/api/events/fsm', window.location.origin);
    if (instanceId) url.searchParams.set('instance', instanceId);

    const source = new EventSource(url);
    source.onmessage = (e) => {
      const event: FsmEventJson = JSON.parse(e.data);
      setEvents((prev) => [event, ...prev].slice(0, MAX_EVENTS));
    };
    return () => source.close();
  }, [instanceId]);

  return events;
}
