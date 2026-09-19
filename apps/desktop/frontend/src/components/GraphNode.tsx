import { Handle, Position, type Node, type NodeProps } from '@xyflow/react';
import type { EditorNode } from '../editor/types';

export interface RawWeaveNodeData extends Record<string, unknown> {
  node: EditorNode;
}

export type RawWeaveFlowNode = Node<RawWeaveNodeData, 'rawweave'>;

export function GraphNode({ data, selected }: NodeProps<RawWeaveFlowNode>) {
  const { node } = data;
  const exposedPorts = (node.exposedParameters ?? [])
    .filter(
      (parameterId) => !node.descriptor.inputs.some((input) => input.id === parameterId),
    )
    .map((parameterId) => ({
      id: parameterId,
      name: node.descriptor.parameters.find((parameter) => parameter.id === parameterId)?.name ?? parameterId,
    }));
  const inputPorts = [
    ...node.descriptor.inputs.map((input) => ({ id: input.id, name: input.name })),
    ...exposedPorts,
  ];
  return (
    <div aria-label={`${node.descriptor.name} node`} className={`graph-node${selected ? ' graph-node--selected' : ''}`}>
      <div className="graph-node__title-row">
        <div className="graph-node__title">{node.descriptor.name}</div>
        {node.descriptor.capabilities?.[0] && (
          <span className="graph-node__badge">{node.descriptor.capabilities[0]}</span>
        )}
      </div>
      <div className="graph-node__type">{node.typeId}</div>
      <div className="graph-node__ports">
        <div className="graph-node__port-column">
          {inputPorts.map((input, index) => (
            <div className="graph-node__port graph-node__port--input" key={input.id}>
              <Handle
                id={input.id}
                type="target"
                position={Position.Left}
                style={{ top: 53 + index * 25 }}
              />
              <span>{input.name}</span>
            </div>
          ))}
        </div>
        <div className="graph-node__port-column graph-node__port-column--output">
          {node.descriptor.outputs.map((output, index) => (
            <div className="graph-node__port graph-node__port--output" key={output.id}>
              <span>{output.name}</span>
              <Handle
                id={output.id}
                type="source"
                position={Position.Right}
                style={{ top: 53 + index * 25 }}
              />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
