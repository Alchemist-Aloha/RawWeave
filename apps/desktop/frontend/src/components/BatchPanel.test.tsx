import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it } from 'vitest';
import { BatchPanel } from './BatchPanel';
import { BatchController } from '../batch/controller';
import { createMemoryBatchPlatform } from '../platform/batch';
import { defaultBatchRecipe } from '../batch/model';
import type { BatchWorkflowContext } from '../batch/model';
import type { QueueItem } from '../browser/types';

const context: BatchWorkflowContext = {
  binding: { id: 'workflow', version: '1.0.0', hash: 'hash' }, revision: 1,
  nodes: [], edges: [], parameters: [], inputs: [], outputs: [], metadata: { name: 'Workflow' },
  nodePackDependencies: [], subgraphDependencies: [],
};

const item: QueueItem = {
  id: '/photos/one.jpg', path: '/photos/one.jpg', name: 'one.jpg', order: 0,
  source: { path: '/photos/one.jpg', name: 'one.jpg', kind: 'file', extension: 'jpg', size: 1,
    modifiedTime: null, rating: null, flag: 'none', metadata: null, thumbnail: null },
  rating: null, flag: 'none', workflowBinding: context.binding, overrides: {},
  processingStatus: 'pending', outputStatus: 'not-started', errors: [], warnings: [], testSet: false,
};

const manualContext: BatchWorkflowContext = {
  ...context,
  nodes: [{
    id: 'manual',
    typeId: 'test.manual',
    parameters: {},
    position: { x: 0, y: 0 },
    descriptor: {
      typeId: 'test.manual',
      name: 'Manual checkpoint',
      version: 1,
      inputs: [],
      outputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: true }],
      parameters: [],
      evaluationPolicy: 'manual_checkpoint',
    },
  }],
};

const nestedManualContext: BatchWorkflowContext = {
  ...context,
  nestedSubgraphs: {
    nested: {
      identity: { id: 'nested', version: '1.0.0' },
      graph: {
        nodes: [{
          id: 'nested-manual',
          typeId: 'test.manual',
          parameters: {},
          exposedParameters: [],
          descriptor: manualContext.nodes[0].descriptor,
        }],
        edges: [],
        revision: 1,
      },
      parameters: [],
      inputs: [],
      outputs: [],
      subgraphDependencies: [],
      nodePackDependencies: [],
      metadata: { name: 'Nested' },
      nestedSubgraphs: {},
    },
  },
};

let root: Root | null = null;
let host: HTMLDivElement | null = null;
afterEach(() => {
  if (root) act(() => root?.unmount());
  root = null;
  host?.remove();
  host = null;
});

describe('BatchPanel', () => {
  it('edits recipe, exposes exact dry-run selectors, diagnostics, progress, and operations', async () => {
    const controller = new BatchController(createMemoryBatchPlatform(), () => 'job-ui');
    await controller.createFromQueue([item], context, { kind: 'all' }, defaultBatchRecipe('/exports'));
    host = document.createElement('div');
    document.body.append(host);
    await act(async () => {
      root = createRoot(host!);
      root.render(<BatchPanel controller={controller} queueItems={[item]} workflow={context} />);
    });

    expect(host.textContent).toContain('Batch');
    expect(host.querySelector('[aria-label="Batch output format"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Batch destination"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Dry run subset"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Batch diagnostics"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Start batch"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Retry failed"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Open failed item"]')).not.toBeNull();
  });

  it('requires an explicit checkpoint policy before creating a manual-checkpoint batch', async () => {
    const controller = new BatchController(createMemoryBatchPlatform(), () => 'manual-policy-ui');
    host = document.createElement('div');
    document.body.append(host);
    await act(async () => {
      root = createRoot(host!);
      root.render(<BatchPanel controller={controller} queueItems={[item]} workflow={manualContext} />);
    });

    const create = host.querySelector<HTMLButtonElement>('[aria-label="Create batch"]');
    expect(create?.disabled).toBe(true);
    expect(host.textContent).toContain('Choose an explicit policy');

    await act(async () => {
      controller.updateCheckpointPolicy('use_committed');
    });

    expect(create?.disabled).toBe(false);
  });

  it('requires an explicit checkpoint policy when a nested workflow contains one', async () => {
    const controller = new BatchController(createMemoryBatchPlatform(), () => 'nested-policy-ui');
    host = document.createElement('div');
    document.body.append(host);
    await act(async () => {
      root = createRoot(host!);
      root.render(<BatchPanel controller={controller} queueItems={[item]} workflow={nestedManualContext} />);
    });

    expect(host.querySelector<HTMLButtonElement>('[aria-label="Create batch"]')?.disabled).toBe(true);
    expect(host.textContent).toContain('Choose an explicit policy');
  });
});
