import { parameterUX, POINT_CURVE_NODES } from './parameter-ux';
import type { EditorNode, PlatformEdge } from './types';

export interface CurveAxes { x: string; y: string; description: string }
const metadataUnits: Record<string, string> = {
  iso: 'ISO', aperture: 'Aperture (f-number)', shutter_seconds: 'Shutter time (s)', focal_length: 'Focal length (mm)',
};

/** Label known quantities, never infer brightness or physical units from node IDs. */
export function curveAxes(node: Pick<EditorNode, 'id' | 'typeId'>, nodes: EditorNode[] = [], edges: PlatformEdge[] = []): CurveAxes {
  if (node.typeId === 'core.curves' || node.typeId === 'core.levels' || node.typeId !== 'core.curve' && POINT_CURVE_NODES.has(node.typeId)) {
    return { x: 'RGB level before curve', y: 'RGB level after curve',
      description: 'Each RGB channel runs from dark to bright: 0 is black; 1 is reference white. Values above 1 exceed reference white. Channel level is not luminance, absolute light intensity or image position.' };
  }
  const sourceEdge = edges.find((edge) => edge.toNode === node.id && edge.toPort === 'value');
  const source = nodes.find((candidate) => candidate.id === sourceEdge?.fromNode);
  const sourceName = source?.descriptor.outputs.find((port) => port.id === sourceEdge?.fromPort)?.name;
  const x = source?.typeId === 'core.metadata' && sourceEdge && metadataUnits[sourceEdge.fromPort]
    || (sourceName && !['Value', 'Result', 'A', 'B', 'C', 'D'].includes(sourceName) ? sourceName : 'Source control value');
  const destinations = new Set(edges.filter((edge) => edge.fromNode === node.id && edge.fromPort === 'value').map((edge) => {
    const target = nodes.find((candidate) => candidate.id === edge.toNode);
    const parameter = target?.descriptor.parameters.find((parameter) => parameter.id === edge.toPort);
    if (!target || !parameter) return 'Mapped control value';
    const ux = parameterUX(target.typeId, parameter);
    const unit = ux.unit === '%' && ux.factor === 100 ? 'fraction' : ux.factor && ux.factor !== 1 ? undefined : ux.unit;
    return unit ? `${ux.name} (${unit})` : ux.name;
  }));
  const y = destinations.size === 1 ? [...destinations][0] : 'Mapped control value';
  return { x, y,
    description: 'Numeric control mapping, not image brightness. Names and units come from connected ports; unspecified units are not inferred.'
      + (y.endsWith('(fraction)') ? ' A fraction of 1 means 100%.' : '') };
}
