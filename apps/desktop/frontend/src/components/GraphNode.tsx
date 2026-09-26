import { createContext, useRef, useContext, useState, type ReactNode } from 'react';
import { Handle, Position, type Node, type NodeProps } from '@xyflow/react';
import type { EditorNode, ParameterDescriptor, ParameterValue, WorkflowPort } from '../editor/types';
import { inputDataType } from '../editor/connections';
import type { CheckpointStatus } from '../checkpoint/types';
import { dataTypeColor } from '../ui/data-type-colors';
import { Icon } from '../ui/Icon';
import {
  CheckpointPanel,
  type CheckpointOutputPort,
  type CheckpointPreviewActions,
} from './CheckpointPanel';

export interface GraphNodeCheckpoint {
  nodeId: string;
  status: CheckpointStatus | null;
  loading: boolean;
  outputPorts: CheckpointOutputPort[];
  previewActions?: CheckpointPreviewActions;
  onGenerate: (outputPort: string) => void | Promise<void>;
  onCancel: () => void | Promise<void>;
}

/**
 * Editor actions the node card needs to edit its own parameters. Provided once
 * around the canvas so node `data` stays `{ node, checkpointStatus }` and the
 * drag-time flow-node cache keeps working.
 */
export interface GraphNodeActions {
  onParameterChange: (nodeId: string, parameterId: string, value: ParameterValue) => void;
  onToggleExposed: (nodeId: string, parameterId: string, exposed: boolean) => void;
  onTogglePort: (nodeId: string, portId: string, direction: 'Input' | 'Output', exposed: boolean) => void;
  onDelete: (nodeId: string) => void;
  workflowInputs: WorkflowPort[];
  workflowOutputs: WorkflowPort[];
  checkpoint: GraphNodeCheckpoint | null;
}

export const GraphNodeActionsContext = createContext<GraphNodeActions | null>(null);

export interface RawWeaveNodeData extends Record<string, unknown> {
  node: EditorNode;
  checkpointStatus?: CheckpointStatus | null;
}

export type RawWeaveFlowNode = Node<RawWeaveNodeData, 'rawweave'>;

interface ParameterFieldProps {
  parameter: ParameterDescriptor;
  value: ParameterValue;
  onChange: (value: ParameterValue) => void;
  toggle: ReactNode;
}

/**
 * One parameter row. The text field keeps a local draft while it has focus:
 * every keystroke round-trips through the graph and comes back as a new `value`
 * prop, and without the draft React would reset the field mid-typing and drop
 * the rest of what was typed.
 */
function ParameterField({ parameter, value, onChange, toggle }: ParameterFieldProps) {
  const [draft, setDraft] = useState<string | null>(null);
  if (parameter.parameterType === 'Boolean') {
    return (
      <div className="parameter">
        <label className="parameter__field parameter__field--checkbox">
          <span>{parameter.name}</span>
          <input
            checked={Boolean(value)}
            onChange={(event) => onChange(event.target.checked)}
            type="checkbox"
          />
        </label>
        {toggle}
      </div>
    );
  }
  const numeric = parameter.parameterType === 'Float' || parameter.parameterType === 'Integer';
  return (
    <div className="parameter">
      <label className="parameter__field">
        <span>{parameter.name}</span>
        <input
          max={parameter.max ?? undefined}
          min={parameter.min ?? undefined}
          onBlur={() => setDraft(null)}
          onChange={(event) => {
            const text = event.target.value;
            setDraft(text);
            const next = numeric ? Number(text) : text;
            if (!numeric || Number.isFinite(next)) onChange(next);
          }}
          step={parameter.parameterType === 'Float' ? 0.01 : parameter.parameterType === 'Integer' ? 1 : undefined}
          type={numeric ? 'number' : 'text'}
          value={draft ?? String(value)}
        />
      </label>
      {toggle}
    </div>
  );
}

export function GraphNode({ data, selected }: NodeProps<RawWeaveFlowNode>) {
  const { node, checkpointStatus } = data;
  const actions = useContext(GraphNodeActionsContext);
  const deletePress = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  const exposedPorts = (node.exposedParameters ?? [])
    .filter(
      (parameterId) => !node.descriptor.inputs.some((input) => input.id === parameterId),
    )
    .map((parameterId) => ({
      id: parameterId,
      name: node.descriptor.parameters.find((parameter) => parameter.id === parameterId)?.name ?? parameterId,
      dataType: inputDataType(node, parameterId) ?? 'value.String',
    }));
  const inputPorts = [
    ...node.descriptor.inputs.map((input) => ({ id: input.id, name: input.name, dataType: input.dataType })),
    ...exposedPorts,
  ];
  const checkpoint = selected && actions?.checkpoint?.nodeId === node.id ? actions.checkpoint : null;
  const hasDetailControls = Boolean(
    actions
    && (node.descriptor.parameters.length > 0
      || node.descriptor.inputs.length > 0
      || node.descriptor.outputs.length > 0),
  );

  const renderPortToggle = (direction: 'Input' | 'Output', portId: string, name: string) => {
    if (!actions) return null;
    const exposed = (direction === 'Input' ? actions.workflowInputs : actions.workflowOutputs).some(
      (port) => port.id === `${direction === 'Input' ? 'input' : 'output'}:${node.id}:${portId}`,
    );
    return (
      <div className="port-row" key={`${direction}:${portId}`}>
        <span><b>{direction === 'Input' ? 'In' : 'Out'}</b> {name}</span>
        <button
          aria-pressed={exposed}
          className={`port-toggle${exposed ? ' port-toggle--active' : ''}`}
          onClick={() => actions.onTogglePort(node.id, portId, direction, !exposed)}
          type="button"
        >
          {exposed ? 'Hide' : 'Expose'}
        </button>
      </div>
    );
  };

  return (
    <div aria-label={`${node.descriptor.name} node`} className={`graph-node${selected ? ' graph-node--selected' : ''}`}>
      <div className="graph-node__title-row">
        <div className="graph-node__title">{node.descriptor.name}</div>
        <div className="graph-node__badges">
          {node.descriptor.evaluationPolicy === 'manual_checkpoint' && (
            <span className={`graph-node__badge graph-node__badge--checkpoint graph-node__badge--${checkpointStatus?.state ?? 'unknown'}`}>
              {checkpointStatus?.state ?? 'checkpoint'}
            </span>
          )}
          {node.descriptor.capabilities?.[0] && (
            <span className="graph-node__badge">{node.descriptor.capabilities[0]}</span>
          )}
        </div>
        {actions && (
          <button
            aria-label={`Delete ${node.descriptor.name}`}
            className="icon-button icon-button--danger nodrag graph-node__delete"
            // The button sits in the title row, which is the node's drag handle,
            // so a press that moves is a drag and must not delete. Keyboard and
            // synthetic activation arrive as a click with no press position.
            onClick={() => {
              const press = deletePress.current;
              deletePress.current = null;
              if (press?.moved) return;
              actions.onDelete(node.id);
            }}
            onPointerCancel={() => {
              deletePress.current = null;
            }}
            onPointerDown={(event) => {
              deletePress.current = { x: event.clientX, y: event.clientY, moved: false };
            }}
            onPointerMove={(event) => {
              const press = deletePress.current;
              if (press && Math.hypot(event.clientX - press.x, event.clientY - press.y) > 4) {
                press.moved = true;
              }
            }}
            title="Delete node"
            type="button"
          >
            <Icon name="close" />
          </button>
        )}
      </div>
      <div className="graph-node__type">{node.typeId}</div>
      <div className="graph-node__ports">
        <div className="graph-node__port-column">
          {inputPorts.map((input) => (
            <div className="graph-node__port graph-node__port--input" key={input.id}>
              <Handle
                data-datatype={input.dataType}
                id={input.id}
                position={Position.Left}
                style={{ background: dataTypeColor(input.dataType) }}
                title={`${input.name} · ${input.dataType}`}
                type="target"
              />
              <span>{input.name}</span>
            </div>
          ))}
        </div>
        <div className="graph-node__port-column graph-node__port-column--output">
          {node.descriptor.outputs.map((output) => (
            <div className="graph-node__port graph-node__port--output" key={output.id}>
              <span>{output.name}</span>
              <Handle
                data-datatype={output.dataType}
                id={output.id}
                position={Position.Right}
                style={{ background: dataTypeColor(output.dataType) }}
                title={`${output.name} · ${output.dataType}`}
                type="source"
              />
            </div>
          ))}
        </div>
      </div>
      {hasDetailControls && (
        // `nodrag` keeps parameter clicks from starting a node drag; the title
        // row above stays the place to grab the frame.
        <details className="graph-node__details nodrag">
          <summary className="graph-node__details-summary">Parameters</summary>
          {(node.descriptor.inputs.length > 0 || node.descriptor.outputs.length > 0) && (
            <section className="graph-node__ports-editor" aria-label="Workflow ports">
              <span className="eyebrow">Workflow ports</span>
              {node.descriptor.inputs.map((port) => renderPortToggle('Input', port.id, port.name))}
              {node.descriptor.outputs.map((port) => renderPortToggle('Output', port.id, port.name))}
            </section>
          )}
          <div className="parameter-list">
            {node.descriptor.parameters.map((parameter) => {
              const exposed = (node.exposedParameters ?? []).includes(parameter.id);
              return (
                <ParameterField
                  key={parameter.id}
                  onChange={(next) => actions?.onParameterChange(node.id, parameter.id, next)}
                  parameter={parameter}
                  toggle={(
                    <button
                      aria-label={`${exposed ? 'Hide' : 'Expose'} ${parameter.name} port`}
                      aria-pressed={exposed}
                      className={`port-toggle${exposed ? ' port-toggle--active' : ''}`}
                      onClick={() => actions?.onToggleExposed(node.id, parameter.id, !exposed)}
                      title={exposed ? 'Remove parameter port' : 'Expose as an input port'}
                      type="button"
                    >
                      ⇄
                    </button>
                  )}
                  value={node.parameters[parameter.id] ?? parameter.default}
                />
              );
            })}
            {node.descriptor.parameters.length === 0 && (
              <p className="empty-state empty-state--compact">This node has no parameters.</p>
            )}
          </div>
          {checkpoint && (
            <CheckpointPanel
              loading={checkpoint.loading}
              nodeLabel={node.descriptor.name}
              onCancel={checkpoint.onCancel}
              onGenerate={checkpoint.onGenerate}
              outputPorts={checkpoint.outputPorts}
              previewActions={checkpoint.previewActions}
              status={checkpoint.status}
            />
          )}
        </details>
      )}
    </div>
  );
}
