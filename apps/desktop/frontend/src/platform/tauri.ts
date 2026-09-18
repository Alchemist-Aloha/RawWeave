import { invoke } from '@tauri-apps/api/core';
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

type RustValue = { Float: number } | { Boolean: boolean } | { String: string };

function fromRustValue(value: RustValue): ParameterValue {
  if ('Float' in value) return value.Float;
  if ('Boolean' in value) return value.Boolean;
  return value.String;
}

function toRustValue(value: ParameterValue): RustValue {
  if (typeof value === 'number') return { Float: value };
  if (typeof value === 'boolean') return { Boolean: value };
  return { String: value };
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

async function readSnapshot(): Promise<PlatformSnapshot> {
  const serialized = await invoke<string>('save_workflow');
  const graph = JSON.parse(serialized) as {
    nodes: Record<string, { type_id: string; parameters: Record<string, RustValue> }>;
    edges: Array<{ from_node: string; from_port: string; to_node: string; to_port: string }>;
    revision?: number;
  };
  return {
    nodes: Object.entries(graph.nodes ?? {}).map(([id, node]) => ({
      id,
      typeId: node.type_id,
      parameters: Object.fromEntries(
        Object.entries(node.parameters).map(([key, value]) => [key, fromRustValue(value)]),
      ),
    })),
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
  return {
    async nodeDescriptors() {
      try {
        const descriptors = await invoke<RustDescriptor[]>('node_descriptors');
        return descriptors.map(mapDescriptor);
      } catch (error) {
        throw message(error);
      }
    },
    snapshot: readSnapshot,
    async openImage(path) {
      try {
        return await invoke<OpenImageResult>('open_image', { path });
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
        await invoke('set_node_parameter', { nodeId, parameterId, value: toRustValue(value) });
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
