import { useMemo, useState } from 'react';
import type { NodeDescriptor } from '../editor/types';

interface NodeLibraryProps {
  descriptors: NodeDescriptor[];
  onAdd: (typeId: string) => void;
}

export function NodeLibrary({ descriptors, onAdd }: NodeLibraryProps) {
  const [query, setQuery] = useState('');
  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return descriptors;
    return descriptors.filter((descriptor) =>
      `${descriptor.name} ${descriptor.typeId}`.toLowerCase().includes(normalized),
    );
  }, [descriptors, query]);

  return (
    <aside className="panel panel--library">
      <div className="panel__heading">
        <div>
          <span className="eyebrow">Library</span>
          <h2>Nodes</h2>
        </div>
        <span className="count-badge">{filtered.length}</span>
      </div>
      <label className="search-field">
        <span className="sr-only">Search nodes</span>
        <input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search nodes"
          type="search"
        />
      </label>
      <div className="node-library">
        {filtered.map((descriptor) => (
          <button
            className="node-library__item"
            key={descriptor.typeId}
            onClick={() => onAdd(descriptor.typeId)}
            type="button"
          >
            <span className="node-library__icon">+</span>
            <span>
              <strong>{descriptor.name}</strong>
              <small>{descriptor.typeId}</small>
            </span>
          </button>
        ))}
        {filtered.length === 0 && <p className="empty-state">No nodes match this search.</p>}
      </div>
    </aside>
  );
}
