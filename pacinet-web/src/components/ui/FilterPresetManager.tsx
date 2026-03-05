import { useRef } from 'react';
import Button from './Button';
import type { FilterPresetManager as FilterPresetManagerType } from '../../hooks/useFilterPresets';

interface FilterPresetManagerProps {
  manager: FilterPresetManagerType;
  exportFilePrefix: string;
}

export default function FilterPresetManager({ manager, exportFilePrefix }: FilterPresetManagerProps) {
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleExport = () => {
    const payload = manager.exportPresets();
    const blob = new Blob([payload], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `${exportFilePrefix}-presets.json`;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  };

  const handleImportFile = async (file?: File) => {
    if (!file) return;

    try {
      const text = await file.text();
      const replace = window.confirm(
        'Import mode: press OK to replace existing presets, or Cancel to merge with existing presets.'
      );
      const result = manager.importPresets(text, replace ? 'replace' : 'merge');

      if (!result.ok) {
        window.alert(result.error ?? 'Import failed.');
        return;
      }

      window.alert(
        `${result.imported} preset(s) imported (${result.replaced ? 'replaced existing' : 'merged'}).`
      );
    } catch {
      window.alert('Unable to read preset file.');
    }
  };

  return (
    <div className="mt-3 flex flex-col gap-2 rounded-xl border border-edge bg-surface p-2 md:flex-row md:items-center">
      <div className="text-xs uppercase tracking-[0.12em] text-content-muted">Saved filters</div>
      <select
        value={manager.selectedName}
        onChange={(e) => manager.setSelectedName(e.target.value)}
        className="px-3 py-1.5 bg-surface-alt border border-edge rounded-lg text-sm text-content focus:outline-none focus:border-accent"
      >
        <option value="">Select preset...</option>
        {manager.names.map((name) => (
          <option key={name} value={name}>
            {name}
          </option>
        ))}
      </select>
      <input
        value={manager.draftName}
        onChange={(e) => manager.setDraftName(e.target.value)}
        placeholder="Preset name"
        className="px-3 py-1.5 bg-surface-alt border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent"
      />

      <input
        ref={fileInputRef}
        type="file"
        accept="application/json,.json"
        className="hidden"
        onChange={(e) => {
          void handleImportFile(e.target.files?.[0]);
          e.currentTarget.value = '';
        }}
      />

      <div className="md:ml-auto flex items-center gap-2">
        <Button size="sm" variant="secondary" onClick={manager.applySelected} disabled={!manager.selectedName}>
          Load
        </Button>
        <Button size="sm" onClick={manager.saveCurrent} disabled={!manager.draftName.trim()}>
          Save
        </Button>
        <Button size="sm" variant="ghost" onClick={manager.deleteSelected} disabled={!manager.selectedName}>
          Delete
        </Button>
        <Button size="sm" variant="secondary" onClick={handleExport}>
          Export
        </Button>
        <Button size="sm" variant="secondary" onClick={() => fileInputRef.current?.click()}>
          Import
        </Button>
      </div>
    </div>
  );
}
