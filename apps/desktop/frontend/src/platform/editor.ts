import type {
  EditorPlatform,
  ExecutionCapability,
  NodeDescriptor,
  OpenImageResult,
  ParameterType,
  ParameterValue,
  PlatformEdge,
  PortDescriptor,
  PlatformNode,
  PlatformSnapshot,
  WorkflowDefinition,
  WorkflowDependency,
  WorkflowMetadata,
  WorkflowParameter,
  WorkflowPort,
  WorkflowSummary,
  ScopeBreadcrumb,
  DependencyDiagnostic,
  DependencyReport,
  CreateSubgraphOptions,
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

function integerParameter(id: string, name: string, defaultValue: number) {
  return { id, name, parameterType: 'Integer' as const, default: defaultValue, min: null, max: null };
}

function booleanParameter(id: string, name: string, defaultValue: boolean) {
  return { id, name, parameterType: 'Boolean' as const, default: defaultValue, min: null, max: null };
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

function controlDescriptor(
  typeId: string,
  name: string,
  inputs: PortDescriptor[],
  outputs: PortDescriptor[],
  parameters: NodeDescriptor['parameters'] = [],
): NodeDescriptor {
  return { typeId, name, version: 1, inputs, outputs, parameters, capabilities: ['CPU'] };
}

const conditionInput = input('value', 'Value', 'value.Condition', true);

const logicDescriptors: NodeDescriptor[] = [
  controlDescriptor(
    'core.constant-integer',
    'Constant Integer',
    [],
    [outputPort('value', 'Value', 'value.Integer')],
    [integerParameter('value', 'Value', 0)],
  ),
  controlDescriptor(
    'core.constant-boolean',
    'Constant Boolean',
    [],
    [outputPort('value', 'Value', 'value.Boolean')],
    [booleanParameter('value', 'Value', false)],
  ),
  controlDescriptor(
    'core.constant-string',
    'Constant String',
    [],
    [outputPort('value', 'Value', 'value.String')],
    [stringParameter('value', 'Value', '')],
  ),
  controlDescriptor(
    'core.metadata',
    'Metadata',
    [
      input('camera', 'Camera Metadata', 'raw.CameraMetadata', false),
      input('exif', 'EXIF Metadata', 'raw.ExifMetadata', false),
    ],
    [
      outputPort('metadata', 'Metadata', 'core.Metadata'),
      outputPort('make', 'Make', 'value.String'),
      outputPort('model', 'Model', 'value.String'),
      outputPort('camera_model', 'Camera Model', 'value.String'),
      outputPort('lens_model', 'Lens Model', 'value.String'),
      outputPort('iso', 'ISO', 'value.Integer'),
      outputPort('aperture', 'Aperture', 'value.Float'),
      outputPort('shutter_seconds', 'Shutter Speed', 'value.Float'),
      outputPort('focal_length', 'Focal Length', 'value.Float'),
      outputPort('capture_time', 'Capture Time', 'value.String'),
      outputPort('orientation', 'Orientation', 'value.String'),
      outputPort('rating', 'Rating', 'value.Integer'),
    ],
  ),
  ...[compareDescriptor('core.equal', 'Equal'), compareDescriptor('core.greater-than', 'Greater Than'), compareDescriptor('core.less-than', 'Less Than')],
  controlDescriptor(
    'core.compare',
    'Compare',
    [input('a', 'A', 'value.Float', true), input('b', 'B', 'value.Float', true)],
    [outputPort('result', 'Result', 'value.Condition')],
    [parameter('epsilon', 'Epsilon', 0.000001, 0), stringParameter('operation', 'Operation', '==')],
  ),
  controlDescriptor(
    'core.and',
    'And',
    [input('a', 'A', 'value.Condition', true), input('b', 'B', 'value.Condition', true)],
    [outputPort('result', 'Result', 'value.Condition')],
  ),
  controlDescriptor(
    'core.or',
    'Or',
    [input('a', 'A', 'value.Condition', true), input('b', 'B', 'value.Condition', true)],
    [outputPort('result', 'Result', 'value.Condition')],
  ),
  controlDescriptor('core.not', 'Not', [conditionInput], [outputPort('result', 'Result', 'value.Condition')]),
  controlDescriptor(
    'core.switch',
    'Switch',
    [
      input('condition', 'Condition', 'value.Condition', true),
      input('true', 'True', 'core.Any', false),
      input('false', 'False', 'core.Any', false),
    ],
    [outputPort('value', 'Value', 'core.Any')],
  ),
  controlDescriptor(
    'core.select',
    'Select',
    [
      input('index', 'Index', 'value.Integer', true),
      input('a', 'A', 'core.Any', false),
      input('b', 'B', 'core.Any', false),
      input('c', 'C', 'core.Any', false),
      input('d', 'D', 'core.Any', false),
    ],
    [outputPort('value', 'Value', 'core.Any')],
  ),
  controlDescriptor(
    'core.enum-select',
    'Enum Select',
    [
      input('selector', 'Selector', 'value.String', true),
      input('a', 'A', 'core.Any', false),
      input('b', 'B', 'core.Any', false),
      input('c', 'C', 'core.Any', false),
      input('d', 'D', 'core.Any', false),
    ],
    [outputPort('value', 'Value', 'core.Any')],
    [
      stringParameter('match_a', 'A Matches', ''),
      stringParameter('match_b', 'B Matches', ''),
      stringParameter('match_c', 'C Matches', ''),
      stringParameter('match_d', 'D Matches', ''),
    ],
  ),
  controlDescriptor(
    'core.map-range',
    'Map Range',
    [input('value', 'Value', 'value.Float', true)],
    [outputPort('value', 'Value', 'value.Float')],
    [
      parameter('in_min', 'Input Minimum', 0),
      parameter('in_max', 'Input Maximum', 1),
      parameter('out_min', 'Output Minimum', 0),
      parameter('out_max', 'Output Maximum', 1),
      booleanParameter('clamp', 'Clamp', false),
    ],
  ),
  controlDescriptor(
    'core.clamp',
    'Clamp',
    [input('value', 'Value', 'value.Float', true)],
    [outputPort('value', 'Value', 'value.Float')],
    [parameter('min', 'Minimum', 0), parameter('max', 'Maximum', 1)],
  ),
  controlDescriptor(
    'core.curve',
    'Curve',
    [input('value', 'Value', 'value.Float', true)],
    [outputPort('value', 'Value', 'value.Float')],
    [stringParameter('points', 'Control Points', '0,0;1,1')],
  ),
  controlDescriptor(
    'core.expression',
    'Expression',
    [
      input('a', 'A', 'value.Float', false),
      input('b', 'B', 'value.Float', false),
      input('c', 'C', 'value.Float', false),
      input('d', 'D', 'value.Float', false),
    ],
    [outputPort('value', 'Value', 'value.Float')],
    [stringParameter('expression', 'Expression', '0')],
  ),
  controlDescriptor(
    'core.string-match',
    'String Match',
    [input('value', 'Value', 'value.String', true)],
    [outputPort('result', 'Result', 'value.Condition')],
    [stringParameter('pattern', 'Pattern', ''), booleanParameter('case_sensitive', 'Case Sensitive', true)],
  ),
];

function compareDescriptor(typeId: string, name: string): NodeDescriptor {
  return controlDescriptor(
    typeId,
    name,
    [input('a', 'A', 'value.Float', true), input('b', 'B', 'value.Float', true)],
    [outputPort('result', 'Result', 'value.Condition')],
    [parameter('epsilon', 'Epsilon', 0.000001, 0)],
  );
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
  ...logicDescriptors,
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

function parameterDataType(parameterType: ParameterType): string {
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

function typesCompatible(expected: string, actual: string): boolean {
  if (expected === actual || expected === 'core.Any' || actual === 'core.Any') return true;
  return (
    (expected === 'value.Float' && actual === 'value.Integer') ||
    (expected === 'value.Integer' && actual === 'value.Float') ||
    (expected === 'value.Condition' && actual === 'value.Boolean') ||
    (expected === 'value.Boolean' && actual === 'value.Condition')
  );
}

/// Resolve the expected data type for a connection target, accepting either a
/// static input port or an exposed parameter of the same id.
function inputType(node: PlatformNode | undefined, descriptor: NodeDescriptor, portId: string): string {
  const input = descriptor.inputs.find((candidate) => candidate.id === portId);
  if (input) return input.dataType;
  if (node?.exposedParameters?.includes(portId)) {
    const parameter = descriptor.parameters.find((candidate) => candidate.id === portId);
    if (parameter) return parameterDataType(parameter.parameterType);
  }
  throw new Error(`port '${portId}' does not exist on node '${descriptor.typeId}'`);
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

function stableStringify(value: unknown): string {
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(',')}]`;
  const entries = Object.entries(value as Record<string, unknown>)
    .filter(([, item]) => item !== undefined)
    .sort(([left], [right]) => left.localeCompare(right));
  return `{${entries.map(([key, item]) => `${JSON.stringify(key)}:${stableStringify(item)}`).join(',')}}`;
}

async function stableHash(value: unknown): Promise<string> {
  const serialized = stableStringify(value);
  const bytes = new TextEncoder().encode(serialized);
  if (globalThis.crypto?.subtle) {
    const digest = await globalThis.crypto.subtle.digest('SHA-256', bytes);
    return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
  }
  let hash = 0xcbf29ce484222325n;
  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = BigInt.asUintN(64, hash * 0x100000001b3n);
  }
  return hash.toString(16).padStart(16, '0');
}

function emptyDependencyReport(): DependencyReport {
  return { available: [], missing: [], mismatched: [], disabledNodes: [], statuses: {} };
}

function summary(definition: WorkflowDefinition): WorkflowSummary {
  return {
    id: definition.identity.id,
    name: definition.metadata.name,
    version: definition.identity.version,
    hash: definition.hash,
  };
}

function portFromBoundary(
  direction: 'Input' | 'Output',
  node: PlatformNode,
  portId: string,
  descriptor: NodeDescriptor,
): WorkflowPort {
  const portDescriptor = direction === 'Input'
    ? descriptor.inputs.find((candidate) => candidate.id === portId)
    : descriptor.outputs.find((candidate) => candidate.id === portId);
  if (!portDescriptor) throw new Error(`port '${portId}' does not exist on node '${node.id}'`);
  return {
    id: `${direction === 'Input' ? 'input' : 'output'}:${node.id}:${portId}`,
    name: portDescriptor.name,
    direction,
    nodeId: node.id,
    portId,
    dataType: portDescriptor.dataType,
    required: portDescriptor.required,
  };
}

function workflowDocument(definition: WorkflowDefinition): Omit<WorkflowDefinition, 'hash'> {
  const { hash: _hash, ...document } = definition;
  document.graph = { nodes: document.graph.nodes, edges: document.graph.edges };
  return document;
}

async function definitionHash(definition: WorkflowDefinition): Promise<string> {
  return stableHash(workflowDocument(definition));
}

function newDefinition(
  identity: { id: string; version: string },
  metadata: WorkflowMetadata,
  graph: PlatformSnapshot,
  dependencies: Partial<Pick<WorkflowDefinition, 'nodePackDependencies' | 'subgraphDependencies'>> = {},
): WorkflowDefinition {
  return {
    identity,
    graph,
    parameters: [],
    inputs: [],
    outputs: [],
    subgraphDependencies: clone(dependencies.subgraphDependencies ?? []),
    nodePackDependencies: clone(dependencies.nodePackDependencies ?? []),
    metadata,
    nestedSubgraphs: {},
    hash: '',
  };
}

function validateGraph(descriptors: NodeDescriptor[], parsed: PlatformSnapshot): void {
  if (!Array.isArray(parsed.nodes) || !Array.isArray(parsed.edges)) throw new Error('invalid workflow document');
  const nodes = new Map(parsed.nodes.map((node) => [node.id, node]));
  for (const node of parsed.nodes) descriptorFor(descriptors, node.typeId);
  for (const edge of parsed.edges) {
    const source = nodes.get(edge.fromNode);
    const target = nodes.get(edge.toNode);
    if (!source || !target) throw new Error('workflow contains an unknown node');
    const sourcePort = port(descriptorFor(descriptors, source.typeId), 'output', edge.fromPort);
    const targetType = inputType(target, descriptorFor(descriptors, target.typeId), edge.toPort);
    if (!typesCompatible(targetType, sourcePort.dataType)) throw new Error('workflow contains an invalid connection');
  }
  if (hasCycle(parsed.edges)) throw new Error('workflow contains a cycle');
}

export function createMemoryPlatform(): EditorPlatform {
  const descriptors = clone(builtInDescriptors);
  const nodes = new Map<string, PlatformNode>();
  let edges: PlatformEdge[] = [];
  let revision = 0;
  const rootDefinition = newDefinition(
    { id: 'workflow', version: '1.0.0' },
    { name: 'Workflow' },
    { nodes: [], edges: [], revision: 0 },
  );
  const scopeStack: WorkflowDefinition[] = [rootDefinition];

  const currentDefinition = (): WorkflowDefinition => scopeStack.at(-1)!;
  const syncDefinition = (): void => {
    const definition = currentDefinition();
    definition.graph = { nodes: clone([...nodes.values()]), edges: clone(edges), revision };
    definition.parameters = [...nodes.values()].flatMap((node) => {
      const descriptor = descriptorFor(descriptors, node.typeId);
      return (node.exposedParameters ?? []).flatMap((parameterId): WorkflowParameter[] => {
        const parameter = descriptor.parameters.find((candidate) => candidate.id === parameterId);
        if (!parameter) return [];
        return [{
          id: `${node.id}:${parameterId}`,
          name: parameter.name,
          nodeId: node.id,
          parameterId,
          parameterType: parameter.parameterType,
          default: node.parameters[parameterId] ?? parameter.default,
        }];
      });
    });
    definition.hash = '';
  };
  const scopePath = (): ScopeBreadcrumb[] =>
    scopeStack.map((definition, index) => ({
      id: index === 0 ? 'root' : definition.identity.id,
      name: definition.metadata.name,
      version: definition.identity.version,
      hash: definition.hash,
    }));
  const workflowPorts = (): { inputs: WorkflowPort[]; outputs: WorkflowPort[] } => {
    const definition = currentDefinition();
    return { inputs: clone(definition.inputs), outputs: clone(definition.outputs) };
  };
  const dependencyReport = (): DependencyReport => {
    const definition = currentDefinition();
    const report = emptyDependencyReport();
    for (const dependency of definition.nodePackDependencies) {
      const diagnostic: DependencyDiagnostic = {
        id: dependency.id,
        requiredVersion: dependency.version,
        availableVersion: null,
      };
      report.missing.push(diagnostic);
      report.statuses![dependency.id] = { kind: 'missing' };
    }
    for (const dependency of definition.subgraphDependencies) {
      const diagnostic: DependencyDiagnostic = {
        id: dependency.id,
        requiredVersion: dependency.version,
        availableVersion: null,
      };
      report.missing.push(diagnostic);
      report.statuses![dependency.id] = { kind: 'missing' };
    }
    return report;
  };
  const snapshot = (): PlatformSnapshot => {
    syncDefinition();
    const ports = workflowPorts();
    return {
      nodes: clone([...nodes.values()]),
      edges: clone(edges),
      revision,
      scopePath: scopePath(),
      workflowInputs: ports.inputs,
      workflowOutputs: ports.outputs,
      workflowParameters: clone(currentDefinition().parameters),
      nestedSubgraphs: Object.values(currentDefinition().nestedSubgraphs).map(summary),
      dependencyReport: dependencyReport(),
    };
  };

  const loadDefinitionGraph = (definition: WorkflowDefinition): void => {
    validateGraph(descriptors, definition.graph);
    nodes.clear();
    for (const node of definition.graph.nodes) nodes.set(node.id, clone(node));
    edges = clone(definition.graph.edges);
    revision = definition.graph.revision ?? revision + 1;
  };

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
      scopeStack.splice(1);
      rootDefinition.nestedSubgraphs = {};
      rootDefinition.inputs = [];
      rootDefinition.outputs = [];
      rootDefinition.parameters = [];
      rootDefinition.nodePackDependencies = [];
      rootDefinition.subgraphDependencies = [];
      nodes.clear();
      edges = [];
      revision = 0;
      const inputDescriptor = descriptorFor(descriptors, 'core.image-input');
      const outputDescriptor = descriptorFor(descriptors, 'core.output');
      nodes.set('input', {
        id: 'input',
        typeId: inputDescriptor.typeId,
        parameters: {},
        exposedParameters: [],
      });
      revision += 1;
      nodes.set('output', {
        id: 'output',
        typeId: outputDescriptor.typeId,
        parameters: {},
        exposedParameters: [],
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
        exposedParameters: [],
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
      const targetType = inputType(target, descriptorFor(descriptors, target.typeId), toPort);
      if (!typesCompatible(targetType, sourcePort.dataType)) {
        throw new Error(`cannot connect '${sourcePort.dataType}' to '${targetType}'`);
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
    async exposeParameter(nodeId, parameterId) {
      const node = nodes.get(nodeId);
      if (!node) throw new Error(`node '${nodeId}' does not exist`);
      const parameter = descriptorFor(descriptors, node.typeId).parameters.find(
        (candidate) => candidate.id === parameterId,
      );
      if (!parameter) throw new Error(`parameter '${parameterId}' does not exist on node '${nodeId}'`);
      const exposed = new Set(node.exposedParameters ?? []);
      exposed.add(parameterId);
      node.exposedParameters = [...exposed];
      revision += 1;
    },
    async unexposeParameter(nodeId, parameterId) {
      const node = nodes.get(nodeId);
      if (!node) throw new Error(`node '${nodeId}' does not exist`);
      const parameter = descriptorFor(descriptors, node.typeId).parameters.find(
        (candidate) => candidate.id === parameterId,
      );
      if (!parameter) throw new Error(`parameter '${parameterId}' does not exist on node '${nodeId}'`);
      node.exposedParameters = (node.exposedParameters ?? []).filter((id) => id !== parameterId);
      edges = edges.filter((edge) => !(edge.toNode === nodeId && edge.toPort === parameterId));
      currentDefinition().parameters = currentDefinition().parameters.filter((item) => item.id !== `${nodeId}:${parameterId}`);
      revision += 1;
    },
    async exposeInput(nodeId, portId) {
      const node = nodes.get(nodeId);
      if (!node) throw new Error(`node '${nodeId}' does not exist`);
      const definition = currentDefinition();
      const exposed = portFromBoundary('Input', node, portId, descriptorFor(descriptors, node.typeId));
      if (definition.inputs.some((port) => port.id === exposed.id)) throw new Error(`workflow port '${exposed.id}' already exists`);
      definition.inputs.push(exposed);
      revision += 1;
    },
    async exposeOutput(nodeId, portId) {
      const node = nodes.get(nodeId);
      if (!node) throw new Error(`node '${nodeId}' does not exist`);
      const definition = currentDefinition();
      const exposed = portFromBoundary('Output', node, portId, descriptorFor(descriptors, node.typeId));
      if (definition.outputs.some((port) => port.id === exposed.id)) throw new Error(`workflow port '${exposed.id}' already exists`);
      definition.outputs.push(exposed);
      revision += 1;
    },
    async hidePort(portId) {
      const definition = currentDefinition();
      const before = definition.inputs.length + definition.outputs.length;
      definition.inputs = definition.inputs.filter((port) => port.id !== portId);
      definition.outputs = definition.outputs.filter((port) => port.id !== portId);
      if (before === definition.inputs.length + definition.outputs.length) throw new Error(`workflow port '${portId}' does not exist`);
      revision += 1;
    },
    async createSubgraph(selection: string[], options: CreateSubgraphOptions) {
      if (selection.length === 0) throw new Error('workflow selection cannot be empty');
      if (!options.id.trim() || !options.version.trim() || !options.metadata.name.trim()) {
        throw new Error('subgraph identity and name cannot be empty');
      }
      syncDefinition();
      const selected = new Set(selection);
      const selectedNodes = selection.map((nodeId) => {
        const node = nodes.get(nodeId);
        if (!node) throw new Error(`node '${nodeId}' does not exist`);
        return clone(node);
      });
      const selectedEdges = edges.filter((edge) => selected.has(edge.fromNode) && selected.has(edge.toNode));
      const child = newDefinition(
        { id: options.id, version: options.version },
        clone(options.metadata),
        { nodes: selectedNodes, edges: selectedEdges, revision: 0 },
        {
          nodePackDependencies: options.nodePackDependencies,
          subgraphDependencies: options.subgraphDependencies,
        },
      );
      for (const edge of edges) {
        if (selected.has(edge.toNode) && !selected.has(edge.fromNode)) {
          const node = nodes.get(edge.toNode)!;
          const exposed = portFromBoundary('Input', node, edge.toPort, descriptorFor(descriptors, node.typeId));
          if (!child.inputs.some((port) => port.id === exposed.id)) child.inputs.push(exposed);
        }
        if (selected.has(edge.fromNode) && !selected.has(edge.toNode)) {
          const node = nodes.get(edge.fromNode)!;
          const exposed = portFromBoundary('Output', node, edge.fromPort, descriptorFor(descriptors, node.typeId));
          if (!child.outputs.some((port) => port.id === exposed.id)) child.outputs.push(exposed);
        }
      }
      child.parameters = selectedNodes.flatMap((node) => {
        const descriptor = descriptorFor(descriptors, node.typeId);
        return (node.exposedParameters ?? []).flatMap((parameterId): WorkflowParameter[] => {
          const parameter = descriptor.parameters.find((candidate) => candidate.id === parameterId);
          return parameter
            ? [{
                id: `${node.id}:${parameterId}`,
                name: parameter.name,
                nodeId: node.id,
                parameterId,
                parameterType: parameter.parameterType,
                default: node.parameters[parameterId] ?? parameter.default,
              }]
            : [];
        });
      });
      child.hash = await definitionHash(child);
      if (currentDefinition().nestedSubgraphs[child.identity.id]) {
        throw new Error(`workflow dependency '${child.identity.id}' already exists`);
      }
      currentDefinition().nestedSubgraphs[child.identity.id] = child;
      currentDefinition().subgraphDependencies.push({
        id: child.identity.id,
        version: child.identity.version,
        hash: child.hash,
      });
      revision += 1;
      return clone(child);
    },
    async openSubgraph(id: string) {
      syncDefinition();
      const definition = currentDefinition().nestedSubgraphs[id];
      if (!definition) throw new Error(`subgraph '${id}' does not exist`);
      scopeStack.push(definition);
      loadDefinitionGraph(definition);
    },
    async returnToParent() {
      if (scopeStack.length <= 1) throw new Error('already at the root workflow scope');
      syncDefinition();
      currentDefinition().hash = await definitionHash(currentDefinition());
      scopeStack.pop();
      loadDefinitionGraph(currentDefinition());
    },
    async saveBlueprint() {
      syncDefinition();
      currentDefinition().hash = await definitionHash(currentDefinition());
      return JSON.stringify(workflowDocument(currentDefinition()), null, 2);
    },
    async exportBlueprint() {
      return this.saveBlueprint();
    },
    async loadBlueprint(serialized) {
      const parsed = JSON.parse(serialized) as WorkflowDefinition;
      if (!parsed?.identity?.id || !parsed.identity.version || !parsed.metadata?.name) {
        throw new Error('invalid blueprint document');
      }
      validateGraph(descriptors, parsed.graph);
      const definition = clone(parsed);
      definition.parameters ??= [];
      definition.inputs ??= [];
      definition.outputs ??= [];
      definition.subgraphDependencies ??= [];
      definition.nodePackDependencies ??= [];
      definition.nestedSubgraphs ??= {};
      definition.hash = await definitionHash(definition);
      return definition;
    },
    async importBlueprint(serialized) {
      return this.loadBlueprint(serialized);
    },
    async instantiateBlueprint(serialized) {
      const definition = await this.loadBlueprint(serialized);
      currentDefinition().graph = clone(definition.graph);
      currentDefinition().parameters = clone(definition.parameters);
      currentDefinition().inputs = clone(definition.inputs);
      currentDefinition().outputs = clone(definition.outputs);
      currentDefinition().subgraphDependencies = clone(definition.subgraphDependencies);
      currentDefinition().nodePackDependencies = clone(definition.nodePackDependencies);
      currentDefinition().metadata = clone(definition.metadata);
      currentDefinition().nestedSubgraphs = clone(definition.nestedSubgraphs);
      loadDefinitionGraph(currentDefinition());
      revision += 1;
    },
    async dependencyStatus() {
      syncDefinition();
      return dependencyReport();
    },
    async workflowHash() {
      syncDefinition();
      currentDefinition().hash = await definitionHash(currentDefinition());
      return currentDefinition().hash;
    },
    async saveWorkflow() {
      return JSON.stringify(snapshot());
    },
    async loadWorkflow(serialized) {
      const parsed = JSON.parse(serialized) as PlatformSnapshot;
      validateGraph(descriptors, parsed);
      scopeStack.splice(1);
      rootDefinition.nestedSubgraphs = {};
      rootDefinition.inputs = [];
      rootDefinition.outputs = [];
      rootDefinition.parameters = [];
      rootDefinition.nodePackDependencies = [];
      rootDefinition.subgraphDependencies = [];
      rootDefinition.graph = clone(parsed);
      nodes.clear();
      for (const node of parsed.nodes) nodes.set(node.id, { ...clone(node), exposedParameters: clone(node.exposedParameters ?? []) });
      edges = clone(parsed.edges);
      revision += 1;
      syncDefinition();
    },
  };
}

export function createPlatform(): EditorPlatform {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    return createTauriPlatform();
  }
  return createMemoryPlatform();
}
