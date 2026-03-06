import { useState } from 'react';
import { useTemplates, useCreateTemplate, useDeleteTemplate } from '../../hooks/useTemplates';
import { formatTimestamp } from '../../lib/utils';
import Card from '../ui/Card';
import Button from '../ui/Button';
import Badge from '../ui/Badge';
import Spinner from '../ui/Spinner';

export default function TemplatesPage() {
  const suggestedTags = ['baseline', 'allowlist', 'l2', 'ipv4', 'ipv6', 'observability', 'security'];
  const [tag, setTag] = useState('');
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [tags, setTags] = useState('');
  const [rulesYaml, setRulesYaml] = useState('');

  const { data: templates, isLoading } = useTemplates(tag || undefined);
  const createTemplate = useCreateTemplate();
  const deleteTemplate = useDeleteTemplate();

  const handleCreate = () => {
    if (!name || !rulesYaml) return;
    const tagList = tags ? tags.split(',').map((s) => s.trim()).filter(Boolean) : [];
    createTemplate.mutate(
      { name, description, rules_yaml: rulesYaml, tags: tagList },
      {
        onSuccess: () => {
          setShowCreate(false);
          setName('');
          setDescription('');
          setTags('');
          setRulesYaml('');
        },
      },
    );
  };

  const handleDelete = (templateName: string) => {
    if (confirm(`Delete template "${templateName}"?`)) {
      deleteTemplate.mutate(templateName);
    }
  };

  const addSuggestedTag = (tagName: string) => {
    const existing = tags
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean);
    if (existing.includes(tagName)) return;
    setTags(existing.length > 0 ? `${existing.join(', ')}, ${tagName}` : tagName);
  };

  return (
    <div className="animate-fade-in space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex gap-3 items-end">
          <div>
            <label className="block text-xs text-content-muted mb-1">Filter by tag</label>
            <input
              type="text"
              value={tag}
              onChange={(e) => setTag(e.target.value)}
              placeholder="e.g. firewall"
              className="px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent"
            />
          </div>
        </div>
        <Button onClick={() => setShowCreate(!showCreate)}>
          {showCreate ? 'Cancel' : 'New Template'}
        </Button>
      </div>

      {showCreate && (
        <Card title="Create Template">
          <div className="space-y-3">
            <div>
              <label className="block text-xs text-content-muted mb-1">Name</label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="template-name"
                className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent"
              />
            </div>
            <div>
              <label className="block text-xs text-content-muted mb-1">Description</label>
              <input
                type="text"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="What this template does"
                className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent"
              />
            </div>
            <div>
              <label className="block text-xs text-content-muted mb-1">Tags (comma-separated)</label>
              <input
                type="text"
                value={tags}
                onChange={(e) => setTags(e.target.value)}
                placeholder="firewall, production"
                className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm text-content placeholder:text-content-muted focus:outline-none focus:border-accent"
              />
              <div className="mt-2 flex flex-wrap gap-1">
                {suggestedTags.map((st) => (
                  <button
                    key={st}
                    type="button"
                    onClick={() => addSuggestedTag(st)}
                    className="text-[10px] px-2 py-0.5 rounded bg-surface-hover text-content-secondary hover:text-content"
                  >
                    + {st}
                  </button>
                ))}
              </div>
            </div>
            <div>
              <label className="block text-xs text-content-muted mb-1">Rules YAML</label>
              <textarea
                value={rulesYaml}
                onChange={(e) => setRulesYaml(e.target.value)}
                rows={10}
                placeholder={`pacgate:
  version: "1.0"
  defaults:
    action: drop
  rules:
    - name: allow_https_\${SITE}
      type: stateless
      priority: 100
      match:
        ethertype: "0x0800"
      action: pass`}
                className="w-full px-3 py-2 bg-surface border border-edge rounded-lg text-sm font-mono text-content placeholder:text-content-muted focus:outline-none focus:border-accent resize-y"
              />
              <p className="mt-2 text-xs text-content-muted">
                Use variables like <code>{'${SITE}'}</code> or <code>{'${CIDR}'}</code> for reusable templates.
              </p>
            </div>
            <Button onClick={handleCreate} disabled={createTemplate.isPending || !name || !rulesYaml}>
              {createTemplate.isPending ? 'Creating...' : 'Create Template'}
            </Button>
            {createTemplate.error && (
              <p className="text-sm text-red-400">{(createTemplate.error as Error).message}</p>
            )}
          </div>
        </Card>
      )}

      <Card title={`Templates${templates ? ` (${templates.length})` : ''}`}>
        {isLoading ? (
          <Spinner />
        ) : !templates || templates.length === 0 ? (
          <p className="text-sm text-content-muted">No templates found</p>
        ) : (
          <div className="space-y-3">
            {templates.map((t) => (
              <div
                key={t.name}
                className="flex items-start justify-between gap-4 p-3 rounded-lg border border-edge bg-surface"
              >
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="font-mono font-medium text-sm">{t.name}</span>
                    {t.tags.map((tag) => (
                      <Badge key={tag} className="bg-accent/20 text-accent text-[10px]">{tag}</Badge>
                    ))}
                  </div>
                  {t.description && (
                    <p className="text-xs text-content-secondary mt-1">{t.description}</p>
                  )}
                  <div className="text-xs text-content-muted mt-1">
                    Created: {formatTimestamp(t.created_at)}
                  </div>
                </div>
                <button
                  onClick={() => handleDelete(t.name)}
                  disabled={deleteTemplate.isPending}
                  className="text-xs text-red-400 hover:text-red-300 transition-colors"
                >
                  Delete
                </button>
              </div>
            ))}
          </div>
        )}
      </Card>
    </div>
  );
}
