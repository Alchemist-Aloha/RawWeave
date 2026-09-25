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
        {filtered.map((descriptor) => (
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
