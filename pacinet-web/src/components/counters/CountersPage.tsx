import { useState } from 'react';
import { useNodes } from '../../hooks/useNodes';
import { useNodeCounters, useAggregateCounters } from '../../hooks/useCounters';
import { useCounterEvents } from '../../hooks/useEvents';
import Card from '../ui/Card';
import Table from '../ui/Table';
import Spinner from '../ui/Spinner';
import CounterRateChart from './CounterRateChart';

export default function CountersPage() {
  const [selectedNode, setSelectedNode] = useState('');
  const [labelFilter, setLabelFilter] = useState('');
  const { data: nodes } = useNodes();
  const { data: nodeCounters, isLoading: nodeLoading } = useNodeCounters(selectedNode);
  const { data: aggregateCounters, isLoading: aggLoading } = useAggregateCounters(
    labelFilter || undefined
  );

  // Live updates via SSE
  const counterEvents = useCounterEvents(selectedNode || undefined);
  const latestEvent = counterEvents[0];

  return (
    <div className="space-y-6 animate-fade-in">
      {/* Controls */}
      <div className="flex items-center gap-4">
        <div>
          <label className="block text-xs text-content-muted mb-1">Node</label>
          <select
            value={selectedNode}
            onChange={(e) => setSelectedNode(e.target.value)}
            className="px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content focus:outline-none focus:border-accent"
          >
            <option value="">All nodes (aggregate)</option>
            {nodes?.map((n) => (
              <option key={n.node_id} value={n.node_id}>
                {n.hostname}
              </option>
            ))}
          </select>
        </div>
        {!selectedNode && (
          <div>
            <label className="block text-xs text-content-muted mb-1">Label Filter</label>
            <input
              type="text"
              value={labelFilter}
              onChange={(e) => setLabelFilter(e.target.value)}
              placeholder="env=prod"
              className="px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent"
            />
          </div>
        )}
      </div>

      {/* Counter rate chart */}
      {counterEvents.length > 0 && (
        <Card title="Counter Rates (matches/s)">
          <CounterRateChart events={counterEvents} />
        </Card>
      )}

      {/* Single node counters */}
      {selectedNode && (
        <Card title={`Counters: ${nodes?.find((n) => n.node_id === selectedNode)?.hostname || selectedNode}`}>
          {nodeLoading ? (
            <Spinner />
          ) : !nodeCounters || nodeCounters.counters.length === 0 ? (
            <div className="text-sm text-content-muted py-4">No counters available</div>
          ) : (
            <Table headers={['Rule', 'Matches', 'Bytes']}>
              {nodeCounters.counters.map((c) => (
                <tr key={c.rule_name}>
                  <td className="px-4 py-2 font-mono text-xs">{c.rule_name}</td>
                  <td className="px-4 py-2 text-right">{c.match_count.toLocaleString()}</td>
                  <td className="px-4 py-2 text-right">{c.byte_count.toLocaleString()}</td>
                </tr>
              ))}
            </Table>
          )}

          {/* Live rates from SSE */}
          {latestEvent && latestEvent.counters.length > 0 && (
            <div className="mt-4">
              <h4 className="text-xs text-content-muted mb-2 uppercase">Live Rates</h4>
              <Table headers={['Rule', 'Matches/s', 'Bytes/s']}>
                {latestEvent.counters.map((c) => (
                  <tr key={c.rule_name}>
                    <td className="px-4 py-2 font-mono text-xs">{c.rule_name}</td>
                    <td className="px-4 py-2 text-right">{c.matches_per_second.toFixed(1)}</td>
                    <td className="px-4 py-2 text-right">{c.bytes_per_second.toFixed(1)}</td>
                  </tr>
                ))}
              </Table>
            </div>
          )}
        </Card>
      )}

      {/* Aggregate counters */}
      {!selectedNode && (
        <Card title="Aggregate Counters">
          {aggLoading ? (
            <Spinner />
          ) : !aggregateCounters || aggregateCounters.length === 0 ? (
            <div className="text-sm text-content-muted py-4">No counters available</div>
          ) : (
            <div className="space-y-4">
              {aggregateCounters.map((nc) => (
                <div key={nc.node_id}>
                  <h4 className="text-sm font-medium mb-2">
                    {nodes?.find((n) => n.node_id === nc.node_id)?.hostname || nc.node_id}
                  </h4>
                  <Table headers={['Rule', 'Matches', 'Bytes']}>
                    {nc.counters.map((c) => (
                      <tr key={c.rule_name}>
                        <td className="px-4 py-2 font-mono text-xs">{c.rule_name}</td>
                        <td className="px-4 py-2 text-right">{c.match_count.toLocaleString()}</td>
                        <td className="px-4 py-2 text-right">{c.byte_count.toLocaleString()}</td>
                      </tr>
                    ))}
                  </Table>
                </div>
              ))}
            </div>
          )}
        </Card>
      )}
    </div>
  );
}
