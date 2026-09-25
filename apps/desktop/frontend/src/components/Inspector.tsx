import type { EditorNode, ParameterValue, WorkflowPort } from '../editor/types';
import type { CheckpointStatus } from '../checkpoint/types';
import {
  CheckpointPanel,
  type CheckpointOutputPort,
  type CheckpointPreviewActions,
} from './CheckpointPanel';
import { ImageSetInspector } from './ImageSetInspector';
import type { ImageSetCollection } from '../imageset/model';
import { Icon } from '../ui/Icon';

interface InspectorProps {
  node: EditorNode | undefined;
  selectedNodeIds?: string[];
  workflowInputs?: WorkflowPort[];
  workflowOutputs?: WorkflowPort[];
  onChange: (parameterId: string, value: ParameterValue) => void;
  onToggleExposed: (parameterId: string, exposed: boolean) => void;
  onToggleInput?: (nodeId: string, portId: string, direction: 'Input' | 'Output', exposed: boolean) => void;
  onDelete: (nodeId: string) => void;
  checkpointStatus?: CheckpointStatus | null;
  checkpointLoading?: boolean;
  onGenerateCheckpoint?: (outputPort: string) => void | Promise<void>;
  onCancelCheckpoint?: () => void | Promise<void>;
  checkpointOutputPorts?: CheckpointOutputPort[];
  checkpointPreviewActions?: CheckpointPreviewActions;
  imageSet?: ImageSetCollection | null;
  onImageSetReorder?: (memberId: string, targetIndex: number) => void;
  onImageSetAlignmentChange?: (referenceMember: string | null) => void;
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
}

export function Inspector({
  node,
  selectedNodeIds = node ? [node.id] : [],
  workflowInputs = [],
  workflowOutputs = [],
  onChange,
  onToggleExposed,
  onToggleInput,
  onDelete,
  checkpointStatus = null,
  checkpointLoading = false,
  onGenerateCheckpoint,
  onCancelCheckpoint,
  checkpointOutputPorts = [],
  checkpointPreviewActions,
  imageSet = null,
  onImageSetReorder,
  onImageSetAlignmentChange,
  collapsed = false,
  onToggleCollapsed,
}: InspectorProps) {
  const collapseButton = onToggleCollapsed ? (
    <button
      aria-label={collapsed ? 'Expand inspector panel' : 'Collapse inspector panel'}
      className="icon-button"
      onClick={onToggleCollapsed}
      title={collapsed ? 'Expand inspector panel' : 'Collapse inspector panel'}
      type="button"
    >
      {collapsed ? <Icon name="chevronRight" /> : <Icon name="chevronDown" />}
    </button>
  ) : null;
  const imageSetPanel = imageSet && onImageSetReorder && onImageSetAlignmentChange ? (
    <ImageSetInspector
      collection={imageSet}
      onAlignmentChange={onImageSetAlignmentChange}
      onReorder={onImageSetReorder}
    />
  ) : null;
  if (selectedNodeIds.length > 1) {
    return (
      <aside className="panel panel--inspector">
        {collapsed ? (
          <div className="panel__heading">
            <div><span className="eyebrow">Inspector</span><h2>{selectedNodeIds.length} nodes selected</h2></div>
            {collapseButton}
          </div>
        ) : (
          <>
            {imageSetPanel}
            <div className="panel__heading">
              <div>
                <span className="eyebrow">Inspector</span>
                <h2>{selectedNodeIds.length} nodes selected</h2>
              </div>
              {collapseButton}
            </div>
            <p className="empty-state">Create a reusable subgraph from this selection, or select one node to edit its parameters.</p>
            <ul className="selection-list">
              {selectedNodeIds.map((id) => <li key={id}><code>{id}</code></li>)}
            </ul>
          </>
        )}
      </aside>
    );
  }
  if (!node) {
    return (
      <aside className="panel panel--inspector inspector-empty">
        <div className="panel__heading">
          <div><span className="eyebrow">Inspector</span><h2>Nothing selected</h2></div>
          {collapseButton}
        </div>
        {!collapsed && <p>Select a node to edit its parameters.</p>}
      </aside>
    );
  }

  const renderPortToggle = (direction: 'Input' | 'Output', portId: string, name: string) => {
    const exposed = (direction === 'Input' ? workflowInputs : workflowOutputs).some(
      (port) => port.id === `${direction === 'Input' ? 'input' : 'output'}:${node.id}:${portId}`,
    );
    return (
      <div className="port-row" key={`${direction}:${portId}`}>
        <span><b>{direction === 'Input' ? 'In' : 'Out'}</b> {name}</span>
        {onToggleInput && (
          <button
            aria-pressed={exposed}
            className={`port-toggle${exposed ? ' port-toggle--active' : ''}`}
            onClick={() => onToggleInput(node.id, portId, direction, !exposed)}
            type="button"
          >
            {exposed ? 'Hide' : 'Expose'}
          </button>
        )}
      </div>
    );
  };

  return (
    <aside className="panel panel--inspector">
      {collapsed ? (
        <div className="panel__heading">
          <div><span className="eyebrow">Inspector</span><h2>{node.descriptor.name}</h2></div>
          {collapseButton}
        </div>
      ) : (
        <>
          {imageSetPanel}
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
          <Icon name="close" />
        </button>
        {collapseButton}
      </div>
      <div className="inspector__identity">
        <span>{node.id}</span>
        <code>{node.typeId}</code>
      </div>
      {(node.descriptor.inputs.length > 0 || node.descriptor.outputs.length > 0) && (
        <section className="inspector__ports" aria-label="Workflow ports">
          <span className="eyebrow">Workflow ports</span>
          {node.descriptor.inputs.map((port) => renderPortToggle('Input', port.id, port.name))}
          {node.descriptor.outputs.map((port) => renderPortToggle('Output', port.id, port.name))}
        </section>
      )}
      <div className="parameter-list">
        {node.descriptor.parameters.map((parameter) => {
          const value = node.parameters[parameter.id] ?? parameter.default;
          const exposed = (node.exposedParameters ?? []).includes(parameter.id);
          const toggle = (
            <button
              aria-label={`${exposed ? 'Hide' : 'Expose'} ${parameter.name} port`}
              aria-pressed={exposed}
              className={`port-toggle${exposed ? ' port-toggle--active' : ''}`}
              onClick={() => onToggleExposed(parameter.id, !exposed)}
              title={exposed ? 'Remove parameter port' : 'Expose as an input port'}
              type="button"
            >
              ⇄
            </button>
          );
          if (parameter.parameterType === 'Boolean') {
            return (
              <div className="parameter" key={parameter.id}>
                <label className="parameter__field parameter__field--checkbox">
                  <span>{parameter.name}</span>
                  <input
                    checked={Boolean(value)}
                    onChange={(event) => onChange(parameter.id, event.target.checked)}
                    type="checkbox"
                  />
                </label>
                {toggle}
              </div>
            );
          }
          return (
            <div className="parameter" key={parameter.id}>
              <label className="parameter__field">
                <span>{parameter.name}</span>
                <input
                  max={parameter.max ?? undefined}
                  min={parameter.min ?? undefined}
                  onChange={(event) => {
                    const next =
                      parameter.parameterType === 'Float' || parameter.parameterType === 'Integer'
                        ? Number(event.target.value)
                        : event.target.value;
                    if (
                      (parameter.parameterType !== 'Float' &&
                        parameter.parameterType !== 'Integer') ||
                      Number.isFinite(next)
                    ) {
                      onChange(parameter.id, next);
                    }
                  }}
                  step={
                    parameter.parameterType === 'Float'
                      ? 0.01
                      : parameter.parameterType === 'Integer'
                        ? 1
                        : undefined
                  }
                  type={
                    parameter.parameterType === 'Float' || parameter.parameterType === 'Integer'
                      ? 'number'
                      : 'text'
                  }
                  value={String(value)}
                />
              </label>
              {toggle}
            </div>
          );
        })}
        {node.descriptor.parameters.length === 0 && (
          <p className="empty-state empty-state--compact">This node has no parameters.</p>
        )}
      </div>
      {node.descriptor.evaluationPolicy === 'manual_checkpoint' && onGenerateCheckpoint && onCancelCheckpoint && (
        <CheckpointPanel
          loading={checkpointLoading}
          onCancel={onCancelCheckpoint}
          onGenerate={onGenerateCheckpoint}
          outputPorts={checkpointOutputPorts}
          previewActions={checkpointPreviewActions}
          nodeLabel={node.descriptor.name}
          status={checkpointStatus}
        />
      )}
        </>
      )}
    </aside>
  );
}
