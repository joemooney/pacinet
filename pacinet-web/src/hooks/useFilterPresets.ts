import { useEffect, useMemo, useState } from 'react';

interface StoredPresets<T> {
  presets: Record<string, T>;
}

export interface FilterPresetImportResult {
  ok: boolean;
  imported: number;
  replaced: boolean;
  error?: string;
}

export interface FilterPresetManager {
  names: string[];
  selectedName: string;
  setSelectedName: (name: string) => void;
  draftName: string;
  setDraftName: (name: string) => void;
  saveCurrent: () => void;
  applySelected: () => void;
  deleteSelected: () => void;
  exportPresets: () => string;
  importPresets: (jsonText: string, mode?: 'merge' | 'replace') => FilterPresetImportResult;
}

function loadPresets<T>(storageKey: string): Record<string, T> {
  try {
    const raw = localStorage.getItem(storageKey);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as StoredPresets<T>;
    return parsed.presets ?? {};
  } catch {
    return {};
  }
}

function parseIncomingPresets<T>(jsonText: string): Record<string, T> {
  const parsed = JSON.parse(jsonText) as unknown;

  if (typeof parsed !== 'object' || parsed === null) {
    throw new Error('Invalid preset payload.');
  }

  const withPresets = parsed as { presets?: unknown };
  const candidate = withPresets.presets ?? parsed;

  if (typeof candidate !== 'object' || candidate === null || Array.isArray(candidate)) {
    throw new Error('Preset payload must be an object map.');
  }

  return candidate as Record<string, T>;
}

export function useFilterPresets<T>(
  storageKey: string,
  currentValue: T,
  onApply: (value: T) => void
): FilterPresetManager {
  const [presets, setPresets] = useState<Record<string, T>>(() => loadPresets<T>(storageKey));
  const [selectedName, setSelectedName] = useState('');
  const [draftName, setDraftName] = useState('');

  useEffect(() => {
    localStorage.setItem(storageKey, JSON.stringify({ presets }));
  }, [presets, storageKey]);

  const names = useMemo(() => Object.keys(presets).sort((a, b) => a.localeCompare(b)), [presets]);

  const saveCurrent = () => {
    const name = draftName.trim();
    if (!name) return;
    setPresets((prev) => ({ ...prev, [name]: currentValue }));
    setSelectedName(name);
    setDraftName('');
  };

  const applySelected = () => {
    if (!selectedName) return;
    const preset = presets[selectedName];
    if (!preset) return;
    onApply(preset);
  };

  const deleteSelected = () => {
    if (!selectedName) return;
    setPresets((prev) => {
      const next = { ...prev };
      delete next[selectedName];
      return next;
    });
    setSelectedName('');
  };

  const exportPresets = () =>
    JSON.stringify(
      {
        version: 1,
        exported_at: new Date().toISOString(),
        storage_key: storageKey,
        presets,
      },
      null,
      2
    );

  const importPresets = (jsonText: string, mode: 'merge' | 'replace' = 'merge'): FilterPresetImportResult => {
    try {
      const incoming = parseIncomingPresets<T>(jsonText);
      const imported = Object.keys(incoming).length;

      if (mode === 'replace') {
        setPresets(incoming);
      } else {
        setPresets((prev) => ({ ...prev, ...incoming }));
      }

      return { ok: true, imported, replaced: mode === 'replace' };
    } catch (error) {
      return {
        ok: false,
        imported: 0,
        replaced: false,
        error: error instanceof Error ? error.message : 'Failed to import presets.',
      };
    }
  };

  return {
    names,
    selectedName,
    setSelectedName,
    draftName,
    setDraftName,
    saveCurrent,
    applySelected,
    deleteSelected,
    exportPresets,
    importPresets,
  };
}
