import { useState } from 'react';
import DefinitionList from './DefinitionList';
import InstanceList from './InstanceList';

type Tab = 'definitions' | 'instances';

export default function FsmPage() {
  const [tab, setTab] = useState<Tab>('definitions');

  return (
    <div className="animate-fade-in">
      <div className="mb-6 rounded-2xl border border-edge/80 bg-surface-alt/80 p-3 md:p-4 md:sticky md:top-2 md:z-20">
        <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
          <div>
            <div className="text-[11px] uppercase tracking-[0.14em] text-content-muted">Orchestration</div>
            <h2 className="text-xl font-semibold tracking-tight">FSM Management</h2>
          </div>
          <div className="flex gap-1 bg-surface-alt border border-edge rounded-xl p-1 w-fit">
            <button
              onClick={() => setTab('definitions')}
              className={`px-4 py-2 text-sm rounded-lg transition-colors ${
                tab === 'definitions' ? 'bg-accent text-white' : 'text-content-secondary hover:text-content'
              }`}
            >
              Definitions
            </button>
            <button
              onClick={() => setTab('instances')}
              className={`px-4 py-2 text-sm rounded-lg transition-colors ${
                tab === 'instances' ? 'bg-accent text-white' : 'text-content-secondary hover:text-content'
              }`}
            >
              Instances
            </button>
          </div>
        </div>
      </div>

      {tab === 'definitions' ? <DefinitionList /> : <InstanceList />}
    </div>
  );
}
