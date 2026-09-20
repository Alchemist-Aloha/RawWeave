import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type {
  DependencyReport,
  DependencyStatus,
  EditorPlatform,
  ExecutionCapability,
  LazyInputGate,
  NodeDescriptor,
  OpenImageResult,
  OpenImageSetResult,
  ParameterType,
  ParameterValue,
  PlatformSnapshot,
  WorkflowDefinition,
  WorkflowDependency,
  WorkflowMetadata,
  WorkflowParameter,
  WorkflowPort,
  WorkflowSummary,
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
  evaluation_policy?: 'automatic' | 'manual_checkpoint';
  capabilities?: string[];
  lazy_inputs?: LazyInputGate[];
}

interface RustOpenImageResult {
  kind: 'ordinary' | 'raw';
  width: number;
  height: number;
  revision: number;
  metadata: OpenImageResult['metadata'];
}

interface RustOpenImageSetResult {
  kind: 'imageset';
  order: OpenImageSetResult['order'];
  revision: number;
  members: OpenImageSetResult['members'];
  sharedMetadata: OpenImageSetResult['sharedMetadata'];
  alignment: OpenImageSetResult['alignment'];
}

interface RustGraphNode {
  id: string;
  typeId?: string;
  type_id?: string;
  parameters: Record<string, RustValue>;
  exposedParameters?: string[];
  exposed_parameters?: string[];
}

interface RustGraphEdge {
  fromNode?: string;
  from_node?: string;
  fromPort?: string;
  from_port?: string;
  toNode?: string;
  to_node?: string;
  toPort?: string;
  to_port?: string;
}

interface RustWorkflowGraph {
  nodes: RustGraphNode[] | Record<string, RustGraphNode>;
  edges: RustGraphEdge[];
  revision?: number;
}

interface RustWorkflowPort {
  id: string;
  name: string;
  direction: 'Input' | 'Output';
  nodeId: string;
  portId: string;
  dataType: string;
  required: boolean;
}

interface RustWorkflowParameter {
  id: string;
  name: string;
  nodeId: string;
  parameterId: string;
  parameterType: ParameterType;
  default: RustValue;
}

interface RustWorkflowDefinition {
  identity: { id: string; version: string };
  graph: RustWorkflowGraph;
  parameters: RustWorkflowParameter[];
  inputs: RustWorkflowPort[];
  outputs: RustWorkflowPort[];
  subgraphDependencies: RustWorkflowDependency[];
  nodePackDependencies: RustWorkflowDependency[];
  metadata: RustWorkflowMetadata;
  nestedSubgraphs: Record<string, RustWorkflowDefinition>;
  hash: string;
}

interface RustWorkflowDependency {
  id: string;
  version: string;
  hash?: string | null;
}

interface RustWorkflowMetadata {
  name: string;
  author?: string | null;
  description?: string | null;
  thumbnail?: string | null;
  tags?: string[];
  license?: string | null;
  recommendedInputType?: string | null;
  minimumAppVersion?: string | null;
}

interface RustDependencyDiagnostic {
  id: string;
  requiredVersion: string;
  availableVersion?: string | null;
}

interface RustDependencyReport {
  available: RustDependencyDiagnostic[];
  missing: RustDependencyDiagnostic[];
  mismatched: RustDependencyDiagnostic[];
  disabledNodes: string[];
  statuses?: Record<string, { kind: string; required?: string; available?: string }>;
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

function mapOpenImageSetResult(result: RustOpenImageSetResult): OpenImageSetResult {
  return {
    kind: 'imageset',
    order: result.order,
    revision: result.revision,
    members: result.members,
    sharedMetadata: result.sharedMetadata,
    alignment: result.alignment,
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
    evaluationPolicy: descriptor.evaluation_policy ?? 'automatic',
    capabilities: descriptor.capabilities?.map(mapCapability),
    lazyInputs: descriptor.lazy_inputs,
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

function mapDependencyStatus(status: { kind: string; required?: string; available?: string }): DependencyStatus {
  switch (status.kind) {
    case 'available':
      return { kind: 'available' };
    case 'version-mismatch':
      return {
        kind: 'version-mismatch',
        required: status.required ?? '',
        available: status.available ?? '',
      };
    default:
      return { kind: 'missing' };
  }
}

export function mapDependencyReport(report: RustDependencyReport): DependencyReport {
  const statuses = Object.fromEntries(
    Object.entries(report.statuses ?? {}).map(([id, status]) => [id, mapDependencyStatus(status)]),
  );
  return {
    available: report.available ?? [],
    missing: report.missing ?? [],
    mismatched: report.mismatched ?? [],
    disabledNodes: report.disabledNodes ?? [],
    statuses,
  };
}

function mapWorkflowGraph(graph: RustWorkflowGraph): PlatformSnapshot {
  const rawNodes = Array.isArray(graph.nodes)
    ? graph.nodes.map((node) => [node.id, node] as const)
    : Object.entries(graph.nodes ?? {});
  return {
    nodes: rawNodes.map(([id, node]) => ({
      id,
      typeId: node.typeId ?? node.type_id ?? '',
      parameters: Object.fromEntries(
        Object.entries(node.parameters ?? {}).map(([key, value]) => [key, fromRustValue(value)]),
      ),
      exposedParameters: node.exposedParameters ?? node.exposed_parameters ?? [],
    })),
    edges: (graph.edges ?? []).map((edge) => ({
      fromNode: edge.fromNode ?? edge.from_node ?? '',
      fromPort: edge.fromPort ?? edge.from_port ?? '',
      toNode: edge.toNode ?? edge.to_node ?? '',
      toPort: edge.toPort ?? edge.to_port ?? '',
    })),
    revision: graph.revision,
  };
}

function mapWorkflowMetadata(metadata: RustWorkflowMetadata): WorkflowMetadata {
  return {
    name: metadata.name,
    author: metadata.author,
    description: metadata.description,
    thumbnail: metadata.thumbnail,
    tags: metadata.tags ?? [],
    license: metadata.license,
    recommendedInputType: metadata.recommendedInputType,
    minimumAppVersion: metadata.minimumAppVersion,
  };
}

function mapWorkflowPort(port: RustWorkflowPort): WorkflowPort {
  return {
    id: port.id,
    name: port.name,
    direction: port.direction,
    nodeId: port.nodeId,
    portId: port.portId,
    dataType: port.dataType,
    required: port.required,
  };
}

function mapWorkflowParameter(parameter: RustWorkflowParameter): WorkflowParameter {
  return {
    id: parameter.id,
    name: parameter.name,
    nodeId: parameter.nodeId,
    parameterId: parameter.parameterId,
    parameterType: parameter.parameterType,
    default: fromRustValue(parameter.default),
  };
}

function mapWorkflowDependency(dependency: RustWorkflowDependency): WorkflowDependency {
  return {
    id: dependency.id,
    version: dependency.version,
    ...(dependency.hash ? { hash: dependency.hash } : {}),
  };
}

export function mapWorkflowDefinition(definition: RustWorkflowDefinition): WorkflowDefinition {
  const graph = mapWorkflowGraph(definition.graph);
  const nestedSubgraphs = Object.fromEntries(
    Object.entries(definition.nestedSubgraphs ?? {}).map(([id, nested]) => [id, mapWorkflowDefinition(nested)]),
  );
  return {
    identity: definition.identity,
    graph,
    parameters: (definition.parameters ?? []).map(mapWorkflowParameter),
    inputs: (definition.inputs ?? []).map(mapWorkflowPort),
    outputs: (definition.outputs ?? []).map(mapWorkflowPort),
    subgraphDependencies: (definition.subgraphDependencies ?? []).map(mapWorkflowDependency),
    nodePackDependencies: (definition.nodePackDependencies ?? []).map(mapWorkflowDependency),
    metadata: mapWorkflowMetadata(definition.metadata),
    nestedSubgraphs,
    hash: definition.hash,
  };
}

function workflowSummary(definition: WorkflowDefinition): WorkflowSummary {
  return {
    id: definition.identity.id,
    name: definition.metadata.name,
    version: definition.identity.version,
    hash: definition.hash,
  };
}

function rustMetadata(metadata: WorkflowMetadata): RustWorkflowMetadata {
  return {
    name: metadata.name,
    author: metadata.author ?? null,
    description: metadata.description ?? null,
    thumbnail: metadata.thumbnail ?? null,
    tags: metadata.tags ?? [],
    license: metadata.license ?? null,
    recommendedInputType: metadata.recommendedInputType ?? null,
    minimumAppVersion: metadata.minimumAppVersion ?? null,
  };
}

function rustDependency(dependency: WorkflowDependency): RustWorkflowDependency {
  return {
    id: dependency.id,
    version: dependency.version,
    hash: dependency.hash ?? null,
  };
}

function message(error: unknown): Error {
  if (error instanceof Error) return error;
  if (typeof error === 'string') return new Error(error);
  return new Error(JSON.stringify(error));
}

const nodeTypes = new Map<string, string>();

async function readSnapshot(
  activeDefinition: WorkflowDefinition | null = null,
  scopePath: Array<{ id: string; name: string; version?: string; hash?: string }> = [
    { id: 'root', name: 'Workflow', version: '1.0.0' },
  ],
): Promise<PlatformSnapshot> {
  const serialized = await invoke<string>('save_workflow');
  const persisted = mapWorkflowGraph(JSON.parse(serialized) as RustWorkflowGraph);
  const [dependencyReport, workflowHash] = await Promise.all([
    invoke<RustDependencyReport>('dependency_status').catch(() => undefined),
    invoke<string>('workflow_hash').catch(() => undefined),
  ]);
  const useDefinitionGraph = activeDefinition !== null && scopePath.length > 1;
  const graph = useDefinitionGraph ? activeDefinition.graph : persisted;
  const nodes = graph.nodes;
  for (const node of nodes) nodeTypes.set(node.id, node.typeId);
  return {
    ...graph,
    scopePath,
    workflowInputs: activeDefinition?.inputs ?? [],
    workflowOutputs: activeDefinition?.outputs ?? [],
    workflowParameters: activeDefinition?.parameters ?? [],
    nestedSubgraphs: activeDefinition ? Object.values(activeDefinition.nestedSubgraphs).map(workflowSummary) : [],
    workflowHash: workflowHash ?? activeDefinition?.hash,
    dependencyReport: dependencyReport ? mapDependencyReport(dependencyReport) : undefined,
  };
}

export function createTauriPlatform(): EditorPlatform {
  let cachedDescriptors: NodeDescriptor[] = [];
  let activeDefinition: WorkflowDefinition | null = null;
  let scopePath: Array<{ id: string; name: string; version?: string; hash?: string }> = [
    { id: 'root', name: 'Workflow', version: '1.0.0' },
  ];
  const rememberDefinition = (definition: WorkflowDefinition): void => {
    activeDefinition = definition;
    const current = scopePath.at(-1);
    if (current?.id === definition.identity.id) {
      scopePath = [...scopePath.slice(0, -1), {
        id: definition.identity.id,
        name: definition.metadata.name,
        version: definition.identity.version,
        hash: definition.hash,
      }];
    }
  };
  const refreshActiveDefinition = async (): Promise<void> => {
    if (!activeDefinition) return;
    const serialized = await invoke<string>('save_blueprint');
    rememberDefinition(mapWorkflowDefinition(JSON.parse(serialized) as RustWorkflowDefinition));
  };
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
    snapshot: () => readSnapshot(activeDefinition, scopePath),
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
        activeDefinition = null;
        scopePath = [{ id: 'root', name: 'Workflow', version: '1.0.0' }];
        return mapOpenImageResult(result);
      } catch (error) {
        throw message(error);
      }
    },
    async openImageSet(paths, order) {
      try {
        const result = await invoke<RustOpenImageSetResult>('open_image_set', { paths, order });
        activeDefinition = null;
        scopePath = [{ id: 'root', name: 'Workflow', version: '1.0.0' }];
        return mapOpenImageSetResult(result);
      } catch (error) {
        throw message(error);
      }
    },
    async addNode(nodeId, typeId) {
      try {
        await invoke('add_node', { nodeId, typeId });
        await refreshActiveDefinition();
      } catch (error) {
        throw message(error);
      }
    },
    async removeNode(nodeId) {
      try {
        await invoke('remove_node', { nodeId });
        await refreshActiveDefinition();
      } catch (error) {
        throw message(error);
      }
    },
    async connect(fromNode, fromPort, toNode, toPort) {
      try {
        await invoke('connect_nodes', { fromNode, fromPort, toNode, toPort });
        await refreshActiveDefinition();
      } catch (error) {
        throw message(error);
      }
    },
    async disconnect(fromNode, fromPort, toNode, toPort) {
      try {
        await invoke('disconnect_nodes', { fromNode, fromPort, toNode, toPort });
        await refreshActiveDefinition();
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
        await refreshActiveDefinition();
      } catch (error) {
        throw message(error);
      }
    },
    async exposeParameter(nodeId, parameterId) {
      try {
        await invoke('expose_parameter', { nodeId, parameterId });
        await refreshActiveDefinition();
      } catch (error) {
        throw message(error);
      }
    },
    async unexposeParameter(nodeId, parameterId) {
      try {
        await invoke('unexpose_parameter', { nodeId, parameterId });
        await refreshActiveDefinition();
      } catch (error) {
        throw message(error);
      }
    },
    async exposeInput(nodeId, portId) {
      try {
        const definition = await invoke<RustWorkflowDefinition>('expose_workflow_input', { nodeId, portId });
        rememberDefinition(mapWorkflowDefinition(definition));
      } catch (error) {
        throw message(error);
      }
    },
    async exposeOutput(nodeId, portId) {
      try {
        const definition = await invoke<RustWorkflowDefinition>('expose_workflow_output', { nodeId, portId });
        rememberDefinition(mapWorkflowDefinition(definition));
      } catch (error) {
        throw message(error);
      }
    },
    async hidePort(portId) {
      try {
        const definition = await invoke<RustWorkflowDefinition>('hide_workflow_port', { portId });
        rememberDefinition(mapWorkflowDefinition(definition));
      } catch (error) {
        throw message(error);
      }
    },
    async createSubgraph(selection, options) {
      try {
        const definition = await invoke<RustWorkflowDefinition>('create_subgraph', {
          selection,
          id: options.id,
          version: options.version,
          metadata: rustMetadata(options.metadata),
          nodePackDependencies: (options.nodePackDependencies ?? []).map(rustDependency),
          subgraphDependencies: (options.subgraphDependencies ?? []).map(rustDependency),
        });
        const mapped = mapWorkflowDefinition(definition);
        rememberDefinition(mapped);
        return mapped;
      } catch (error) {
        throw message(error);
      }
    },
    async openSubgraph(id) {
      try {
        const definition = await invoke<RustWorkflowDefinition>('open_subgraph', { id });
        const mapped = mapWorkflowDefinition(definition);
        activeDefinition = mapped;
        scopePath = [
          ...scopePath,
          {
            id: mapped.identity.id,
            name: mapped.metadata.name,
            version: mapped.identity.version,
            hash: mapped.hash,
          },
        ];
        // The native navigation command changes blueprint scope; instantiate it
        // into the editor graph so graph edits and previews follow the scope.
        await invoke('instantiate_blueprint', { serialized: null });
      } catch (error) {
        throw message(error);
      }
    },
    async returnToParent() {
      try {
        const definition = await invoke<RustWorkflowDefinition>('return_to_parent');
        const mapped = mapWorkflowDefinition(definition);
        activeDefinition = mapped;
        scopePath = scopePath.length > 1 ? scopePath.slice(0, -1) : scopePath;
        await invoke('instantiate_blueprint', { serialized: null });
      } catch (error) {
        throw message(error);
      }
    },
    async saveBlueprint() {
      try {
        return await invoke<string>('save_blueprint');
      } catch (error) {
        throw message(error);
      }
    },
    async exportBlueprint() {
      try {
        return await invoke<string>('export_blueprint');
      } catch (error) {
        throw message(error);
      }
    },
    async loadBlueprint(serialized) {
      try {
        const definition = await invoke<RustWorkflowDefinition>('load_blueprint', { serialized });
        const mapped = mapWorkflowDefinition(definition);
        activeDefinition = mapped;
        scopePath = [{
          id: mapped.identity.id,
          name: mapped.metadata.name,
          version: mapped.identity.version,
          hash: mapped.hash,
        }];
        return mapped;
      } catch (error) {
        throw message(error);
      }
    },
    async importBlueprint(serialized) {
      try {
        const definition = await invoke<RustWorkflowDefinition>('import_blueprint', { serialized });
        const mapped = mapWorkflowDefinition(definition);
        activeDefinition = mapped;
        scopePath = [{
          id: mapped.identity.id,
          name: mapped.metadata.name,
          version: mapped.identity.version,
          hash: mapped.hash,
        }];
        return mapped;
      } catch (error) {
        throw message(error);
      }
    },
    async instantiateBlueprint(serialized) {
      try {
        await invoke('instantiate_blueprint', { serialized });
        await refreshActiveDefinition();
      } catch (error) {
        throw message(error);
      }
    },
    async dependencyStatus() {
      try {
        const report = await invoke<RustDependencyReport>('dependency_status');
        return mapDependencyReport(report);
      } catch (error) {
        throw message(error);
      }
    },
    async workflowHash() {
      try {
        return await invoke<string>('workflow_hash');
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
        activeDefinition = null;
        scopePath = [{ id: 'root', name: 'Workflow', version: '1.0.0' }];
      } catch (error) {
        throw message(error);
      }
    },
  };
}
