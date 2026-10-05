import { expect, it } from 'vitest';
import { curveAxes } from './curve-axes';
import type { EditorNode, PlatformEdge } from './types';

const node = (id: string, typeId: string): EditorNode => ({ id, typeId, position: { x: 0, y: 0 }, parameters: {}, descriptor: {
  typeId, name: typeId, version: 1, inputs: [], outputs: [], parameters: [],
} });
it('labels image channel levels without pretending scalar curves measure brightness', () => {
  for (const typeId of ['core.curves', 'core.levels', 'pro.lut', 'pro.lut-tools', 'pro.film-curve', 'pro.film-simulation']) {
    const axes = curveAxes(node('curve', typeId));
    expect(axes.x).toBe('RGB level before curve');
    expect(axes.y).toBe('RGB level after curve');
    expect(axes.description).toContain('dark to bright');
    expect(axes.description).toContain('not luminance');
  }
  const scalar = curveAxes(node('curve', 'core.curve'));
  expect(scalar.description).toContain('not image brightness');
  expect(scalar.x).not.toContain('RGB');
});
it('uses actual metadata units and connected parameter names', () => {
  const curve = node('curve', 'core.curve');
  const metadata = node('metadata', 'core.metadata');
  metadata.descriptor.outputs = [{ id: 'iso', name: 'ISO', dataType: 'value.Integer', required: false }];
  const recovery = node('recovery', 'raw.highlight-reconstruction');
  recovery.descriptor.parameters = [{ id: 'strength', name: 'Recovery Strength', parameterType: 'Float', default: 1, min: 0, max: 1 }];
  const edges: PlatformEdge[] = [
    { fromNode: 'metadata', fromPort: 'iso', toNode: 'curve', toPort: 'value' },
    { fromNode: 'curve', fromPort: 'value', toNode: 'recovery', toPort: 'strength' },
  ];
  expect(curveAxes(curve, [curve, metadata, recovery], edges)).toMatchObject({ x: 'ISO', y: 'Recovery Strength (fraction)' });
  metadata.descriptor.outputs = [{ id: 'shutter_seconds', name: 'Shutter Speed', dataType: 'value.Float', required: false }];
  edges[0].fromPort = 'shutter_seconds';
  expect(curveAxes(curve, [curve, metadata, recovery], edges).x).toBe('Shutter time (s)');
});
it('does not invent units for untyped or differently-targeted scalar values', () => {
  const curve = node('curve', 'core.curve');
  const exposure = node('exposure', 'core.exposure');
  exposure.descriptor.parameters = [{ id: 'exposure', name: 'Exposure', parameterType: 'Float', default: 0, min: null, max: null }];
  const blur = node('blur', 'core.blur');
  blur.descriptor.parameters = [{ id: 'radius', name: 'Radius', parameterType: 'Float', default: 1, min: 0, max: null }];
  const edges = [{ fromNode: 'curve', fromPort: 'value', toNode: 'exposure', toPort: 'exposure' }];
  expect(curveAxes(curve, [curve, exposure], edges).y).toBe('Exposure (EV)');
  edges.push({ fromNode: 'curve', fromPort: 'value', toNode: 'blur', toPort: 'radius' });
  expect(curveAxes(curve, [curve, exposure, blur], edges).y).toBe('Mapped control value');
});
