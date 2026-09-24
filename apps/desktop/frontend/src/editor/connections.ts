import type { EditorEdge, EditorNode, ParameterType } from './types';

/** Mirrors `parameterDataType` in the desktop platform adapter. */
export function parameterDataType(parameterType: ParameterType): string {
  switch (parameterType) {
    case 'Float':
      return 'value.Float';
    case 'Integer':
      return 'value.Integer';
    case 'Boolean':
      return 'value.Boolean';
    default:
      return 'value.String';
  }
}

/**
 * Mirrors the backend's `typesCompatible` rule exactly.
 *
 * Keep this in sync with `apps/desktop/src-tauri` and the memory platform: the
 * canvas uses it to refuse connections the backend would reject, so a drift
 * here shows up as a connection that looks legal and then errors.
 */
export function dataTypesCompatible(expected: string, actual: string): boolean {
  if (expected === actual || expected === 'core.Any' || actual === 'core.Any') return true;
  return (
    (expected === 'value.Float' && actual === 'value.Integer')
    || (expected === 'value.Integer' && actual === 'value.Float')
    || (expected === 'value.Condition' && actual === 'value.Boolean')
    || (expected === 'value.Boolean' && actual === 'value.Condition')
  );
}

/** Data type produced by an output port, or null when the port is unknown. */
export function outputDataType(node: EditorNode | undefined, portId: string | null): string | null {
  const port = node?.descriptor.outputs.find((candidate) => candidate.id === portId);
  return port?.dataType ?? null;
}

/**
 * Data type expected by a connection target. A target is either a static input
 * port or a parameter the node exposes as a port, matching the backend.
 */
export function inputDataType(node: EditorNode | undefined, portId: string | null): string | null {
  if (!node || !portId) return null;
  const input = node.descriptor.inputs.find((candidate) => candidate.id === portId);
  if (input) return input.dataType;
  if (node.exposedParameters?.includes(portId)) {
    const parameter = node.descriptor.parameters.find((candidate) => candidate.id === portId);
    if (parameter) return parameterDataType(parameter.parameterType);
  }
  return null;
}

export interface ConnectionDraft {
  source?: string | null;
  sourceHandle?: string | null;
  target?: string | null;
  targetHandle?: string | null;
  /** Set while reconnecting an existing edge so it may keep its own input. */
  id?: string | null;
}

export interface ConnectionCheck {
  valid: boolean;
  reason: string | null;
}

/** True when `fromNode -> toNode` would close a cycle. */
export function createsCycle(edges: EditorEdge[], fromNode: string, toNode: string): boolean {
  const adjacency = new Map<string, string[]>();
  for (const edge of edges) {
    const children = adjacency.get(edge.fromNode) ?? [];
    children.push(edge.toNode);
    adjacency.set(edge.fromNode, children);
  }
  const stack = [toNode];
  const seen = new Set<string>();
  while (stack.length > 0) {
    const current = stack.pop() as string;
    if (current === fromNode) return true;
    if (seen.has(current)) continue;
    seen.add(current);
    stack.push(...(adjacency.get(current) ?? []));
  }
  return false;
}

/**
 * Mirrors the platform's `connect()` rules so React Flow never offers a
 * connection the backend would reject. Also used to explain a refusal, which
 * the canvas surfaces in the context menu and status bar.
 */
export function checkConnection(
  nodes: EditorNode[],
  edges: EditorEdge[],
  draft: ConnectionDraft,
): ConnectionCheck {
  const { source, sourceHandle, target, targetHandle } = draft;
  if (!source || !target || !sourceHandle || !targetHandle) {
    return { valid: false, reason: 'Drop on a matching port to connect' };
  }
  if (source === target) {
    return { valid: false, reason: 'A node cannot connect to itself' };
  }
  const sourceNode = nodes.find((node) => node.id === source);
  const targetNode = nodes.find((node) => node.id === target);
  if (!sourceNode || !targetNode) {
    return { valid: false, reason: 'Unknown node' };
  }
  const actual = outputDataType(sourceNode, sourceHandle);
  if (!actual) {
    return { valid: false, reason: `Output '${sourceHandle}' does not exist` };
  }
  const expected = inputDataType(targetNode, targetHandle);
  if (!expected) {
    return { valid: false, reason: `Input '${targetHandle}' does not exist` };
  }
  if (!dataTypesCompatible(expected, actual)) {
    return { valid: false, reason: `Cannot connect ${actual} to ${expected}` };
  }
  const occupied = edges.find((edge) => edge.toNode === target && edge.toPort === targetHandle);
  if (occupied && occupied.id !== draft.id) {
    return { valid: false, reason: `'${targetHandle}' already has a connection` };
  }
  if (createsCycle(edges, source, target)) {
    return { valid: false, reason: 'That would create a cycle' };
  }
  return { valid: true, reason: null };
}
