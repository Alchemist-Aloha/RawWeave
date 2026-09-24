import { Handle, Position, type Node, type NodeProps } from '@xyflow/react';
import type { EditorNode } from '../editor/types';
import { inputDataType } from '../editor/connections';
import type { CheckpointStatus } from '../checkpoint/types';
import { dataTypeColor } from '../ui/data-type-colors';

export interface RawWeaveNodeData extends Record<string, unknown> {
  node: EditorNode;
  checkpointStatus?: CheckpointStatus | null;
}

export type RawWeaveFlowNode = Node<RawWeaveNodeData, 'rawweave'>;

export function GraphNode({ data, selected }: NodeProps<RawWeaveFlowNode>) {
  const { node, checkpointStatus } = data;
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
    </div>
  );
}
