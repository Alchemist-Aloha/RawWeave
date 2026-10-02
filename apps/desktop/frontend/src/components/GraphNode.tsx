import { createContext, useRef, useContext, useState, useId, type ReactNode } from 'react';
import { Handle, Position, type Node, type NodeProps } from '@xyflow/react';
import type { EditorNode, ParameterDescriptor, ParameterValue, WorkflowPort } from '../editor/types';
import { isRecommendedValue, parameterUX } from '../editor/parameter-ux';
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
  imageControls?: (node: EditorNode) => ReactNode;
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
  typeId: string;
  parameter: ParameterDescriptor;
  value: ParameterValue;
  onChange: (value: ParameterValue) => void;
  toggle: ReactNode;
  wholeNumber?: boolean;
  multiline?: boolean;
  choices?: string[];
  suggestions?: string[];
}

/**
 * Numeric edits stay local until Enter or blur so one edit produces one graph
 * command. Escape discards the draft; sliders still commit on pointer release.
 */
function ParameterField({ typeId, parameter, value, onChange, toggle, wholeNumber = false, multiline = false, choices, suggestions }: ParameterFieldProps) {
  const [draft, setDraft] = useState<string | null>(null);
  const sliding = useRef(false);
  const suggestionId = useId();
  const ux = parameterUX(typeId, parameter);
  const modified = !isRecommendedValue(value, parameter.default);
  const numeric = parameter.parameterType === 'Float' || parameter.parameterType === 'Integer';
  const factor = ux.factor ?? 1;
  const min = ux.min ?? parameter.min ?? undefined;
  const max = ux.max ?? parameter.max ?? undefined;
  const bounded = numeric && min != null && max != null && max > min;
  const validNumericValue = (next: number) => Number.isFinite(next)
    && (parameter.parameterType !== 'Integer' && !wholeNumber || Number.isInteger(next))
    && (parameter.min == null || next >= parameter.min)
    && (parameter.max == null || next <= parameter.max);
  const reset = () => {
    setDraft(null);
    onChange(parameter.default);
  };

  if (parameter.parameterType === 'Boolean') {
    return (
      <div className={`parameter${modified ? ' parameter--modified' : ''}`}>
        <label className="parameter__field parameter__field--checkbox parameter__label" data-tooltip={ux.description}>
          <span>{ux.name}</span>
          <input
            aria-description={ux.description}
            checked={Boolean(value)}
            onChange={(event) => onChange(event.target.checked)}
            type="checkbox"
          />
        </label>
        {modified && <button aria-label={`Reset ${ux.name}`} className="parameter__reset" onClick={() => onChange(parameter.default)} type="button">Reset</button>}
        {toggle}
      </div>
    );
  }
  if (parameter.parameterType === 'String' && (ux.options || choices)) {
    const options = ux.options ?? choices!.map((option) => ({ value: option, label: option }));
    const completeOptions = options.some((option) => option.value === value)
      ? options
      : [...options, { value: String(value), label: `${String(value)} (saved value)` }];
    return (
      <div className={`parameter${modified ? ' parameter--modified' : ''}`}>
        <label className="parameter__field parameter__label" data-tooltip={ux.description}>
          <span>{ux.name}</span>
          <select aria-description={ux.description} aria-label={ux.name} onChange={(event) => onChange(event.target.value)} value={String(value)}>
            {completeOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
          </select>
        </label>
        {modified && <button aria-label={`Reset ${ux.name}`} className="parameter__reset" onClick={() => onChange(parameter.default)} type="button">Reset</button>}
        {toggle}
      </div>
    );
  }
  const hasSlider = bounded;
  const outsideRecommended = numeric && ux.recommendedRange && typeof value === 'number'
    && ((ux.min != null && value < ux.min) || (ux.max != null && value > ux.max));
  const shownNumber = typeof value === 'number' ? value * factor : value;
  const displayValue = typeof shownNumber === 'number' && ux.precision != null ? Number(shownNumber.toFixed(ux.precision)) : shownNumber;
  const sliderValue = typeof value === 'number' ? Math.min(max! * factor, Math.max(min! * factor, value * factor)) : min! * factor;
  return (
    <div className={`parameter${modified ? ' parameter--modified' : ''}`}>
      <label className="parameter__field parameter__label" data-tooltip={ux.description}>
        <span>{ux.name}{ux.unit ? ` (${ux.unit})` : ''}</span>
        <span className="parameter__value-row">
        {hasSlider && <input
          aria-description={ux.description}
          aria-label={`${ux.name} slider`}
          max={max! * factor}
          min={min! * factor}
          onPointerDown={(event) => {
            sliding.current = true;
            try { event.currentTarget.setPointerCapture?.(event.pointerId); } catch { /* Continue within the surface. */ }
          }}
          onPointerUp={(event) => {
            if (!sliding.current) return;
            sliding.current = false;
            setDraft(null);
            const next = Number(event.currentTarget.value) / factor;
            if (validNumericValue(next)) onChange(next);
          }}
          onPointerCancel={() => { sliding.current = false; setDraft(null); }}
          onChange={(event) => {
            const text = event.target.value;
            if (sliding.current) setDraft(text);
            else {
              setDraft(null);
              const next = Number(text) / factor;
              if (validNumericValue(next)) onChange(next);
            }
          }}
          step={(wholeNumber ? 1 : ux.step ?? 0.01) * factor}
          type="range"
          value={draft != null && Number.isFinite(Number(draft)) ? Number(draft) : sliderValue}
        />}
        {multiline && !numeric ? <textarea
          className="parameter__field--multiline"
          aria-label={ux.name}
          aria-description={ux.description}
          onBlur={() => setDraft(null)}
          onChange={(event) => { setDraft(event.target.value); onChange(event.target.value); }}
          value={draft ?? String(value)}
        /> : <input
          list={suggestions ? suggestionId : undefined}
          max={parameter.max == null ? undefined : parameter.max * factor}
          min={parameter.min == null ? undefined : parameter.min * factor}
          aria-description={ux.description}
          aria-label={ux.name}
          aria-invalid={numeric && draft !== null && (draft.trim() === '' || !validNumericValue(Number(draft) / factor)) ? true : undefined}
          title={numeric ? 'Enter or leave the field to apply; Escape to cancel' : undefined}
          onBlur={() => {
            if (numeric && draft !== null && draft.trim() !== '') {
              const next = Number(draft) / factor;
              if (validNumericValue(next) && next !== value) onChange(next);
            }
            setDraft(null);
          }}
          onKeyDown={(event) => {
            if (!numeric || event.nativeEvent.isComposing) return;
            if (event.key === 'Enter') {
              event.preventDefault();
              event.stopPropagation();
              event.currentTarget.blur();
            } else if (event.key === 'Escape') {
              event.preventDefault();
              event.stopPropagation();
              setDraft(null);
            }
          }}
          onChange={(event) => {
            const text = event.target.value;
            setDraft(text);
            if (!numeric) onChange(text);
          }}
          step={(wholeNumber ? 1 : ux.step ?? 0.01) * factor}
          type={numeric ? 'number' : 'text'}
          value={draft ?? String(displayValue)}
        />}
        {suggestions && <datalist id={suggestionId}>{suggestions.map((suggestion) => <option key={suggestion} value={suggestion} />)}</datalist>}
        </span>
      </label>
      {outsideRecommended && <span className="parameter__warning">Outside recommended range</span>}
      {modified && <button aria-label={`Reset ${ux.name}`} className="parameter__reset" onClick={() => onChange(parameter.default)} type="button">Reset</button>}
      {toggle}
    </div>
  );
}

export function GraphNode({ data, selected }: NodeProps<RawWeaveFlowNode>) {
  const { node, checkpointStatus } = data;
  const [detailsOpen, setDetailsOpen] = useState(false);
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
        <details className="graph-node__details nodrag" onToggle={(event) => setDetailsOpen(event.currentTarget.open)}>
          <summary className="graph-node__details-summary">Parameters</summary>
          {detailsOpen && actions?.imageControls?.(node)}
          <div className="parameter-list">
            {node.descriptor.parameters.filter((parameter) => !parameterUX(node.typeId, parameter).advanced).map((parameter) => {
              const exposed = (node.exposedParameters ?? []).includes(parameter.id);
              return (
                <ParameterField
                  key={parameter.id}
                  typeId={node.typeId}
                  onChange={(next) => actions?.onParameterChange(node.id, parameter.id, next)}
                  parameter={parameter}
                  multiline={['prompt', 'negative_prompt', 'workflow_definition', 'points', 'expression'].includes(parameter.id)}
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
                  choices={['pro.tone-map', 'pro.advanced-tone-map'].includes(node.typeId) && parameter.id === 'operator' ? ['reinhard', 'filmic', 'aces'] : undefined}
                  suggestions={node.typeId === 'raw.camera-transform' && parameter.id === 'working_space' ? ['sRGB', 'CameraNative', 'DisplayP3', 'ProPhoto', 'Rec2020'] : undefined}
                  wholeNumber={
                    (node.typeId === 'core.crop' || node.typeId === 'core.resize')
                    && ['x', 'y', 'width', 'height'].includes(parameter.id)
                    || ['core.blur', 'core.mask-feather', 'core.mask-blur', 'core.mask-expand', 'core.mask-contract'].includes(node.typeId) && parameter.id === 'radius'
                    || node.typeId === 'core.mask-painted' && ['width', 'height', 'origin_x', 'origin_y'].includes(parameter.id)
                  }
                />
              );
            })}
            {node.descriptor.parameters.some((parameter) => parameterUX(node.typeId, parameter).advanced) && (
              <details className="parameter-advanced">
                <summary>Advanced</summary>
                {node.descriptor.parameters.filter((parameter) => parameterUX(node.typeId, parameter).advanced).map((parameter) => {
                  const exposed = (node.exposedParameters ?? []).includes(parameter.id);
                  return <ParameterField key={parameter.id} typeId={node.typeId} onChange={(next) => actions?.onParameterChange(node.id, parameter.id, next)} parameter={parameter} multiline={['prompt', 'negative_prompt', 'workflow_definition', 'points', 'expression'].includes(parameter.id)} wholeNumber={(node.typeId === 'core.crop' || node.typeId === 'core.resize') && ['x', 'y', 'width', 'height'].includes(parameter.id) || ['core.blur', 'core.mask-feather', 'core.mask-blur', 'core.mask-expand', 'core.mask-contract'].includes(node.typeId) && parameter.id === 'radius' || node.typeId === 'core.mask-painted' && ['width', 'height', 'origin_x', 'origin_y'].includes(parameter.id)} toggle={<button aria-label={`${exposed ? 'Hide' : 'Expose'} ${parameterUX(node.typeId, parameter).name} port`} aria-pressed={exposed} className={`port-toggle${exposed ? ' port-toggle--active' : ''}`} onClick={() => actions?.onToggleExposed(node.id, parameter.id, !exposed)} title={exposed ? 'Remove parameter port' : 'Expose as an input port'} type="button">⇄</button>} value={node.parameters[parameter.id] ?? parameter.default} />;
                })}
              </details>
            )}
            {node.descriptor.parameters.length > 0 && node.descriptor.parameters.some((parameter) => !isRecommendedValue(node.parameters[parameter.id] ?? parameter.default, parameter.default)) && (
              <button className="parameter-reset-all" onClick={() => node.descriptor.parameters.forEach((parameter) => actions?.onParameterChange(node.id, parameter.id, parameter.default))} type="button">Reset parameters</button>
            )}
            {node.descriptor.parameters.length === 0 && (
              <p className="empty-state empty-state--compact">This node has no parameters.</p>
            )}
          </div>
          {(node.descriptor.inputs.length > 0 || node.descriptor.outputs.length > 0) && (
            <section className="graph-node__ports-editor" aria-label="Workflow ports">
              <span className="eyebrow">Workflow ports</span>
              {node.descriptor.inputs.map((port) => renderPortToggle('Input', port.id, port.name))}
              {node.descriptor.outputs.map((port) => renderPortToggle('Output', port.id, port.name))}
            </section>
          )}
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
