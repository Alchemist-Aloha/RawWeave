import { describe, expect, it } from 'vitest';
import {
  checkConnection,
  createsCycle,
  dataTypesCompatible,
  inputDataType,
  outputDataType,
  parameterDataType,
} from './connections';
import type { EditorEdge, EditorNode, NodeDescriptor } from './types';

function descriptor(overrides: Partial<NodeDescriptor> = {}): NodeDescriptor {
  return {
    typeId: 'test.node',
    name: 'Test',
    version: 1,
    inputs: [],
    outputs: [],
    parameters: [],
    ...overrides,
  };
}

function node(id: string, overrides: Partial<NodeDescriptor> = {}, exposedParameters: string[] = []): EditorNode {
  return {
    id,
    typeId: overrides.typeId ?? 'test.node',
    parameters: {},
    position: { x: 0, y: 0 },
    exposedParameters,
    descriptor: descriptor(overrides),
  };
}

function edge(fromNode: string, toNode: string, toPort = 'image'): EditorEdge {
  return {
    id: `${fromNode}:image->${toNode}:${toPort}`,
    fromNode,
    fromPort: 'image',
    toNode,
    toPort,
    source: fromNode,
    sourceHandle: 'image',
    target: toNode,
    targetHandle: toPort,
  };
}

const IMAGE_OUT = { id: 'image', name: 'Image', dataType: 'core.Image', required: false };
const IMAGE_IN = { id: 'image', name: 'Image', dataType: 'core.Image', required: true };
const FLOAT_OUT = { id: 'value', name: 'Value', dataType: 'value.Float', required: false };

describe('connection rules', () => {
  it('mirrors the backend type compatibility table', () => {
    expect(dataTypesCompatible('core.Image', 'core.Image')).toBe(true);
    expect(dataTypesCompatible('core.Any', 'core.Image')).toBe(true);
    expect(dataTypesCompatible('core.Image', 'core.Any')).toBe(true);
    expect(dataTypesCompatible('value.Float', 'value.Integer')).toBe(true);
    expect(dataTypesCompatible('value.Integer', 'value.Float')).toBe(true);
    expect(dataTypesCompatible('value.Condition', 'value.Boolean')).toBe(true);
    expect(dataTypesCompatible('core.Image', 'value.Float')).toBe(false);
    // The backend has no '*' alias, so neither does the canvas.
    expect(dataTypesCompatible('*', 'core.Image')).toBe(false);
  });

  it('maps parameter types to port data types', () => {
    expect(parameterDataType('Float')).toBe('value.Float');
    expect(parameterDataType('Integer')).toBe('value.Integer');
    expect(parameterDataType('Boolean')).toBe('value.Boolean');
    expect(parameterDataType('String')).toBe('value.String');
  });

  it('resolves static inputs and exposed parameter ports', () => {
    const source = node('source', { outputs: [IMAGE_OUT] });
    const target = node(
      'target',
      { inputs: [IMAGE_IN], parameters: [{ id: 'radius', name: 'Radius', parameterType: 'Float', default: 1, min: null, max: null }] },
      ['radius'],
    );

    expect(outputDataType(source, 'image')).toBe('core.Image');
    expect(outputDataType(source, 'missing')).toBeNull();
    expect(inputDataType(target, 'image')).toBe('core.Image');
    expect(inputDataType(target, 'radius')).toBe('value.Float');
    expect(inputDataType(target, 'missing')).toBeNull();
  });

  it('detects cycles', () => {
    const edges = [edge('a', 'b'), edge('b', 'c')];
    expect(createsCycle(edges, 'c', 'a')).toBe(true);
    expect(createsCycle(edges, 'a', 'c')).toBe(false);
  });

  it('accepts a valid connection', () => {
    const nodes = [node('a', { outputs: [IMAGE_OUT] }), node('b', { inputs: [IMAGE_IN] })];
    expect(checkConnection(nodes, [], { source: 'a', sourceHandle: 'image', target: 'b', targetHandle: 'image' }))
      .toEqual({ valid: true, reason: null });
  });

  it('explains each refusal like the backend would', () => {
    const nodes = [
      node('a', { outputs: [IMAGE_OUT] }),
      node('b', { inputs: [IMAGE_IN] }),
      node('c', { inputs: [IMAGE_IN] }),
    ];
    const cases: Array<[Parameters<typeof checkConnection>[2], RegExp]> = [
      [{ source: 'a', sourceHandle: 'image', target: 'a', targetHandle: 'image' }, /itself/],
      [{ source: 'a', sourceHandle: 'nope', target: 'b', targetHandle: 'image' }, /Output 'nope'/],
      [{ source: 'a', sourceHandle: 'image', target: 'b', targetHandle: 'nope' }, /Input 'nope'/],
      [{ source: 'a', sourceHandle: 'image', target: 'b', targetHandle: 'image', id: 'other' }, /already has a connection/],
      [{ source: 'a', sourceHandle: 'image', target: 'b', targetHandle: 'image' }, /already has a connection/],
    ];
    const edges = [edge('a', 'b')];
    for (const [draft, pattern] of cases) {
      const result = checkConnection(nodes, edges, draft);
      expect(result.valid).toBe(false);
      expect(result.reason ?? '').toMatch(pattern);
    }
  });

  it('lets a reconnecting edge keep its own input', () => {
    const nodes = [node('a', { outputs: [IMAGE_OUT] }), node('b', { inputs: [IMAGE_IN] })];
    const edges = [edge('a', 'b')];
    expect(checkConnection(nodes, edges, {
      source: 'a',
      sourceHandle: 'image',
      target: 'b',
      targetHandle: 'image',
      id: edges[0].id,
    }).valid).toBe(true);
  });

  it('refuses a type mismatch and a cycle', () => {
    const nodes = [
      node('image', { outputs: [IMAGE_OUT] }),
      node('scalar', { outputs: [FLOAT_OUT] }),
      node('b', { inputs: [IMAGE_IN] }),
      node('c', { outputs: [IMAGE_OUT], inputs: [IMAGE_IN] }),
    ];
    expect(checkConnection(nodes, [], { source: 'scalar', sourceHandle: 'value', target: 'b', targetHandle: 'image' }).reason)
      .toMatch(/Cannot connect value\.Float to core\.Image/);
    expect(checkConnection(nodes, [edge('b', 'c', 'image')], {
      source: 'c',
      sourceHandle: 'image',
      target: 'b',
      targetHandle: 'image',
    }).reason).toMatch(/cycle/);
  });
});
