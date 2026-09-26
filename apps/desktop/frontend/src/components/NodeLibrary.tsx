import { useMemo, useState, type RefObject } from 'react';
import type { NodeDescriptor } from '../editor/types';
import { dataTypesCompatible } from '../editor/connections';
import { Icon } from '../ui/Icon';

interface NodeLibraryProps {
  descriptors: NodeDescriptor[];
  onAdd: (typeId: string) => void;
  selectedCount?: number;
  onCreateSubgraph?: () => void;
  searchInputRef?: RefObject<HTMLInputElement | null>;
  compatibleDataTypes?: string[];
  onCollapse?: () => void;
}

const CATEGORY_LABELS: Record<string, string> = { ai: 'AI', core: 'Core', pro: 'Pro Tools', raw: 'RAW' };

/** `core.exposure` -> `Core`; unknown families fall back to title case. */
function categoryLabel(category: string): string {
  return CATEGORY_LABELS[category]
    ?? category.replace(/[-_]+/g, ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());
}

export function NodeLibrary({
  descriptors,
  onAdd,
  selectedCount = 0,
  onCreateSubgraph,
  searchInputRef,
  compatibleDataTypes,
  onCollapse,
}: NodeLibraryProps) {
  const [query, setQuery] = useState('');
  const [compatibleOnly, setCompatibleOnly] = useState(true);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  const hasCompatibleTypes = Boolean(compatibleDataTypes && compatibleDataTypes.length > 0);
  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    const compatible = hasCompatibleTypes && compatibleOnly && compatibleDataTypes
      ? new Set(compatibleDataTypes)
      : null;
    return descriptors.filter((descriptor) => {
      if (normalized && !`${descriptor.name} ${descriptor.typeId}`.toLowerCase().includes(normalized)) return false;
      if (compatible && !descriptor.inputs.some((input) =>
        [...compatible].some((outputType) => dataTypesCompatible(input.dataType, outputType)))) return false;
      return true;
    });
  }, [compatibleDataTypes, compatibleOnly, descriptors, hasCompatibleTypes, query]);
  // Grouped by the family in the type id (`core.exposure` -> Core), keeping the
  // backend's own ordering so the list reads the same top to bottom.
  const groups = useMemo(() => {
    const byCategory = new Map<string, NodeDescriptor[]>();
    for (const descriptor of filtered) {
      const category = descriptor.typeId.split('.')[0] || descriptor.typeId;
      const bucket = byCategory.get(category);
      if (bucket) bucket.push(descriptor);
      else byCategory.set(category, [descriptor]);
    }
    return [...byCategory];
  }, [filtered]);
  const searching = query.trim().length > 0;

  return (
    <aside className="panel panel--library">
      <div className="panel__heading">
        <div>
          <span className="eyebrow">Library</span>
          <h2>Nodes</h2>
        </div>
        <span className="count-badge">{filtered.length}</span>
        {onCollapse && (
          <button
            aria-label="Collapse Nodes panel"
            className="icon-button"
            onClick={onCollapse}
            title="Collapse Nodes panel"
            type="button"
          >
            <Icon name="chevronLeft" />
          </button>
        )}
      </div>
      <label className="search-field">
        <span className="sr-only">Search nodes</span>
        <input
          aria-describedby="node-library-hint"
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && filtered[0]) {
              event.preventDefault();
              onAdd(filtered[0].typeId);
            }
          }}
          placeholder="Search nodes"
          ref={searchInputRef}
          type="search"
          value={query}
        />
      </label>
      <div className="node-library__toolbar">
        <span id="node-library-hint">Enter adds the first result · ⌘K focuses search</span>
        {hasCompatibleTypes && (
          <label className="filter-chip">
            <input
              aria-label="Compatible nodes only"
              checked={compatibleOnly}
              onChange={(event) => setCompatibleOnly(event.target.checked)}
              type="checkbox"
            />
            <span>{compatibleOnly ? 'Compatible inputs' : 'Showing all nodes'}</span>
          </label>
        )}
      </div>
      <div className="node-library">
        {groups.map(([category, items]) => (
          <details
            className="node-library__group"
            key={category}
            // A search must never hide its own matches, so every group opens
            // while filtering and the user's collapses only apply when browsing.
            onToggle={(event) => {
              if (searching) return;
              const open = event.currentTarget.open;
              setCollapsed((current) => {
                const next = new Set(current);
                if (open) next.delete(category);
                else next.add(category);
                return next;
              });
            }}
            open={searching || !collapsed.has(category)}
          >
            <summary className="node-library__group-summary">
              <span>{categoryLabel(category)}</span>
              <span className="count-badge">{items.length}</span>
            </summary>
            {items.map((descriptor) => (
              <button
                className="node-library__item"
                key={descriptor.typeId}
                onClick={() => onAdd(descriptor.typeId)}
                type="button"
              >
                <span className="node-library__icon"><Icon name="plus" /></span>
                <span>
                  <strong>{descriptor.name}</strong>
                  <small>{descriptor.typeId}</small>
                </span>
              </button>
            ))}
          </details>
        ))}
        {filtered.length === 0 && <p className="empty-state">No nodes match this search.</p>}
      </div>
      {onCreateSubgraph && (
        <div className="library-selection">
          <span>{selectedCount ? `${selectedCount} nodes selected` : 'Select nodes to compose a subgraph'}</span>
          <button
            className="button button--small"
            disabled={selectedCount === 0}
            onClick={onCreateSubgraph}
            type="button"
          >
            Create subgraph
          </button>
        </div>
      )}
    </aside>
  );
}
