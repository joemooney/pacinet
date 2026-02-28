import { useState } from 'react';
import DefinitionList from './DefinitionList';
import InstanceList from './InstanceList';

type Tab = 'definitions' | 'instances';

export default function FsmPage() {
  const [tab, setTab] = useState<Tab>('definitions');

  return (
    <div className="animate-fade-in">
      <div className="flex gap-1 mb-6 bg-surface-alt border border-edge rounded-xl p-1 w-fit">
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

      {tab === 'definitions' ? <DefinitionList /> : <InstanceList />}
    </div>
  );
}
