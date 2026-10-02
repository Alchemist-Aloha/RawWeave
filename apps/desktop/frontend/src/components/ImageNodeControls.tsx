import { useEffect, useState } from 'react';
import type { EditorNode, ParameterValue } from '../editor/types';
import type { ImageDimensions, PreviewTarget } from '../viewer/types';
import { ViewerController } from '../viewer/controller';
import { DRAWABLE_NODES } from '../viewer/region';

export function ImageNodeControls({ node, target, inputSize, controller, onDraw, onChange }: {
  node: EditorNode;
  target: PreviewTarget | null;
  inputSize?: ImageDimensions;
  controller: ViewerController;
  onDraw: () => void;
  onChange: (values: Record<string, ParameterValue>) => void;
}) {
  const [, render] = useState(0);
  useEffect(() => controller.subscribe(() => render((value) => value + 1)), [controller]);
  if (!DRAWABLE_NODES.has(node.typeId) && node.typeId !== 'core.resize') return null;
  const pane = controller.state.panes.A;
  const size = target && pane.target?.nodeId === target.nodeId && pane.target?.outputPort === target.outputPort && pane.status === 'ready' ? pane.imageRegion : inputSize ?? null;
  const driven = node.exposedParameters?.some((id) => ['x', 'y', 'width', 'height', 'start_x', 'start_y', 'end_x', 'end_y', 'center_x', 'center_y', 'radius'].includes(id));
  return <section className="image-node-controls" aria-label={`${node.descriptor.name} image controls`}>
    <span>{size ? `Input resolution: ${size.width} × ${size.height} px` : target ? 'Preview input to read its resolution' : 'Connect an image input to use these controls'}</span>
    <button type="button" disabled={!target} onClick={onDraw}>{node.typeId === 'core.resize' ? 'View input size' : node.typeId === 'core.crop' ? 'Draw crop region' : 'Draw gradient'}</button>
    {driven && <small>Exposed parameter ports may override these values.</small>}
    {size && (node.typeId === 'core.crop' || node.typeId === 'core.resize') && <div>
      <button type="button" onClick={() => onChange(node.typeId === 'core.crop' ? { x: 0, y: 0, width: size.width, height: size.height } : { width: size.width, height: size.height })}>{node.typeId === 'core.crop' ? 'Use full image' : 'Original size'}</button>
      {node.typeId === 'core.resize' && <button type="button" onClick={() => onChange({ width: Math.max(1, Math.round(size.width / 2)), height: Math.max(1, Math.round(size.height / 2)) })}>Half size</button>}
    </div>}
  </section>;
}
