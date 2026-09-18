import type {
  EditorPlatform,
  ExecutionCapability,
  NodeDescriptor,
  OpenImageResult,
  ParameterValue,
  PlatformEdge,
  PortDescriptor,
  PlatformNode,
  PlatformSnapshot,
} from '../editor/types';
import { createTauriPlatform } from './tauri';

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

const imageCapabilities: ExecutionCapability[] = ['CPU', 'RegionAware'];
const tileCapabilities: ExecutionCapability[] = ['CPU', 'TileLocal', 'RegionAware'];
const fullFrameCapabilities: ExecutionCapability[] = ['CPU', 'FullFrame', 'RegionAware'];

function imageInputDescriptor(): NodeDescriptor {
  return {
    typeId: 'core.image-input',
    name: 'Image Input',
    version: 1,
    inputs: [],
    outputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: false }],
    parameters: [],
    capabilities: imageCapabilities,
  };
}

function imageProcessingDescriptor(typeId: string, name: string): NodeDescriptor {
  return {
    typeId,
    name,
    version: 1,
    inputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: true }],
    outputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: false }],
    parameters: [],
    capabilities: imageCapabilities,
  };
}

function parameter(id: string, name: string, defaultValue: number, min: number | null = null, max: number | null = null) {
  return { id, name, parameterType: 'Float' as const, default: defaultValue, min, max };
}

function stringParameter(id: string, name: string, defaultValue: string) {
  return { id, name, parameterType: 'String' as const, default: defaultValue, min: null, max: null };
}

function input(id: string, name: string, dataType: string, required: boolean): PortDescriptor {
  return { id, name, dataType, required };
}

function outputPort(id: string, name: string, dataType: string): PortDescriptor {
  return { id, name, dataType, required: false };
}

const rawCapabilities: ExecutionCapability[] = ['CPU', 'FullFrame'];

const rawDescriptors: NodeDescriptor[] = [
  {
    typeId: 'raw.decode',
    name: 'RAW Decode',
    version: 1,
    inputs: [input('bytes', 'RAW Bytes', 'core.Bytes', false)],
    outputs: [
      outputPort('frame', 'RAW Frame', 'raw.Frame'),
      outputPort('mosaic', 'Mosaic', 'raw.Mosaic'),
      outputPort('camera', 'Camera Metadata', 'raw.CameraMetadata'),
      outputPort('camera_profile', 'Camera Profile', 'raw.CameraProfile'),
      outputPort('lens_profile', 'Lens Profile', 'raw.LensProfile'),
      outputPort('exif', 'EXIF Metadata', 'raw.ExifMetadata'),
      outputPort('preview', 'Embedded Preview', 'raw.EmbeddedPreview'),
    ],
    parameters: [],
    capabilities: rawCapabilities,
  },
  {
    typeId: 'raw.black-level',
    name: 'Black Level',
    version: 1,
    inputs: [input('frame', 'RAW Frame', 'raw.Frame', true)],
    outputs: [outputPort('mosaic', 'Mosaic', 'raw.Mosaic')],
    parameters: [],
    capabilities: rawCapabilities,
  },
  {
    typeId: 'raw.white-balance',
    name: 'White Balance',
    version: 1,
    inputs: [input('mosaic', 'Mosaic', 'raw.Mosaic', true)],
    outputs: [outputPort('mosaic', 'Mosaic', 'raw.Mosaic')],
    parameters: [
      parameter('red_gain', 'Red Gain', 1, 0),
      parameter('green_gain', 'Green Gain', 1, 0),
      parameter('blue_gain', 'Blue Gain', 1, 0),
    ],
    capabilities: rawCapabilities,
  },
  {
    typeId: 'raw.highlight-reconstruction',
    name: 'Highlight Reconstruction',
    version: 1,
    inputs: [input('mosaic', 'Mosaic', 'raw.Mosaic', true)],
    outputs: [outputPort('mosaic', 'Mosaic', 'raw.Mosaic')],
    parameters: [parameter('threshold', 'Threshold', 1, 0), parameter('strength', 'Recovery Strength', 1, 0, 1)],
    capabilities: rawCapabilities,
  },
  {
    typeId: 'raw.demosaic',
    name: 'Demosaic',
    version: 1,
    inputs: [input('mosaic', 'Mosaic', 'raw.Mosaic', true)],
    outputs: [outputPort('scene', 'Scene Linear RGB', 'color.SceneLinearRGB')],
    parameters: [],
    capabilities: rawCapabilities,
  },
  {
    typeId: 'raw.camera-transform',
    name: 'Camera Transform',
    version: 1,
    inputs: [
      input('scene', 'Scene Linear RGB', 'color.SceneLinearRGB', true),
      input('camera_profile', 'Camera Profile', 'raw.CameraProfile', false),
    ],
    outputs: [outputPort('scene', 'Scene Linear RGB', 'color.SceneLinearRGB')],
    parameters: [stringParameter('working_space', 'Working Space', 'sRGB')],
    capabilities: rawCapabilities,
  },
  {
    typeId: 'raw.lens-correction',
    name: 'Lens Correction',
    version: 1,
    inputs: [
      input('scene', 'Scene Linear RGB', 'color.SceneLinearRGB', true),
      input('lens_profile', 'Lens Profile', 'raw.LensProfile', false),
    ],
    outputs: [outputPort('scene', 'Scene Linear RGB', 'color.SceneLinearRGB')],
    parameters: [],
    capabilities: rawCapabilities,
  },
  {
    typeId: 'raw.display-transform',
    name: 'Display Transform',
    version: 1,
    inputs: [input('scene', 'Scene Linear RGB', 'color.SceneLinearRGB', true)],
    outputs: [outputPort('display', 'Display RGB', 'color.DisplayRGB')],
    parameters: [],
    capabilities: rawCapabilities,
  },
];

const imageInput = imageInputDescriptor();
const constantFloat: NodeDescriptor = {
  typeId: 'core.constant-float',
  name: 'Constant Float',
  version: 1,
  inputs: [],
  outputs: [{ id: 'value', name: 'Value', dataType: 'value.Float', required: false }],
  parameters: [parameter('value', 'Value', 0)],
};

const exposure = imageProcessingDescriptor('core.exposure', 'Exposure');
exposure.inputs.push({ id: 'exposure', name: 'Exposure', dataType: 'value.Float', required: false });
exposure.parameters.push(parameter('exposure', 'Exposure', 0));
exposure.capabilities = tileCapabilities;

const invert = imageProcessingDescriptor('core.invert', 'Invert');
const output = imageProcessingDescriptor('core.output', 'Output');

const resize = imageProcessingDescriptor('core.resize', 'Resize');
resize.parameters.push(parameter('width', 'Width', 1, 1));
resize.parameters.push(parameter('height', 'Height', 1, 1));

const crop = imageProcessingDescriptor('core.crop', 'Crop');
crop.parameters.push(parameter('x', 'X', 0, 0));
crop.parameters.push(parameter('y', 'Y', 0, 0));
crop.parameters.push(parameter('width', 'Width', 1, 1));
crop.parameters.push(parameter('height', 'Height', 1, 1));

const blur = imageProcessingDescriptor('core.blur', 'Blur');
blur.parameters.push(parameter('radius', 'Radius', 1, 0, 64));
blur.capabilities = fullFrameCapabilities;

const levels = imageProcessingDescriptor('core.levels', 'Levels');
levels.parameters.push(parameter('black_point', 'Black Point', 0));
levels.parameters.push(parameter('white_point', 'White Point', 1));
levels.parameters.push(parameter('gamma', 'Gamma', 1, 0.0001));

const curves = imageProcessingDescriptor('core.curves', 'Curves');
curves.parameters.push(parameter('gamma', 'Gamma', 1, 0.0001));

const colorMatrix = imageProcessingDescriptor('core.color-matrix', 'Color Matrix');
colorMatrix.capabilities = ['CPU', 'GPU', 'TileLocal', 'RegionAware'];
for (let row = 0; row < 4; row += 1) {
  for (let column = 0; column < 4; column += 1) {
    colorMatrix.parameters.push(parameter(`m${row}${column}`, `Matrix ${row}${column}`, row === column ? 1 : 0));
  }
}
for (const [id, name] of [
  ['offset_r', 'Red Offset'],
  ['offset_g', 'Green Offset'],
  ['offset_b', 'Blue Offset'],
  ['offset_a', 'Alpha Offset'],
] as const) {
  colorMatrix.parameters.push(parameter(id, name, 0));
}

export const builtInDescriptors: NodeDescriptor[] = [
  imageInput,
  constantFloat,
  exposure,
  invert,
  resize,
  crop,
  blur,
  levels,
  curves,
  colorMatrix,
  output,
  ...rawDescriptors,
];

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
  let revision = 0;

  const snapshot = (): PlatformSnapshot => ({
    nodes: clone([...nodes.values()]),
    edges: clone(edges),
    revision,
  });

  return {
    async nodeDescriptors() {
      return clone(descriptors);
    },
    async snapshot() {
      return snapshot();
    },
    async chooseImagePath() {
      return null;
    },
    async openImage(_path): Promise<OpenImageResult> {
      const existing = [...nodes.keys()];
      for (const nodeId of existing) await this.removeNode(nodeId);
      const inputDescriptor = descriptorFor(descriptors, 'core.image-input');
      const outputDescriptor = descriptorFor(descriptors, 'core.output');
      nodes.set('input', {
        id: 'input',
        typeId: inputDescriptor.typeId,
        parameters: {},
      });
      revision += 1;
      nodes.set('output', {
        id: 'output',
        typeId: outputDescriptor.typeId,
        parameters: {},
      });
      revision += 1;
      edges = [{ fromNode: 'input', fromPort: 'image', toNode: 'output', toPort: 'image' }];
      revision += 1;
      return {
        kind: 'ordinary',
        width: 1,
        height: 1,
        revision,
        metadata: null,
      };
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
      revision += 1;
    },
    async removeNode(nodeId) {
      if (!nodes.delete(nodeId)) throw new Error(`node '${nodeId}' does not exist`);
      edges = edges.filter((edge) => edge.fromNode !== nodeId && edge.toNode !== nodeId);
      revision += 1;
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
      revision += 1;
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
      revision += 1;
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
      revision += 1;
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
      revision += 1;
    },
  };
}

export function createPlatform(): EditorPlatform {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    return createTauriPlatform();
  }
  return createMemoryPlatform();
}
