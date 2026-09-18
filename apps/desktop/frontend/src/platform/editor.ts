import type {
  EditorPlatform,
  NodeDescriptor,
  ParameterValue,
  PlatformEdge,
  PlatformNode,
  PlatformSnapshot,
} from '../editor/types';
import { createTauriPlatform } from './tauri';

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

const imageInput: NodeDescriptor = {
  typeId: 'core.image-input',
  name: 'Image Input',
  version: 1,
  inputs: [],
  outputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: false }],
  parameters: [],
};

const exposure: NodeDescriptor = {
  typeId: 'core.exposure',
  name: 'Exposure',
  version: 1,
  inputs: [
    { id: 'image', name: 'Image', dataType: 'core.Image', required: true },
    { id: 'exposure', name: 'Exposure', dataType: 'value.Float', required: false },
  ],
  outputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: false }],
  parameters: [
    { id: 'exposure', name: 'Exposure', parameterType: 'Float', default: 0, min: null, max: null },
  ],
};

const invert: NodeDescriptor = {
  typeId: 'core.invert',
  name: 'Invert',
  version: 1,
  inputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: true }],
  outputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: false }],
  parameters: [],
};

const output: NodeDescriptor = {
  typeId: 'core.output',
  name: 'Output',
  version: 1,
  inputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: true }],
  outputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: false }],
  parameters: [],
};

const constantFloat: NodeDescriptor = {
  typeId: 'core.constant-float',
  name: 'Constant Float',
  version: 1,
  inputs: [],
  outputs: [{ id: 'value', name: 'Value', dataType: 'value.Float', required: false }],
  parameters: [{ id: 'value', name: 'Value', parameterType: 'Float', default: 0, min: null, max: null }],
};

export const builtInDescriptors: NodeDescriptor[] = [imageInput, constantFloat, exposure, invert, output];

function descriptorFor(descriptors: NodeDescriptor[], typeId: string): NodeDescriptor {
  const descriptor = descriptors.find((candidate) => candidate.typeId === typeId);
  if (!descriptor) {
    throw new Error(`node type '${typeId}' is not registered`);
  }
  return descriptor;
}

function port(descriptor: NodeDescriptor, direction: 'input' | 'output', portId: string) {
  const ports = direction === 'input' ? descriptor.inputs : descriptor.outputs;
  const found = ports.find((candidate) => candidate.id === portId);
  if (!found) {
    throw new Error(`port '${portId}' does not exist on node '${descriptor.typeId}'`);
  }
  return found;
}

function hasCycle(edges: PlatformEdge[]): boolean {
  const adjacency = new Map<string, string[]>();
  for (const edge of edges) {
    const children = adjacency.get(edge.fromNode) ?? [];
    children.push(edge.toNode);
    adjacency.set(edge.fromNode, children);
  }
  const visiting = new Set<string>();
  const visited = new Set<string>();
  const visit = (nodeId: string): boolean => {
    if (visiting.has(nodeId)) return true;
    if (visited.has(nodeId)) return false;
    visiting.add(nodeId);
    if ((adjacency.get(nodeId) ?? []).some(visit)) return true;
    visiting.delete(nodeId);
    visited.add(nodeId);
    return false;
  };
  return [...new Set(edges.flatMap((edge) => [edge.fromNode, edge.toNode]))].some(visit);
}

export function createMemoryPlatform(): EditorPlatform {
  const descriptors = clone(builtInDescriptors);
  const nodes = new Map<string, PlatformNode>();
  let edges: PlatformEdge[] = [];

  const snapshot = (): PlatformSnapshot => ({
    nodes: clone([...nodes.values()]),
    edges: clone(edges),
  });

  return {
    async nodeDescriptors() {
      return clone(descriptors);
    },
    async snapshot() {
      return snapshot();
    },
    async addNode(nodeId, typeId) {
      if (!nodeId) throw new Error('node identifier cannot be empty');
      if (nodes.has(nodeId)) throw new Error(`node '${nodeId}' already exists`);
      const descriptor = descriptorFor(descriptors, typeId);
      nodes.set(nodeId, {
        id: nodeId,
        typeId,
        parameters: Object.fromEntries(
          descriptor.parameters.map((parameter) => [parameter.id, clone(parameter.default)]),
        ),
      });
    },
    async removeNode(nodeId) {
      if (!nodes.delete(nodeId)) throw new Error(`node '${nodeId}' does not exist`);
      edges = edges.filter((edge) => edge.fromNode !== nodeId && edge.toNode !== nodeId);
    },
    async connect(fromNode, fromPort, toNode, toPort) {
      const source = nodes.get(fromNode);
      const target = nodes.get(toNode);
      if (!source) throw new Error(`node '${fromNode}' does not exist`);
      if (!target) throw new Error(`node '${toNode}' does not exist`);
      const sourcePort = port(descriptorFor(descriptors, source.typeId), 'output', fromPort);
      const targetPort = port(descriptorFor(descriptors, target.typeId), 'input', toPort);
      if (sourcePort.dataType !== targetPort.dataType) {
        throw new Error(`cannot connect '${sourcePort.dataType}' to '${targetPort.dataType}'`);
      }
      if (edges.some((edge) => edge.toNode === toNode && edge.toPort === toPort)) {
        throw new Error(`input '${toNode}:${toPort}' already has a connection`);
      }
      const candidate = [...edges, { fromNode, fromPort, toNode, toPort }];
      if (hasCycle(candidate)) throw new Error('connection would create a cycle');
      edges = candidate;
    },
    async disconnect(fromNode, fromPort, toNode, toPort) {
      const index = edges.findIndex(
        (edge) =>
          edge.fromNode === fromNode &&
          edge.fromPort === fromPort &&
          edge.toNode === toNode &&
          edge.toPort === toPort,
      );
      if (index < 0) throw new Error('connection does not exist');
      edges = edges.filter((_, edgeIndex) => edgeIndex !== index);
    },
    async setParameter(nodeId, parameterId, value: ParameterValue) {
      const node = nodes.get(nodeId);
      if (!node) throw new Error(`node '${nodeId}' does not exist`);
      const parameter = descriptorFor(descriptors, node.typeId).parameters.find(
        (candidate) => candidate.id === parameterId,
      );
      if (!parameter) throw new Error(`parameter '${parameterId}' does not exist on node '${nodeId}'`);
      const validType =
        (parameter.parameterType === 'Float' && typeof value === 'number') ||
        (parameter.parameterType === 'Boolean' && typeof value === 'boolean') ||
        (parameter.parameterType === 'String' && typeof value === 'string');
      if (!validType) throw new Error(`parameter '${parameterId}' on node '${nodeId}' has the wrong type`);
      if (
        typeof value === 'number' &&
        ((parameter.min !== null && value < parameter.min) ||
          (parameter.max !== null && value > parameter.max))
      ) {
        throw new Error(`parameter '${parameterId}' on node '${nodeId}' is outside its allowed range`);
      }
      node.parameters[parameterId] = value;
    },
    async saveWorkflow() {
      return JSON.stringify(snapshot());
    },
    async loadWorkflow(serialized) {
      const parsed = JSON.parse(serialized) as PlatformSnapshot;
      if (!Array.isArray(parsed.nodes) || !Array.isArray(parsed.edges)) {
        throw new Error('invalid workflow document');
      }
      nodes.clear();
      for (const node of parsed.nodes) {
        descriptorFor(descriptors, node.typeId);
        nodes.set(node.id, clone(node));
      }
      edges = clone(parsed.edges);
      for (const edge of edges) {
        const source = nodes.get(edge.fromNode);
        const target = nodes.get(edge.toNode);
        if (!source || !target) throw new Error('workflow contains an unknown node');
        const sourcePort = port(descriptorFor(descriptors, source.typeId), 'output', edge.fromPort);
        const targetPort = port(descriptorFor(descriptors, target.typeId), 'input', edge.toPort);
        if (sourcePort.dataType !== targetPort.dataType) throw new Error('workflow contains an invalid connection');
      }
      if (hasCycle(edges)) throw new Error('workflow contains a cycle');
    },
  };
}

export function createPlatform(): EditorPlatform {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    return createTauriPlatform();
  }
  return createMemoryPlatform();
}
