import { afterEach, describe, expect, it, vi } from 'vitest';
import { BatchController } from './controller';
import { buildBatchJob, defaultBatchRecipe } from './model';
import { createMemoryBatchPlatform } from '../platform/batch';
import type { BatchJob, BatchPlatform } from './types';
import type { EditorNode } from '../editor/types';
import type { BrowserEntry, QueueItem } from '../browser/types';

function queueItem(path: string, testSet = false): QueueItem {
  const source: BrowserEntry = {
    path, name: path.split('/').at(-1)!, kind: 'file', extension: 'jpg', size: 10,
    modifiedTime: null, rating: null, flag: 'none', metadata: null, thumbnail: null,
  };
  return {
    id: path, path, name: source.name, source, rating: null, flag: 'none', order: 0,
    workflowBinding: { id: 'workflow', version: '1.0.0', hash: 'workflow-hash' }, overrides: {},
    processingStatus: 'pending', outputStatus: 'not-started', errors: [], warnings: [], testSet,
  };
}

const node: EditorNode = {
  id: 'input', typeId: 'core.image-input', parameters: {}, exposedParameters: [],
  position: { x: 0, y: 0 },
  descriptor: { typeId: 'core.image-input', name: 'Image Input', version: 1, inputs: [],
    outputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: false }], parameters: [], capabilities: ['CPU'] },
};

const context = {
  binding: { id: 'workflow', version: '1.0.0', hash: 'workflow-hash' },
  revision: 9,
  nodes: [node], edges: [], parameters: [], inputs: [], outputs: [],
  metadata: { name: 'Workflow' }, nodePackDependencies: [], subgraphDependencies: [],
};

describe('batch controller', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('creates a pinned job from the exact test-set queue subset and recipe', () => {
    const job = buildBatchJob([queueItem('/photos/one.jpg'), queueItem('/photos/two.jpg', true)], context, {
      kind: 'test-set',
    }, defaultBatchRecipe('/exports'), 'job-1');

    expect(job.id).toBe('job-1');
    expect(job.workflow.revision).toBe(9);
    expect(job.workflow.hash).toBe('workflow-hash');
    expect(job.items.map((item) => item.id)).toEqual(['/photos/two.jpg']);
    expect(job.recipes[0].destination).toBe('/exports');
  });

  it('pins an explicit checkpoint policy for manual-checkpoint workflows', async () => {
    const controller = new BatchController(createMemoryBatchPlatform(), () => 'job-policy');
    controller.updateCheckpointPolicy('fail_if_stale');

    await controller.createFromQueue(
      [queueItem('/photos/one.jpg')],
      context,
      { kind: 'all' },
      defaultBatchRecipe('/exports'),
    );

    expect(controller.state.job?.checkpointPolicy).toBe('fail_if_stale');
  });

  it('polls a running job until it settles so the panel can show progress', async () => {
    vi.useFakeTimers();
    try {
      const job = buildBatchJob([queueItem('/photos/one.jpg')], context, { kind: 'all' }, defaultBatchRecipe('/exports'), 'job-3');
      const running: BatchJob = { ...job, state: 'running' };
      let snapshots = 0;
      const platform: BatchPlatform = {
        ...createMemoryBatchPlatform(),
        async start() { return running; },
        async snapshot() {
          snapshots += 1;
          return snapshots >= 2
            ? { ...job, state: 'completed', items: [{ ...job.items[0], state: 'completed' }] }
            : running;
        },
      };
      const controller = new BatchController(platform, () => 'job-3');
      await controller.createFromQueue([queueItem('/photos/one.jpg')], context, { kind: 'all' }, defaultBatchRecipe('/exports'));
      await controller.start();
      expect(controller.state.job?.state).toBe('running');

      await vi.advanceTimersByTimeAsync(1500);
      expect(snapshots).toBeGreaterThanOrEqual(2);
      expect(controller.state.job?.state).toBe('completed');
      expect(controller.state.job?.items[0].state).toBe('completed');

      // A settled job stops polling.
      const settled = snapshots;
      await vi.advanceTimersByTimeAsync(2000);
      expect(snapshots).toBe(settled);
      controller.dispose();
    } finally {
      vi.useRealTimers();
    }
  });

  it('runs preflight, dry run, start, pause, resume, and cancel without blocking state consumers', async () => {
    const controller = new BatchController(createMemoryBatchPlatform(), () => 'job-2');
    await controller.createFromQueue([queueItem('/photos/one.jpg')], context, { kind: 'all' }, defaultBatchRecipe('/exports'));
    await controller.preflight();
    expect(controller.state.diagnostics.length).toBeGreaterThan(0);
    await controller.dryRun({ kind: 'current-preview', itemId: '/photos/one.jpg' });
    expect(controller.state.dryRun?.itemIds).toEqual(['/photos/one.jpg']);
    await controller.start();
    expect(controller.state.job?.items[0].state).toBe('completed');
    await controller.retrySelected(['/photos/one.jpg']);
    await controller.pause();
    await controller.resume();
    await controller.cancel();
    expect(controller.state.job?.id).toBe('job-2');
  });
});
