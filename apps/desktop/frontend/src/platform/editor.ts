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
import { isTauriRuntime } from './runtime';

import { aiNodeDescriptors } from './ai-nodes';

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

// Mirror Rust descriptors for browser-only UI regression coverage, not processing.
const colorQualifier = imageProcessingDescriptor('core.mask-color-qualifier', 'Color Qualifier');
colorQualifier.outputs = [{ id: 'mask', name: 'Mask', dataType: 'core.Mask', required: false }];
colorQualifier.capabilities = tileCapabilities;
colorQualifier.parameters = [
  parameter('target_r', 'Target Red', 1, 0, 1),
  parameter('target_g', 'Target Green', 1, 0, 1),
  parameter('target_b', 'Target Blue', 1, 0, 1),
  parameter('tolerance', 'Tolerance', 0.1, 0, 2),
  parameter('softness', 'Softness', 0, 0, 2),
];
const colorZones = imageProcessingDescriptor('pro.color-zones', 'Color Zones');
colorZones.parameters = [
  parameter('hue', 'Hue', 0, 0, 1),
  parameter('width', 'Width', 0.2, 0.001, 0.5),
  parameter('saturation', 'Saturation', 0, -4, 4),
  parameter('lightness', 'Lightness', 0, -4, 4),
];
const splitToning = imageProcessingDescriptor('pro.split-toning', 'Split Toning');
splitToning.parameters = [
  parameter('shadow_hue', 'Shadow Hue', 0.6, 0, 1),
  parameter('highlight_hue', 'Highlight Hue', 0.1, 0, 1),
  parameter('shadow_saturation', 'Shadow Saturation', 0, 0, 1),
  parameter('highlight_saturation', 'Highlight Saturation', 0, 0, 1),
  parameter('balance', 'Balance', 0.5, 0, 1),
];

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
  lazyInputs: NodeDescriptor['lazyInputs'] = [],
): NodeDescriptor {
  return { typeId, name, version: 1, inputs, outputs, parameters, capabilities: ['CPU'], lazyInputs };
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
    [],
    [{
      selector: 'condition',
      required: ['condition'],
      branches: [
        { condition: 'True', inputs: ['true'] },
        { condition: 'False', inputs: ['false'] },
      ],
    }],
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
    [],
    [{
      selector: 'index',
      required: ['index'],
      branches: [
        { condition: { Index: 0 }, inputs: ['a'] },
        { condition: { Index: 1 }, inputs: ['b'] },
        { condition: { Index: 2 }, inputs: ['c'] },
        { condition: { Index: 3 }, inputs: ['d'] },
      ],
    }],
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

function imageSetInputDescriptor(): NodeDescriptor {
  return {
    typeId: 'core.imageset-input',
    name: 'ImageSet Input',
    version: 1,
    inputs: [],
    outputs: [outputPort('images', 'Image Set', 'core.ImageSet'), outputPort('set', 'Image Set (alias)', 'core.ImageSet')],
    parameters: [],
    capabilities: fullFrameCapabilities,
  };
}

function imageSetCollectionDescriptor(typeId: string, name: string): NodeDescriptor {
  return {
    typeId,
    name,
    version: 1,
    inputs: [input('images', 'Image Set', 'core.ImageSet', true), input('set', 'Image Set (alias)', 'core.ImageSet', false)],
    outputs: [outputPort('images', 'Image Set', 'core.ImageSet'), outputPort('set', 'Image Set (alias)', 'core.ImageSet')],
    parameters: [],
    capabilities: fullFrameCapabilities,
  };
}

function imageSetResultDescriptor(typeId: string, name: string): NodeDescriptor {
  return {
    typeId,
    name,
    version: 1,
    inputs: [input('images', 'Image Set', 'core.ImageSet', true), input('set', 'Image Set (alias)', 'core.ImageSet', false)],
    outputs: [outputPort('image', 'Image', 'core.Image'), outputPort('member_id', 'Member ID', 'value.String')],
    parameters: [integerParameter('index', 'Member Index', 0), stringParameter('member_id', 'Member ID', '')],
    capabilities: fullFrameCapabilities,
  };
}

const imageSetDescriptors: NodeDescriptor[] = [
  imageSetInputDescriptor(),
  { ...imageSetCollectionDescriptor('core.imageset-collect', 'Collect Images'), inputs: [input('image_0', 'Image 0', 'core.Image', false), input('image_1', 'Image 1', 'core.Image', false)], parameters: [stringParameter('id_prefix', 'Member ID Prefix', 'member'), booleanParameter('ordered', 'Preserve Input Order', true)] },
  imageSetCollectionDescriptor('core.alignment', 'Alignment'),
  { ...imageSetCollectionDescriptor('core.exposure-set', 'Exposure Set'), parameters: [booleanParameter('sort_by_exposure', 'Sort by Exposure', true)] },
  imageSetResultDescriptor('core.hdr-merge', 'HDR Merge'),
  imageSetResultDescriptor('core.focus-stack', 'Focus Stack'),
  imageSetResultDescriptor('core.panorama', 'Panorama'),
  imageSetResultDescriptor('core.panorama-stitch', 'Panorama Stitch'),
  imageSetResultDescriptor('core.imageset-select', 'Select'),
  { ...imageSetCollectionDescriptor('core.imageset-filter', 'Filter'), parameters: [stringParameter('tag', 'Tag', ''), stringParameter('value', 'Value', '')] },
  { ...imageSetCollectionDescriptor('core.imageset-map', 'Map'), parameters: [parameter('exposure', 'Exposure', 0)] },
  { ...imageSetCollectionDescriptor('core.imageset-group', 'Group'), parameters: [stringParameter('tag', 'Tag', ''), stringParameter('value', 'Value', '')], outputs: [outputPort('images', 'Image Set', 'core.ImageSet'), outputPort('set', 'Image Set (alias)', 'core.ImageSet'), outputPort('group', 'Group', 'value.String')] },
];

const localExposure = imageProcessingDescriptor('core.local-exposure', 'Local Exposure');
localExposure.inputs.push(input('mask', 'Mask', 'core.Mask', false), input('exposure', 'Exposure', 'value.Float', false));
localExposure.parameters.push(parameter('exposure', 'Exposure', 0));
localExposure.capabilities = tileCapabilities;

// Keep existing image sockets and identifiers; scene sockets remain explicitly typed.
for (const descriptor of [exposure, localExposure, blur, resize, colorMatrix, output]) {
  descriptor.inputs.find((port) => port.id === 'image')!.required = false;
  descriptor.inputs.push(input('scene', 'Scene Linear RGB', 'color.SceneLinearRGB', false));
  descriptor.outputs.push(outputPort('scene', 'Scene Linear RGB', 'color.SceneLinearRGB'));
}
const sceneToImage: NodeDescriptor = {
  typeId: 'core.scene-linear-to-image', name: 'Scene Linear RGB to Image', version: 1,
  inputs: [input('scene', 'Scene Linear RGB', 'color.SceneLinearRGB', true)],
  outputs: [outputPort('image', 'Linear sRGB Image', 'core.Image')],
  parameters: [], capabilities: ['CPU', 'FullFrame', 'MipInvariant'],
};

export const builtInDescriptors: NodeDescriptor[] = [
  imageInput,
  constantFloat,
  exposure,
  localExposure,
  sceneToImage,
  invert,
  resize,
  crop,
  blur,
  levels,
  curves,
  colorQualifier,
  colorZones,
  splitToning,
  colorMatrix,
  output,
  ...logicDescriptors,
  ...imageSetDescriptors,
  ...rawDescriptors,
  ...aiNodeDescriptors,
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

function rotateRight(value: number, amount: number): number {
  return (value >>> amount) | (value << (32 - amount));
}

function sha256Fallback(bytes: Uint8Array): string {
  const constants = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
  ];
  const paddedLength = Math.ceil((bytes.length + 9) / 64) * 64;
  const padded = new Uint8Array(paddedLength);
  padded.set(bytes);
  padded[bytes.length] = 0x80;
  const bitLength = bytes.length * 8;
  const view = new DataView(padded.buffer);
  view.setUint32(paddedLength - 4, bitLength >>> 0);
  view.setUint32(paddedLength - 8, Math.floor(bitLength / 0x100000000));

  let h0 = 0x6a09e667;
  let h1 = 0xbb67ae85;
  let h2 = 0x3c6ef372;
  let h3 = 0xa54ff53a;
  let h4 = 0x510e527f;
  let h5 = 0x9b05688c;
  let h6 = 0x1f83d9ab;
  let h7 = 0x5be0cd19;
  const words = new Uint32Array(64);

  for (let offset = 0; offset < padded.length; offset += 64) {
    for (let index = 0; index < 16; index += 1) words[index] = view.getUint32(offset + index * 4);
    for (let index = 16; index < 64; index += 1) {
      const s0 = rotateRight(words[index - 15], 7) ^ rotateRight(words[index - 15], 18) ^ (words[index - 15] >>> 3);
      const s1 = rotateRight(words[index - 2], 17) ^ rotateRight(words[index - 2], 19) ^ (words[index - 2] >>> 10);
      words[index] = (words[index - 16] + s0 + words[index - 7] + s1) >>> 0;
    }
    let a = h0;
    let b = h1;
    let c = h2;
    let d = h3;
    let e = h4;
    let f = h5;
    let g = h6;
    let h = h7;
    for (let index = 0; index < 64; index += 1) {
      const s1 = rotateRight(e, 6) ^ rotateRight(e, 11) ^ rotateRight(e, 25);
      const choice = (e & f) ^ (~e & g);
      const temp1 = (h + s1 + choice + constants[index] + words[index]) >>> 0;
      const s0 = rotateRight(a, 2) ^ rotateRight(a, 13) ^ rotateRight(a, 22);
      const majority = (a & b) ^ (a & c) ^ (b & c);
      const temp2 = (s0 + majority) >>> 0;
      h = g;
      g = f;
      f = e;
      e = (d + temp1) >>> 0;
      d = c;
      c = b;
      b = a;
      a = (temp1 + temp2) >>> 0;
    }
    h0 = (h0 + a) >>> 0;
    h1 = (h1 + b) >>> 0;
    h2 = (h2 + c) >>> 0;
    h3 = (h3 + d) >>> 0;
    h4 = (h4 + e) >>> 0;
    h5 = (h5 + f) >>> 0;
    h6 = (h6 + g) >>> 0;
    h7 = (h7 + h) >>> 0;
  }

  return [h0, h1, h2, h3, h4, h5, h6, h7]
    .map((word) => word.toString(16).padStart(8, '0'))
    .join('');
}

async function stableHash(serialized: string): Promise<string> {
  const bytes = new TextEncoder().encode(serialized);
  if (globalThis.crypto?.subtle) {
    const digest = await globalThis.crypto.subtle.digest('SHA-256', bytes);
    return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
  }
  return sha256Fallback(bytes);
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

function rustParameterValue(value: ParameterValue, parameterType: ParameterType): Record<string, unknown> {
  if (parameterType === 'Float') return { Float: typeof value === 'number' ? Math.fround(value) : value };
  if (parameterType === 'Integer') return { Integer: typeof value === 'number' ? Math.trunc(value) : value };
  if (parameterType === 'Boolean') return { Boolean: Boolean(value) };
  return { String: String(value) };
}

function rustCapability(capability: ExecutionCapability): string {
  switch (capability) {
    case 'GPU': return 'Gpu';
    case 'TileLocal': return 'TileLocal';
    case 'RegionAware': return 'RegionAware';
    case 'FullFrame': return 'FullFrame';
    default: return 'Cpu';
  }
}

function rustPortDescriptor(port: PortDescriptor): Record<string, unknown> {
  return {
    id: port.id,
    name: port.name,
    data_type: port.dataType,
    required: port.required,
  };
}

function rustParameterDescriptor(parameter: NodeDescriptor['parameters'][number]): Record<string, unknown> {
  return {
    id: parameter.id,
    name: parameter.name,
    parameter_type: parameter.parameterType,
    default: rustParameterValue(parameter.default, parameter.parameterType),
    min: parameter.min,
    max: parameter.max,
  };
}

function rustNodeDescriptor(descriptor: NodeDescriptor): Record<string, unknown> {
  return {
    type_id: descriptor.typeId,
    name: descriptor.name,
    version: descriptor.version,
    inputs: descriptor.inputs.map(rustPortDescriptor),
    outputs: descriptor.outputs.map(rustPortDescriptor),
    parameters: descriptor.parameters.map(rustParameterDescriptor),
    capabilities: (descriptor.capabilities ?? []).map(rustCapability),
    ...(descriptor.lazyInputs && descriptor.lazyInputs.length > 0
      ? { lazy_inputs: descriptor.lazyInputs }
      : {}),
  };
}

function stringCompare(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function rustGraphNode(node: PlatformNode, descriptors: NodeDescriptor[]): Record<string, unknown> {
  const descriptor = descriptorFor(descriptors, node.typeId);
  const parameters = Object.fromEntries(
    Object.entries(node.parameters)
      .sort(([left], [right]) => stringCompare(left, right))
      .map(([id, value]) => {
        const parameterType = descriptor.parameters.find((candidate) => candidate.id === id)?.parameterType ?? 'String';
        return [id, rustParameterValue(value, parameterType)];
      }),
  );
  return {
    id: node.id,
    type_id: node.typeId,
    descriptor: rustNodeDescriptor(descriptor),
    parameters,
    exposed_parameters: [...new Set(node.exposedParameters ?? [])].sort(stringCompare),
  };
}

function rustWorkflowPort(port: WorkflowPort): Record<string, unknown> {
  return {
    id: port.id,
    name: port.name,
    direction: port.direction,
    node_id: port.nodeId,
    port_id: port.portId,
    data_type: port.dataType,
    required: port.required,
  };
}

function rustWorkflowParameter(parameter: WorkflowParameter): Record<string, unknown> {
  return {
    id: parameter.id,
    name: parameter.name,
    node_id: parameter.nodeId,
    parameter_id: parameter.parameterId,
    parameter_type: parameter.parameterType,
    default: rustParameterValue(parameter.default, parameter.parameterType),
  };
}

function canonicalHashDocument(
  definition: WorkflowDefinition,
  descriptors: NodeDescriptor[],
  nestedHashes: Record<string, string>,
): Record<string, unknown> {
  const nodes = Object.fromEntries(
    definition.graph.nodes
      .slice()
      .sort((left, right) => stringCompare(left.id, right.id))
      .map((node) => [node.id, rustGraphNode(node, descriptors)]),
  );
  const parameters = Object.fromEntries(
    definition.parameters
      .slice()
      .sort((left, right) => stringCompare(left.id, right.id))
      .map((parameter) => [parameter.id, rustWorkflowParameter(parameter)]),
  );
  const sortPorts = (left: WorkflowPort, right: WorkflowPort) => stringCompare(left.id, right.id);
  const sortEdges = (left: PlatformEdge, right: PlatformEdge) =>
    stringCompare(
      `${left.fromNode}\u0000${left.fromPort}\u0000${left.toNode}\u0000${left.toPort}`,
      `${right.fromNode}\u0000${right.fromPort}\u0000${right.toNode}\u0000${right.toPort}`,
    );
  return {
    identity: { id: definition.identity.id, version: definition.identity.version },
    nodes,
    edges: definition.graph.edges
      .slice()
      .sort(sortEdges)
      .map((edge) => ({
        from_node: edge.fromNode,
        from_port: edge.fromPort,
        to_node: edge.toNode,
        to_port: edge.toPort,
      })),
    parameters,
    inputs: definition.inputs.slice().sort(sortPorts).map(rustWorkflowPort),
    outputs: definition.outputs.slice().sort(sortPorts).map(rustWorkflowPort),
    subgraph_dependencies: definition.subgraphDependencies
      .slice()
      .sort((left, right) => stringCompare(`${left.id}\u0000${left.version}\u0000${left.hash ?? ''}`, `${right.id}\u0000${right.version}\u0000${right.hash ?? ''}`))
      .map((dependency) => ({ id: dependency.id, version: dependency.version, hash: dependency.hash ?? '' })),
    node_pack_dependencies: definition.nodePackDependencies
      .slice()
      .sort((left, right) => stringCompare(`${left.id}\u0000${left.version}`, `${right.id}\u0000${right.version}`))
      .map((dependency) => ({ id: dependency.id, version: dependency.version })),
    nested_hashes: Object.fromEntries(
      Object.entries(nestedHashes).sort(([left], [right]) => stringCompare(left, right)),
    ),
  };
}

async function definitionHash(definition: WorkflowDefinition, descriptors: NodeDescriptor[]): Promise<string> {
  const nestedHashes = Object.fromEntries(
    await Promise.all(
      Object.entries(definition.nestedSubgraphs).map(async ([id, nested]) => [
        id,
        await definitionHash(nested, descriptors),
      ] as const),
    ),
  );
  return stableHash(JSON.stringify(canonicalHashDocument(definition, descriptors, nestedHashes)));
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
    scopeStack.map((definition) => ({
      id: definition.identity.id,
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
    async openImageSet(paths, order) {
      if (paths.length === 0) throw new Error('image set must contain at least one file');
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
      nodes.set('imageset-input', { id: 'imageset-input', typeId: 'core.imageset-input', parameters: {}, exposedParameters: [] });
      nodes.set('imageset-select', { id: 'imageset-select', typeId: 'core.imageset-select', parameters: { index: 0, member_id: '' }, exposedParameters: [] });
      edges = [{ fromNode: 'imageset-input', fromPort: 'images', toNode: 'imageset-select', toPort: 'images' }];
      revision = 3;
      return {
        kind: 'imageset' as const,
        order,
        revision,
        members: paths.map((path) => ({ id: path, path, name: path.split(/[\\\\/]/).at(-1) ?? path, width: 1, height: 1, metadata: null })),
        sharedMetadata: null,
        alignment: { state: 'unaligned' as const },
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
      child.hash = await definitionHash(child, descriptors);
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
      currentDefinition().hash = await definitionHash(currentDefinition(), descriptors);
      scopeStack.pop();
      loadDefinitionGraph(currentDefinition());
    },
    async saveBlueprint() {
      syncDefinition();
      currentDefinition().hash = await definitionHash(currentDefinition(), descriptors);
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
      definition.hash = await definitionHash(definition, descriptors);
      return definition;
    },
    async importBlueprint(serialized) {
      return this.loadBlueprint(serialized);
    },
    async instantiateBlueprint(serialized) {
      const definition = await this.loadBlueprint(serialized);
      currentDefinition().identity = clone(definition.identity);
      currentDefinition().graph = clone(definition.graph);
      currentDefinition().parameters = clone(definition.parameters);
      currentDefinition().inputs = clone(definition.inputs);
      currentDefinition().outputs = clone(definition.outputs);
      currentDefinition().subgraphDependencies = clone(definition.subgraphDependencies);
      currentDefinition().nodePackDependencies = clone(definition.nodePackDependencies);
      currentDefinition().metadata = clone(definition.metadata);
      currentDefinition().nestedSubgraphs = clone(definition.nestedSubgraphs);
      currentDefinition().hash = definition.hash;
      loadDefinitionGraph(currentDefinition());
      revision += 1;
    },
    async dependencyStatus() {
      syncDefinition();
      return dependencyReport();
    },
    async workflowHash() {
      syncDefinition();
      currentDefinition().hash = await definitionHash(currentDefinition(), descriptors);
      return currentDefinition().hash;
    },
    async saveWorkflow() {
      return JSON.stringify(snapshot());
    },
    async restoreWorkflowHistory(serialized) {
      await this.loadWorkflow(serialized);
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
  if (isTauriRuntime()) {
    return createTauriPlatform();
  }
  return createMemoryPlatform();
}
