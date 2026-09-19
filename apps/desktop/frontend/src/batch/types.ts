import type {
  NodeDescriptor,
  ParameterValue,
  PlatformEdge,
  WorkflowDependency,
  WorkflowMetadata,
  WorkflowParameter,
  WorkflowPort,
  WorkflowIdentity,
} from '../editor/types';

export type BatchJobState = 'draft' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';
export type BatchItemState = 'waiting' | 'running' | 'completed' | 'skipped' | 'failed' | 'cancelled';
export type BatchFormat = 'jpeg' | 'png' | 'tiff' | 'open_exr';
export type BatchResolution = 'original' | { exact: { width: number; height: number } } | { long_edge: number };
export type BatchBitDepth = 'eight' | 'sixteen' | 'float32';
export type BatchColorSpace = 'linear_srgb' | 'srgb' | 'display_p3' | { named: string };
export type BatchMetadataPolicy = 'preserve' | 'strip' | 'sidecar';
export type BatchCompression = 'default' | 'fast' | 'best' | 'lossless';
export type BatchCollisionPolicy = 'error' | 'skip' | 'overwrite' | 'suffix';
export type BatchSharpening = 'None' | { UnsharpMask: { radius: number; amount: number; threshold: number } };
export type BatchCheckpointPolicy =
  | 'after_each_item'
  | 'after_each_output'
  | 'manual'
  | 'use_committed'
  | 'generate_if_missing'
  | 'regenerate_all'
  | 'fail_if_stale';

export interface BatchRecipe {
  format: BatchFormat;
  resolution: BatchResolution;
  bitDepth: BatchBitDepth;
  colorSpace: BatchColorSpace;
  iccProfile: string | null;
  ocioTransform: string | null;
  metadataPolicy: BatchMetadataPolicy;
  sharpening: BatchSharpening;
  quality: number;
  compression: BatchCompression;
  destination: string;
  filenameTemplate: string;
  collisionPolicy: BatchCollisionPolicy;
}

export interface BatchOutputRecord {
  path: string;
  sha256: string;
  byteLen: number;
}

export interface BatchItem {
  id: string;
  sourcePath: string;
  displayName: string;
  overrides: Record<string, ParameterValue>;
  testSet: boolean;
  state: BatchItemState;
  attempts: number;
  failure: string | null;
  outputs: BatchOutputRecord[];
}

export interface BatchDependencies {
  nodePacks: WorkflowDependency[];
  subgraphs: WorkflowDependency[];
  plugins: Record<string, string>;
  externalProviders: Record<string, string>;
}

export interface BatchGraphNode {
  id: string;
  typeId: string;
  descriptor: NodeDescriptor;
  parameters: Record<string, ParameterValue>;
  exposedParameters: string[];
}

export interface BatchWorkflowDefinition {
  identity: WorkflowIdentity;
  graph: { nodes: BatchGraphNode[] | Record<string, BatchGraphNode>; edges: PlatformEdge[]; revision?: number };
  parameters: WorkflowParameter[] | Record<string, WorkflowParameter>;
  inputs: WorkflowPort[];
  outputs: WorkflowPort[];
  subgraphDependencies: WorkflowDependency[];
  nodePackDependencies: WorkflowDependency[];
  metadata: WorkflowMetadata;
  nestedSubgraphs: Record<string, BatchWorkflowDefinition>;
}

export interface BatchWorkflowPin {
  definition: BatchWorkflowDefinition;
  revision: number;
  hash: string;
}

export interface BatchJob {
  schemaVersion: number;
  id: string;
  workflow: BatchWorkflowPin;
  dependencies: BatchDependencies;
  overrides: Record<string, ParameterValue>;
  recipes: BatchRecipe[];
  checkpointPolicy: BatchCheckpointPolicy;
  items: BatchItem[];
  state: BatchJobState;
}

export interface BatchSessionReference {
  jobId: string | null;
  statePath: string | null;
}

export interface BatchDiagnostic {
  severity: 'error' | 'warning' | 'info';
  code: string;
  message: string;
  itemId: string | null;
  path: string | null;
}

export interface BatchPreflightReport {
  diagnostics: BatchDiagnostic[];
}

export interface BatchPreflightOptions {
  checkSourceFiles?: boolean;
  checkOutputDirectories?: boolean;
  checkCollisions?: boolean;
  availableNodePacks?: unknown[];
  availableSubgraphs?: Record<string, string>;
  availablePlugins?: Record<string, string>;
  availableExternalProviders?: Record<string, string>;
  availableDiskBytes?: number | null;
}

export type BatchSubset =
  | { kind: 'current-preview'; itemId: string }
  | { kind: 'test-set' }
  | { kind: 'first-n'; count: number }
  | { kind: 'selected'; itemIds: string[] }
  | { kind: 'all' };

export interface BatchDryRunResult {
  workflowRevision: number;
  workflowHash: string;
  itemIds: string[];
  recipes: BatchRecipe[];
}

export interface BatchCreateOptions {
  statePath?: string | null;
  maxWorkers?: number;
}

export interface BatchPlatform {
  createJob(job: BatchJob, options?: BatchCreateOptions): Promise<BatchJob>;
  loadJob(statePath: string, maxWorkers?: number): Promise<BatchJob>;
  preflight(jobId: string, options?: BatchPreflightOptions): Promise<BatchPreflightReport>;
  start(jobId: string): Promise<BatchJob>;
  pause(jobId: string): Promise<BatchJob>;
  resume(jobId: string): Promise<BatchJob>;
  cancel(jobId: string): Promise<BatchJob>;
  retryFailed(jobId: string): Promise<number>;
  retrySelected(jobId: string, itemIds: string[]): Promise<number>;
  skip(jobId: string, itemIds: string[]): Promise<number>;
  snapshot(jobId: string): Promise<BatchJob>;
  dryRun(jobId: string, subset: BatchSubset): Promise<BatchDryRunResult>;
  openFailedItem(jobId: string, itemId: string): Promise<BatchItem>;
}

export interface BatchState {
  job: BatchJob | null;
  diagnostics: BatchDiagnostic[];
  dryRun: BatchDryRunResult | null;
  loading: boolean;
  operation: string | null;
  error: string | null;
  openedItem?: BatchItem | null;
}
