import type { EditorNode, ParameterValue } from '../editor/types';

interface InspectorProps {
  node: EditorNode | undefined;
  onChange: (parameterId: string, value: ParameterValue) => void;
  onDelete: (nodeId: string) => void;
}

export function Inspector({ node, onChange, onDelete }: InspectorProps) {
  if (!node) {
    return (
      <aside className="panel panel--inspector inspector-empty">
        <span className="eyebrow">Inspector</span>
        <p>Select a node to edit its parameters.</p>
      </aside>
    );
  }

  return (
    <aside className="panel panel--inspector">
      <div className="panel__heading">
        <div>
          <span className="eyebrow">Inspector</span>
          <h2>{node.descriptor.name}</h2>
        </div>
        <button
          aria-label={`Delete ${node.descriptor.name}`}
          className="icon-button icon-button--danger"
          onClick={() => onDelete(node.id)}
          title="Delete node"
          type="button"
        >
          ×
        </button>
      </div>
      <div className="inspector__identity">
        <span>{node.id}</span>
        <code>{node.typeId}</code>
      </div>
      <div className="parameter-list">
        {node.descriptor.parameters.map((parameter) => {
          const value = node.parameters[parameter.id] ?? parameter.default;
          if (parameter.parameterType === 'Boolean') {
            return (
              <label className="parameter parameter--checkbox" key={parameter.id}>
                <span>{parameter.name}</span>
                <input
                  checked={Boolean(value)}
                  onChange={(event) => onChange(parameter.id, event.target.checked)}
                  type="checkbox"
                />
              </label>
            );
          }
          return (
            <label className="parameter" key={parameter.id}>
              <span>{parameter.name}</span>
              <input
                max={parameter.max ?? undefined}
                min={parameter.min ?? undefined}
                onChange={(event) => {
                  const next =
                    parameter.parameterType === 'Float'
                      ? Number(event.target.value)
                      : event.target.value;
                  if (parameter.parameterType !== 'Float' || Number.isFinite(next)) {
                    onChange(parameter.id, next);
                  }
                }}
                step={parameter.parameterType === 'Float' ? 0.01 : undefined}
                type={parameter.parameterType === 'Float' ? 'number' : 'text'}
                value={String(value)}
              />
            </label>
          );
        })}
        {node.descriptor.parameters.length === 0 && (
          <p className="empty-state empty-state--compact">This node has no parameters.</p>
        )}
      </div>
    </aside>
  );
}
