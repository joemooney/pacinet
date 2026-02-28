import { useState } from 'react';
import { setApiKey } from '../../api/client';

interface ApiKeyPromptProps {
  onSubmit: () => void;
  onDismiss: () => void;
}

export default function ApiKeyPrompt({ onSubmit, onDismiss }: ApiKeyPromptProps) {
  const [key, setKey] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (key.trim()) {
      setApiKey(key.trim());
      onSubmit();
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-surface border border-edge rounded-xl p-6 w-96 shadow-xl">
        <h2 className="text-lg font-semibold mb-2">Authentication Required</h2>
        <p className="text-sm text-content-secondary mb-4">
          This PaciNet instance requires an API key. Enter your key to continue.
        </p>
        <form onSubmit={handleSubmit}>
          <input
            type="password"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder="API key"
            autoFocus
            className="w-full px-3 py-2 bg-surface-alt border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent mb-4"
          />
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={onDismiss}
              className="px-4 py-2 text-sm text-content-secondary hover:text-content rounded-lg hover:bg-surface-hover transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!key.trim()}
              className="px-4 py-2 text-sm bg-accent text-white rounded-lg hover:bg-accent/90 transition-colors disabled:opacity-50"
            >
              Connect
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
