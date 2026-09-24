import { invoke } from '@tauri-apps/api/core';
import { isTauriRuntime } from './runtime';
import type { NodeDescriptor, ParameterType, ParameterValue, PortDescriptor, WorkflowDependency, WorkflowMetadata, WorkflowParameter, WorkflowPort } from '../editor/types';
import type { BatchPlatform, BatchCreateOptions, BatchDependencies, BatchDiagnostic, BatchDryRunResult, BatchItem, BatchJob, BatchPreflightOptions, BatchPreflightReport, BatchRecipe, BatchSubset, BatchWorkflowDefinition } from '../batch/types';

interface RustValue {
  Float?: number;
  Integer?: number;
  Boolean?: boolean;
  String?: string;
}

interface RustRecipe {
  format: string;
  resolution: unknown;
  bit_depth: string;
  color_space: unknown;
  icc_profile?: string | null;
  ocio_transform?: string | null;
  metadata_policy: string;
  sharpening: unknown;
  quality: number;
  compression: string;
  destination: string;
  filename_template: string;
  collision_policy: string;
}

interface RustBatchItem {
  id: string;
  source_path: string;
  display_name: string;
  overrides?: Record<string, RustValue>;
  test_set?: boolean;
  state?: string;
  attempts?: number;
  failure?: string | null;
  outputs?: Array<{ path: string; sha256: string; byte_len: number }>;
}

interface RustWorkflowDefinition {
  identity: { id: string; version: string };
  graph: {
    nodes: Record<string, RustGraphNode> | RustGraphNode[];
    edges: RustGraphEdge[];
    revision?: number;
  };
  parameters?: Record<string, RustWorkflowParameter> | RustWorkflowParameter[];
  inputs?: RustWorkflowPort[];
  outputs?: RustWorkflowPort[];
  subgraph_dependencies?: RustWorkflowDependency[];
  node_pack_dependencies?: RustWorkflowDependency[];
  metadata: RustWorkflowMetadata;
  nested_subgraphs?: Record<string, RustWorkflowDefinition>;
}

interface RustGraphNode {
  id: string;
  type_id?: string;
  typeId?: string;
  descriptor: RustDescriptor;
  parameters: Record<string, RustValue>;
  exposed_parameters?: string[];
  exposedParameters?: string[];
}

interface RustGraphEdge {
  from_node?: string;
  fromNode?: string;
  from_port?: string;
  fromPort?: string;
  to_node?: string;
  toNode?: string;
  to_port?: string;
  toPort?: string;
}

interface RustDescriptor {
  type_id: string;
  name: string;
  version: number;
  inputs: Array<{ id: string; name: string; data_type: string; required: boolean }>;
  outputs: Array<{ id: string; name: string; data_type: string; required: boolean }>;
  parameters: Array<{ id: string; name: string; parameter_type: ParameterType; default: RustValue; min: number | null; max: number | null }>;
  evaluation_policy?: 'automatic' | 'manual_checkpoint';
  capabilities?: string[];
  lazy_inputs?: unknown[];
}

interface RustWorkflowParameter {
  id: string;
  name: string;
  node_id: string;
  parameter_id: string;
  parameter_type: ParameterType;
  default: RustValue;
}

interface RustWorkflowPort {
  id: string;
  name: string;
  direction: 'Input' | 'Output';
  node_id: string;
  port_id: string;
  data_type: string;
  required: boolean;
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
  recommended_input_type?: string | null;
  minimum_app_version?: string | null;
}

interface RustBatchJob {
  schema_version: number;
  id: string;
  workflow: { definition: RustWorkflowDefinition; revision: number; hash: string };
  dependencies: { node_packs: RustWorkflowDependency[]; subgraphs: RustWorkflowDependency[]; plugins: Record<string, string>; external_providers: Record<string, string> };
  overrides?: Record<string, RustValue>;
  recipes: RustRecipe[];
  checkpoint_policy: BatchJob['checkpointPolicy'];
  items: RustBatchItem[];
  state?: BatchJob['state'];
}

function message(error: unknown): Error {
  if (error instanceof Error) return error;
  return new Error(typeof error === 'string' ? error : JSON.stringify(error));
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function toRustValue(value: ParameterValue): RustValue {
  if (typeof value === 'number') return Number.isInteger(value) ? { Integer: value } : { Float: value };
  if (typeof value === 'boolean') return { Boolean: value };
  return { String: value };
}

function fromRustValue(value: RustValue | ParameterValue): ParameterValue {
  if (typeof value !== 'object' || value === null) return value;
  if (value.Float !== undefined) return value.Float;
  if (value.Integer !== undefined) return value.Integer;
  if (value.Boolean !== undefined) return value.Boolean;
  return value.String ?? '';
}

function rustPort(port: PortDescriptor): Record<string, unknown> {
  return { id: port.id, name: port.name, data_type: port.dataType, required: port.required };
}

function rustWorkflowPort(port: WorkflowPort): RustWorkflowPort {
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

function rustDescriptor(descriptor: NodeDescriptor): RustDescriptor {
  return {
    type_id: descriptor.typeId,
    name: descriptor.name,
    version: descriptor.version,
    inputs: descriptor.inputs.map((port) => rustPort(port) as RustDescriptor['inputs'][number]),
    outputs: descriptor.outputs.map((port) => rustPort(port) as RustDescriptor['outputs'][number]),
    parameters: descriptor.parameters.map((parameter) => ({
      id: parameter.id, name: parameter.name, parameter_type: parameter.parameterType,
      default: toRustValue(parameter.default), min: parameter.min, max: parameter.max,
    })),
    evaluation_policy: descriptor.evaluationPolicy ?? 'automatic',
    capabilities: (descriptor.capabilities ?? []).map((capability) => capability === 'CPU' ? 'Cpu' : capability === 'GPU' ? 'Gpu' : capability),
    lazy_inputs: descriptor.lazyInputs,
  };
}

function rustDefinition(definition: BatchWorkflowDefinition): RustWorkflowDefinition {
  const nodes = Array.isArray(definition.graph.nodes)
    ? definition.graph.nodes
    : Object.values(definition.graph.nodes);
  const parameters = Array.isArray(definition.parameters)
    ? definition.parameters
    : Object.values(definition.parameters);
  return {
    identity: clone(definition.identity),
    graph: {
      nodes: Object.fromEntries(nodes.map((node) => [node.id, {
        id: node.id,
        type_id: node.typeId,
        descriptor: rustDescriptor(node.descriptor),
        parameters: Object.fromEntries(Object.entries(node.parameters).map(([key, value]) => [key, toRustValue(value)])),
        exposed_parameters: [...node.exposedParameters],
      }])),
      edges: definition.graph.edges.map((edge) => ({
        from_node: edge.fromNode, from_port: edge.fromPort, to_node: edge.toNode, to_port: edge.toPort,
      })),
      revision: definition.graph.revision,
    },
    parameters: Object.fromEntries(parameters.map((parameter) => [parameter.id, {
      id: parameter.id,
      name: parameter.name,
      node_id: parameter.nodeId,
      parameter_id: parameter.parameterId,
      parameter_type: parameter.parameterType,
      default: toRustValue(parameter.default),
    }])),
    inputs: definition.inputs.map(rustWorkflowPort),
    outputs: definition.outputs.map(rustWorkflowPort),
    subgraph_dependencies: definition.subgraphDependencies.map(rustDependency),
    node_pack_dependencies: definition.nodePackDependencies.map(rustDependency),
    metadata: rustMetadata(definition.metadata),
    nested_subgraphs: Object.fromEntries(Object.entries(definition.nestedSubgraphs).map(([id, child]) => [id, rustDefinition(child)])),
  };
}

function rustDependency(dependency: WorkflowDependency): RustWorkflowDependency {
  return { id: dependency.id, version: dependency.version, hash: dependency.hash ?? null };
}

function rustMetadata(metadata: WorkflowMetadata): RustWorkflowMetadata {
  return {
    name: metadata.name,
    author: metadata.author ?? null,
    description: metadata.description ?? null,
    thumbnail: metadata.thumbnail ?? null,
    tags: metadata.tags ?? [],
    license: metadata.license ?? null,
    recommended_input_type: metadata.recommendedInputType ?? null,
    minimum_app_version: metadata.minimumAppVersion ?? null,
  };
}

function rustRecipe(recipe: BatchRecipe): RustRecipe {
  return {
    format: recipe.format,
    resolution: recipe.resolution,
    bit_depth: recipe.bitDepth,
    color_space: recipe.colorSpace,
    icc_profile: recipe.iccProfile,
    ocio_transform: recipe.ocioTransform,
    metadata_policy: recipe.metadataPolicy,
    sharpening: recipe.sharpening,
    quality: recipe.quality,
    compression: recipe.compression,
    destination: recipe.destination,
    filename_template: recipe.filenameTemplate,
    collision_policy: recipe.collisionPolicy,
  };
}

function rustJob(job: BatchJob): RustBatchJob {
  return {
    schema_version: job.schemaVersion,
    id: job.id,
    workflow: { definition: rustDefinition(job.workflow.definition), revision: job.workflow.revision, hash: job.workflow.hash },
    dependencies: {
      node_packs: job.dependencies.nodePacks.map(rustDependency),
      subgraphs: job.dependencies.subgraphs.map(rustDependency),
      plugins: clone(job.dependencies.plugins),
      external_providers: clone(job.dependencies.externalProviders),
    },
    overrides: Object.fromEntries(Object.entries(job.overrides).map(([key, value]) => [key, toRustValue(value)])),
    recipes: job.recipes.map(rustRecipe),
    checkpoint_policy: job.checkpointPolicy,
    items: job.items.map((item) => ({
      id: item.id, source_path: item.sourcePath, display_name: item.displayName,
      overrides: Object.fromEntries(Object.entries(item.overrides).map(([key, value]) => [key, toRustValue(value)])),
      test_set: item.testSet, state: item.state, attempts: item.attempts, failure: item.failure,
      outputs: item.outputs.map((output) => ({ path: output.path, sha256: output.sha256, byte_len: output.byteLen })),
    })),
    state: job.state,
  };
}

function mapPort(port: RustWorkflowPort): WorkflowPort {
  return { id: port.id, name: port.name, direction: port.direction, nodeId: port.node_id, portId: port.port_id, dataType: port.data_type, required: port.required };
}

function mapDependency(dependency: RustWorkflowDependency): WorkflowDependency {
  return { id: dependency.id, version: dependency.version, ...(dependency.hash ? { hash: dependency.hash } : {}) };
}

function mapMetadata(metadata: RustWorkflowMetadata): WorkflowMetadata {
  return {
    name: metadata.name,
    author: metadata.author ?? null,
    description: metadata.description ?? null,
    thumbnail: metadata.thumbnail ?? null,
    tags: metadata.tags ?? [],
    license: metadata.license ?? null,
    recommendedInputType: metadata.recommended_input_type ?? null,
    minimumAppVersion: metadata.minimum_app_version ?? null,
  };
}

function mapDescriptor(descriptor: RustDescriptor): NodeDescriptor {
  return {
    typeId: descriptor.type_id,
    name: descriptor.name,
    version: descriptor.version,
    inputs: descriptor.inputs.map((port) => ({ id: port.id, name: port.name, dataType: port.data_type, required: port.required })),
    outputs: descriptor.outputs.map((port) => ({ id: port.id, name: port.name, dataType: port.data_type, required: port.required })),
    parameters: descriptor.parameters.map((parameter) => ({ id: parameter.id, name: parameter.name, parameterType: parameter.parameter_type, default: fromRustValue(parameter.default), min: parameter.min, max: parameter.max })),
    evaluationPolicy: descriptor.evaluation_policy ?? 'automatic',
    capabilities: (descriptor.capabilities ?? []).map((capability) => {
      if (capability === 'Cpu') return 'CPU';
      if (capability === 'Gpu') return 'GPU';
      return capability as NodeDescriptor['capabilities'] extends Array<infer Capability> ? Capability : never;
    }),
    lazyInputs: descriptor.lazy_inputs as NodeDescriptor['lazyInputs'],
  };
}

function mapDefinition(definition: RustWorkflowDefinition): BatchWorkflowDefinition {
  const rawNodes = Array.isArray(definition.graph.nodes) ? definition.graph.nodes : Object.values(definition.graph.nodes ?? {});
  const rawParameters = Array.isArray(definition.parameters) ? definition.parameters : Object.values(definition.parameters ?? {});
  return {
    identity: definition.identity,
    graph: {
      nodes: rawNodes.map((node) => ({
        id: node.id,
        typeId: node.type_id ?? node.typeId ?? '',
        descriptor: mapDescriptor(node.descriptor),
        parameters: Object.fromEntries(Object.entries(node.parameters ?? {}).map(([key, value]) => [key, fromRustValue(value)])),
        exposedParameters: node.exposed_parameters ?? node.exposedParameters ?? [],
      })),
      edges: (definition.graph.edges ?? []).map((edge) => ({
        fromNode: edge.from_node ?? edge.fromNode ?? '',
        fromPort: edge.from_port ?? edge.fromPort ?? '',
        toNode: edge.to_node ?? edge.toNode ?? '',
        toPort: edge.to_port ?? edge.toPort ?? '',
      })),
      revision: definition.graph.revision,
    },
    parameters: rawParameters.map((parameter): WorkflowParameter => ({
      id: parameter.id, name: parameter.name, nodeId: parameter.node_id, parameterId: parameter.parameter_id,
      parameterType: parameter.parameter_type, default: fromRustValue(parameter.default),
    })),
    inputs: (definition.inputs ?? []).map(mapPort),
    outputs: (definition.outputs ?? []).map(mapPort),
    subgraphDependencies: (definition.subgraph_dependencies ?? []).map(mapDependency),
    nodePackDependencies: (definition.node_pack_dependencies ?? []).map(mapDependency),
    metadata: mapMetadata(definition.metadata),
    nestedSubgraphs: Object.fromEntries(Object.entries(definition.nested_subgraphs ?? {}).map(([id, child]) => [id, mapDefinition(child)])),
  };
}

function mapRecipe(recipe: RustRecipe | BatchRecipe): BatchRecipe {
  const value = recipe as RustRecipe & BatchRecipe;
  return {
    format: value.format as BatchRecipe['format'],
    resolution: value.resolution as BatchRecipe['resolution'],
    bitDepth: (value.bit_depth ?? value.bitDepth) as BatchRecipe['bitDepth'],
    colorSpace: (value.color_space ?? value.colorSpace) as BatchRecipe['colorSpace'],
    iccProfile: value.icc_profile ?? value.iccProfile ?? null,
    ocioTransform: value.ocio_transform ?? value.ocioTransform ?? null,
    metadataPolicy: (value.metadata_policy ?? value.metadataPolicy) as BatchRecipe['metadataPolicy'],
    sharpening: value.sharpening,
    quality: value.quality,
    compression: value.compression,
    destination: value.destination,
    filenameTemplate: value.filename_template ?? value.filenameTemplate,
    collisionPolicy: (value.collision_policy ?? value.collisionPolicy) as BatchRecipe['collisionPolicy'],
  };
}

function mapItem(item: RustBatchItem | BatchItem): BatchItem {
  const value = item as RustBatchItem & BatchItem;
  return {
    id: value.id,
    sourcePath: value.source_path ?? value.sourcePath,
    displayName: value.display_name ?? value.displayName,
    overrides: Object.fromEntries(Object.entries(value.overrides ?? {}).map(([key, entry]) => [key, fromRustValue(entry)])),
    testSet: value.test_set ?? value.testSet ?? false,
    state: value.state ?? 'waiting',
    attempts: value.attempts ?? 0,
    failure: value.failure ?? null,
    outputs: (value.outputs ?? []).map((output) => {
      const raw = output as { path: string; sha256: string; byte_len?: number; byteLen?: number };
      return { path: raw.path, sha256: raw.sha256, byteLen: raw.byte_len ?? raw.byteLen ?? 0 };
    }),
  };
}

function mapJob(raw: RustBatchJob | BatchJob): BatchJob {
  const value = raw as RustBatchJob & BatchJob;
  const dependencies = value.dependencies as RustBatchJob['dependencies'] & BatchDependencies;
  return {
    schemaVersion: value.schema_version ?? value.schemaVersion,
    id: value.id,
    workflow: {
      definition: mapDefinition(value.workflow.definition as RustWorkflowDefinition),
      revision: value.workflow.revision,
      hash: value.workflow.hash,
    },
    dependencies: {
      nodePacks: (dependencies.node_packs ?? dependencies.nodePacks ?? []).map(mapDependency),
      subgraphs: (dependencies.subgraphs ?? dependencies.subgraphs ?? []).map(mapDependency),
      plugins: dependencies.plugins ?? {},
      externalProviders: dependencies.external_providers ?? dependencies.externalProviders ?? {},
    },
    overrides: Object.fromEntries(Object.entries(value.overrides ?? {}).map(([key, entry]) => [key, fromRustValue(entry)])),
    recipes: value.recipes.map(mapRecipe),
    checkpointPolicy: value.checkpoint_policy ?? value.checkpointPolicy,
    items: value.items.map(mapItem),
    state: value.state ?? 'draft',
  };
}

function rustSubset(subset: BatchSubset): unknown {
  switch (subset.kind) {
    case 'current-preview': return { current_preview: { item_id: subset.itemId } };
    case 'test-set': return 'test_set';
    case 'first-n': return { first_n: Math.max(0, Math.trunc(subset.count)) };
    case 'selected': return { selected: [...subset.itemIds] };
    case 'all': return 'all';
  }
}

function mapDiagnostic(diagnostic: BatchDiagnostic): BatchDiagnostic {
  return { severity: diagnostic.severity, code: diagnostic.code, message: diagnostic.message, itemId: diagnostic.itemId ?? null, path: diagnostic.path ?? null };
}

export function createTauriBatchPlatform(): BatchPlatform {
  const call = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    try {
      return await invoke<T>(command, args);
    } catch (error) {
      throw message(error);
    }
  };
  return {
    async createJob(job, options = {}) {
      const raw = await call<RustBatchJob>('create_batch_job', { request: {
        job: rustJob(job), statePath: options.statePath ?? null, maxWorkers: options.maxWorkers ?? 4,
      } });
      return mapJob(raw);
    },
    async loadJob(statePath, maxWorkers = 4) {
      return mapJob(await call<RustBatchJob>('load_batch_job', { request: { statePath, maxWorkers } }));
    },
    async preflight(jobId, options) {
      const report = await call<BatchPreflightReport>('batch_preflight', { jobId, options: options ? {
        check_source_files: options.checkSourceFiles,
        check_output_directories: options.checkOutputDirectories,
        check_collisions: options.checkCollisions,
        available_node_packs: options.availableNodePacks,
        available_subgraphs: options.availableSubgraphs,
        available_plugins: options.availablePlugins,
        available_external_providers: options.availableExternalProviders,
        available_disk_bytes: options.availableDiskBytes,
      } : undefined });
      return { diagnostics: report.diagnostics.map(mapDiagnostic) };
    },
    async start(jobId) { return mapJob(await call<RustBatchJob>('start_batch', { jobId })); },
    async pause(jobId) { return mapJob(await call<RustBatchJob>('pause_batch', { jobId })); },
    async resume(jobId) { return mapJob(await call<RustBatchJob>('resume_batch', { jobId })); },
    async cancel(jobId) { return mapJob(await call<RustBatchJob>('cancel_batch', { jobId })); },
    async retryFailed(jobId) { return call<number>('retry_failed_batch', { jobId }); },
    async retrySelected(jobId, itemIds) { return call<number>('retry_selected_batch', { jobId, itemIds }); },
    async skip(jobId, itemIds) { return call<number>('skip_batch_items', { jobId, itemIds }); },
    async snapshot(jobId) { return mapJob(await call<RustBatchJob>('batch_snapshot', { jobId })); },
    async dryRun(jobId, subset) {
      const result = await call<{ workflowRevision: number; workflowHash: string; itemIds: string[]; recipes: RustRecipe[] }>('batch_dry_run', { jobId, subset: rustSubset(subset) });
      return { workflowRevision: result.workflowRevision, workflowHash: result.workflowHash, itemIds: result.itemIds, recipes: result.recipes.map(mapRecipe) };
    },
    async openFailedItem(jobId, itemId) { return mapItem(await call<RustBatchItem>('open_failed_batch_item', { jobId, itemId })); },
  };
}

export function createBatchPlatform(initialJobs: BatchJob[] = []): BatchPlatform {
  if (isTauriRuntime()) return createTauriBatchPlatform();
  return createMemoryBatchPlatform(initialJobs);
}
function itemCanRetry(state: BatchItem['state']): boolean {
  return state === 'failed' || state === 'cancelled' || state === 'skipped';
}

function findJob(jobs: Map<string, BatchJob>, jobId: string): BatchJob {
  const job = jobs.get(jobId);
  if (!job) throw new Error(`batch job '${jobId}' is not loaded`);
  return job;
}

function memoryCloneJob(job: BatchJob): BatchJob {
  return clone(job);
}

export function createMemoryBatchPlatform(initialJobs: BatchJob[] = []): BatchPlatform {
  const jobs = new Map(initialJobs.map((job) => [job.id, memoryCloneJob(job)]));
  return {
    async createJob(job) {
      if (jobs.has(job.id)) throw new Error(`batch job '${job.id}' is already loaded`);
      const copy = memoryCloneJob(job);
      jobs.set(copy.id, copy);
      return memoryCloneJob(copy);
    },
    async loadJob(statePath) {
      const job = [...jobs.values()].find((candidate) => candidate.id === statePath || candidate.id === statePath.split('/').at(-1)?.replace(/\.json$/, ''));
      if (!job) throw new Error(`batch state '${statePath}' is not available in memory`);
      return memoryCloneJob(job);
    },
    async preflight(jobId) {
      const job = findJob(jobs, jobId);
      return { diagnostics: [{ severity: 'info', code: 'checkpoint-policy', message: `checkpoint policy is ${job.checkpointPolicy}`, itemId: null, path: null }] };
    },
    async start(jobId) {
      const job = findJob(jobs, jobId);
      for (const item of job.items) if (item.state === 'waiting') { item.state = 'completed'; item.attempts += 1; }
      job.state = job.items.some((item) => item.state === 'failed') ? 'failed' : 'completed';
      return memoryCloneJob(job);
    },
    async pause(jobId) {
      const job = findJob(jobs, jobId);
      if (job.state === 'running') job.state = 'paused';
      return memoryCloneJob(job);
    },
    async resume(jobId) {
      const job = findJob(jobs, jobId);
      if (job.state === 'paused') job.state = 'running';
      return memoryCloneJob(job);
    },
    async cancel(jobId) {
      const job = findJob(jobs, jobId);
      for (const item of job.items) if (item.state === 'waiting' || item.state === 'running') item.state = 'cancelled';
      job.state = 'cancelled';
      return memoryCloneJob(job);
    },
    async retryFailed(jobId) {
      const job = findJob(jobs, jobId);
      const count = job.items.filter((item) => item.state === 'failed').length;
      for (const item of job.items) if (item.state === 'failed') item.state = 'waiting';
      if (count) job.state = 'draft';
      return count;
    },
    async retrySelected(jobId, itemIds) {
      const job = findJob(jobs, jobId);
      let count = 0;
      for (const item of job.items) if (itemIds.includes(item.id) && itemCanRetry(item.state)) { item.state = 'waiting'; item.failure = null; count += 1; }
      if (count) job.state = 'draft';
      return count;
    },
    async skip(jobId, itemIds) {
      const job = findJob(jobs, jobId);
      let count = 0;
      for (const item of job.items) if (itemIds.includes(item.id) && (item.state === 'waiting' || item.state === 'failed')) { item.state = 'skipped'; count += 1; }
      return count;
    },
    async snapshot(jobId) { return memoryCloneJob(findJob(jobs, jobId)); },
    async dryRun(jobId, subset) {
      const job = findJob(jobs, jobId);
      let items: BatchItem[];
      switch (subset.kind) {
        case 'current-preview': items = job.items.filter((item) => item.id === subset.itemId); break;
        case 'test-set': items = job.items.filter((item) => item.testSet); break;
        case 'first-n': items = job.items.slice(0, Math.max(0, Math.trunc(subset.count))); break;
        case 'selected': items = subset.itemIds.map((id) => job.items.find((item) => item.id === id)).filter((item): item is BatchItem => Boolean(item)); break;
        case 'all': items = job.items; break;
      }
      return { workflowRevision: job.workflow.revision, workflowHash: job.workflow.hash, itemIds: items.map((item) => item.id), recipes: clone(job.recipes) };
    },
    async openFailedItem(jobId, itemId) {
      const item = findJob(jobs, jobId).items.find((candidate) => candidate.id === itemId);
      if (!item || item.state !== 'failed') throw new Error(`batch item '${itemId}' is not failed`);
      return memoryCloneJob({ ...findJob(jobs, jobId), items: [item] }).items[0];
    },
  };
}
