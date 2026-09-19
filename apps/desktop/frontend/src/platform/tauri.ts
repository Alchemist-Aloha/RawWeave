import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type {
  EditorPlatform,
  ExecutionCapability,
  NodeDescriptor,
  OpenImageResult,
  ParameterType,
  ParameterValue,
  PlatformSnapshot,
} from '../editor/types';

interface RustPort {
  id: string;
  name: string;
  data_type: string;
  required: boolean;
}

interface RustParameter {
  id: string;
  name: string;
  parameter_type: ParameterType;
  default: RustValue;
  min: number | null;
  max: number | null;
}

interface RustDescriptor {
  type_id: string;
  name: string;
  version: number;
  inputs: RustPort[];
  outputs: RustPort[];
  parameters: RustParameter[];
  capabilities?: string[];
}

interface RustOpenImageResult {
  kind: 'ordinary' | 'raw';
  width: number;
  height: number;
  revision: number;
  metadata: OpenImageResult['metadata'];
}

const RAW_EXTENSIONS = [
  '3fr',
  'arw',
  'cr2',
  'cr3',
  'dcr',
  'dng',
  'erf',
  'kdc',
  'mrw',
  'nef',
  'nrw',
  'orf',
  'pef',
  'raf',
  'raw',
  'rw2',
  'rwl',
  'srw',
  'x3f',
];

const IMAGE_DIALOG_FILTERS = [
  {
    name: 'Images and RAW',
    extensions: [...RAW_EXTENSIONS, 'png', 'jpg', 'jpeg'],
  },
];

type RustValue = { Float: number } | { Integer: number } | { Boolean: boolean } | { String: string };

function fromRustValue(value: RustValue): ParameterValue {
  if ('Float' in value) return value.Float;
  if ('Integer' in value) return value.Integer;
  if ('Boolean' in value) return value.Boolean;
  return value.String;
}

function toRustValue(value: ParameterValue, parameterType?: ParameterType): RustValue {
  if (typeof value === 'number') {
    return parameterType === 'Integer' ? { Integer: Math.trunc(value) } : { Float: value };
  }
  if (typeof value === 'boolean') return { Boolean: value };
  return { String: value };
}

function mapOpenImageResult(result: RustOpenImageResult): OpenImageResult {
  return {
    kind: result.kind,
    width: result.width,
    height: result.height,
    revision: result.revision,
    metadata: result.metadata,
  };
}

function mapDescriptor(descriptor: RustDescriptor): NodeDescriptor {
  return {
    typeId: descriptor.type_id,
    name: descriptor.name,
    version: descriptor.version,
    inputs: descriptor.inputs.map((port) => ({
      id: port.id,
      name: port.name,
      dataType: port.data_type,
      required: port.required,
    })),
    outputs: descriptor.outputs.map((port) => ({
      id: port.id,
      name: port.name,
      dataType: port.data_type,
      required: port.required,
    })),
    parameters: descriptor.parameters.map((parameter) => ({
      id: parameter.id,
      name: parameter.name,
      parameterType: parameter.parameter_type,
      default: fromRustValue(parameter.default),
      min: parameter.min,
      max: parameter.max,
    })),
    capabilities: descriptor.capabilities?.map(mapCapability),
  };
}

function mapCapability(capability: string): ExecutionCapability {
  switch (capability) {
    case 'Gpu':
      return 'GPU';
    case 'TileLocal':
      return 'TileLocal';
    case 'RegionAware':
      return 'RegionAware';
    case 'FullFrame':
      return 'FullFrame';
    default:
      return 'CPU';
  }
}

function message(error: unknown): Error {
  if (error instanceof Error) return error;
  if (typeof error === 'string') return new Error(error);
  return new Error(JSON.stringify(error));
}

const nodeTypes = new Map<string, string>();

async function readSnapshot(): Promise<PlatformSnapshot> {
  const serialized = await invoke<string>('save_workflow');
  const graph = JSON.parse(serialized) as {
    nodes: Record<
      string,
      { type_id: string; parameters: Record<string, RustValue>; exposed_parameters?: string[] }
    >;
    edges: Array<{ from_node: string; from_port: string; to_node: string; to_port: string }>;
    revision?: number;
  };
  const nodes = Object.entries(graph.nodes ?? {}).map(([id, node]) => ({
    id,
    typeId: node.type_id,
    parameters: Object.fromEntries(
      Object.entries(node.parameters).map(([key, value]) => [key, fromRustValue(value)]),
    ),
    exposedParameters: node.exposed_parameters ?? [],
  }));
  for (const node of nodes) nodeTypes.set(node.id, node.typeId);
  return {
    nodes,
    edges: (graph.edges ?? []).map((edge) => ({
      fromNode: edge.from_node,
      fromPort: edge.from_port,
      toNode: edge.to_node,
      toPort: edge.to_port,
    })),
    revision: typeof graph.revision === 'number' ? graph.revision : undefined,
  };
}

export function createTauriPlatform(): EditorPlatform {
  let cachedDescriptors: NodeDescriptor[] = [];
  return {
    async nodeDescriptors() {
      try {
        const descriptors = await invoke<RustDescriptor[]>('node_descriptors');
        cachedDescriptors = descriptors.map(mapDescriptor);
        return cachedDescriptors;
      } catch (error) {
        throw message(error);
      }
    },
    snapshot: readSnapshot,
    async chooseImagePath() {
      try {
        const selected = await open({
          directory: false,
          multiple: false,
          filters: IMAGE_DIALOG_FILTERS,
        });
        if (Array.isArray(selected)) return selected[0] ?? null;
        return selected;
      } catch (error) {
        throw message(error);
      }
    },
    async openImage(path) {
      try {
        const result = await invoke<RustOpenImageResult>('open_image', { path });
        return mapOpenImageResult(result);
      } catch (error) {
        throw message(error);
      }
    },
    async addNode(nodeId, typeId) {
      try {
        await invoke('add_node', { nodeId, typeId });
      } catch (error) {
        throw message(error);
      }
    },
    async removeNode(nodeId) {
      try {
        await invoke('remove_node', { nodeId });
      } catch (error) {
        throw message(error);
      }
    },
    async connect(fromNode, fromPort, toNode, toPort) {
      try {
        await invoke('connect_nodes', { fromNode, fromPort, toNode, toPort });
      } catch (error) {
        throw message(error);
      }
    },
    async disconnect(fromNode, fromPort, toNode, toPort) {
      try {
        await invoke('disconnect_nodes', { fromNode, fromPort, toNode, toPort });
      } catch (error) {
        throw message(error);
      }
    },
    async setParameter(nodeId, parameterId, value) {
      try {
        const typeId = nodeTypes.get(nodeId);
        const parameterType = cachedDescriptors
          .find((descriptor) => descriptor.typeId === typeId)
          ?.parameters.find((parameter) => parameter.id === parameterId)?.parameterType;
        await invoke('set_node_parameter', {
          nodeId,
          parameterId,
          value: toRustValue(value, parameterType),
        });
      } catch (error) {
        throw message(error);
      }
    },
    async exposeParameter(nodeId, parameterId) {
      try {
        await invoke('expose_parameter', { nodeId, parameterId });
      } catch (error) {
        throw message(error);
      }
    },
    async unexposeParameter(nodeId, parameterId) {
      try {
        await invoke('unexpose_parameter', { nodeId, parameterId });
      } catch (error) {
        throw message(error);
      }
    },
    async saveWorkflow() {
      try {
        return await invoke<string>('save_workflow');
      } catch (error) {
        throw message(error);
      }
    },
    async loadWorkflow(serialized) {
      try {
        await invoke('load_workflow', { workflow: serialized });
      } catch (error) {
        throw message(error);
      }
    },
  };
}
