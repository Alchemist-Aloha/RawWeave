import type { EditorNode, ParameterValue, PlatformEdge, WorkflowDependency, WorkflowMetadata, WorkflowParameter, WorkflowPort } from '../editor/types';
import type { QueueItem, WorkflowBinding } from '../browser/types';
import type {
  BatchBitDepth,
  BatchColorSpace,
  BatchCheckpointPolicy,
  BatchCompression,
  BatchCollisionPolicy,
  BatchFormat,
  BatchJob,
  BatchRecipe,
  BatchSharpening,
  BatchSubset,
  BatchWorkflowDefinition,
} from './types';

export interface BatchWorkflowContext {
  binding: { id: string; version: string; hash: string };
  revision: number;
  nodes: EditorNode[];
  edges: PlatformEdge[];
  parameters: WorkflowParameter[];
  inputs: WorkflowPort[];
  outputs: WorkflowPort[];
  metadata: WorkflowMetadata;
  nodePackDependencies: WorkflowDependency[];
  subgraphDependencies: WorkflowDependency[];
}

export function defaultBatchRecipe(destination = ''): BatchRecipe {
  return {
    format: 'jpeg',
    resolution: 'original',
    bitDepth: 'eight',
    colorSpace: 'srgb',
    iccProfile: null,
    ocioTransform: null,
    metadataPolicy: 'preserve',
    sharpening: 'None',
    quality: 92,
    compression: 'default',
    destination,
    filenameTemplate: '{stem}-{index}',
    collisionPolicy: 'suffix',
  };
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

export function selectQueueSubset(items: QueueItem[], subset: BatchSubset): QueueItem[] {
  switch (subset.kind) {
    case 'current-preview': {
      const item = items.find((candidate) => candidate.id === subset.itemId);
      if (!item) throw new Error(`queue item '${subset.itemId}' does not exist`);
      return [item];
    }
    case 'test-set':
      return items.filter((item) => item.testSet);
    case 'first-n':
      return items.slice(0, Math.max(0, Math.trunc(subset.count)));
    case 'selected': {
      const byId = new Map(items.map((item) => [item.id, item]));
      return subset.itemIds.map((id) => {
        const item = byId.get(id);
        if (!item) throw new Error(`queue item '${id}' does not exist`);
        return item;
      });
    }
    case 'all':
      return [...items];
  }
}

function definitionFromContext(context: BatchWorkflowContext): BatchWorkflowDefinition {
  return {
    identity: { id: context.binding.id, version: context.binding.version },
    graph: {
      nodes: context.nodes.map((node) => ({
        id: node.id,
        typeId: node.typeId,
        descriptor: clone(node.descriptor),
        parameters: clone(node.parameters),
        exposedParameters: [...(node.exposedParameters ?? [])],
      })),
      edges: clone(context.edges),
      revision: context.revision,
    },
    parameters: clone(context.parameters),
    inputs: clone(context.inputs),
    outputs: clone(context.outputs),
    subgraphDependencies: clone(context.subgraphDependencies),
    nodePackDependencies: clone(context.nodePackDependencies),
    metadata: clone(context.metadata),
    nestedSubgraphs: {},
  };
}

function workflowOverrides(items: QueueItem[]): Record<string, ParameterValue> {
  const values: Record<string, ParameterValue> = {};
  for (const item of items) {
    for (const [key, value] of Object.entries(item.overrides)) {
      if (!(key in values)) values[key] = value;
    }
  }
  return values;
}

export function buildBatchJob(
  queueItems: QueueItem[],
  context: BatchWorkflowContext,
  subset: BatchSubset,
  recipe: BatchRecipe,
  id: string,
  checkpointPolicy: BatchCheckpointPolicy = 'after_each_item',
): BatchJob {
  if (!id.trim()) throw new Error('batch job id cannot be empty');
  const selected = selectQueueSubset(queueItems, subset);
  const items = selected.map((item) => ({
    id: item.id,
    sourcePath: item.path,
    displayName: item.name,
    overrides: clone(item.overrides),
    testSet: item.testSet,
    state: 'waiting' as const,
    attempts: 0,
    failure: null,
    outputs: [],
  }));
  return {
    schemaVersion: 1,
    id,
    workflow: {
      definition: definitionFromContext(context),
      revision: context.revision,
      hash: context.binding.hash,
    },
    dependencies: {
      nodePacks: clone(context.nodePackDependencies),
      subgraphs: clone(context.subgraphDependencies),
      plugins: {},
      externalProviders: {},
    },
    overrides: workflowOverrides(selected),
    recipes: [clone(recipe)],
    checkpointPolicy,
    items,
    state: 'draft',
  };
}

export function recipeFormatSupports(format: BatchFormat, bitDepth: BatchBitDepth): boolean {
  return format !== 'jpeg' || bitDepth === 'eight';
}

export function recipeColorSpaceLabel(colorSpace: BatchColorSpace): string {
  if (typeof colorSpace === 'string') return colorSpace;
  return colorSpace.named;
}

export function recipeSharpeningLabel(sharpening: BatchSharpening): string {
  return sharpening === 'None' ? 'None' : 'Unsharp mask';
}

export function recipeCollisionLabel(policy: BatchCollisionPolicy): string {
  return policy === 'error' ? 'Error' : policy === 'skip' ? 'Skip' : policy === 'overwrite' ? 'Overwrite' : 'Suffix';
}

export function recipeCompressionLabel(compression: BatchCompression): string {
  return compression[0].toUpperCase() + compression.slice(1);
}

export function recipeBitDepthLabel(bitDepth: BatchBitDepth): string {
  return bitDepth === 'float32' ? '32-bit float' : bitDepth === 'sixteen' ? '16-bit' : '8-bit';
}

export function recipeFormatLabel(format: BatchFormat): string {
  return format === 'open_exr' ? 'OpenEXR' : format.toUpperCase();
}

export function recipeWorkflowBinding(context: BatchWorkflowContext): WorkflowBinding {
  return { ...context.binding };
}
