import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { createMemoryBatchPlatform, createTauriBatchPlatform } from '../platform/batch';
import { defaultBatchRecipe } from './model';
import type { BatchJob } from './types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const mockedInvoke = vi.mocked(invoke);

function job(): BatchJob {
  return {
    schemaVersion: 1,
    id: 'job-1',
    workflow: { revision: 4, hash: 'hash', definition: {
      identity: { id: 'workflow', version: '1.0.0' },
      graph: { nodes: {}, edges: [], revision: 4 },
      parameters: {}, inputs: [], outputs: [], subgraphDependencies: [], nodePackDependencies: [],
      metadata: { name: 'Workflow' }, nestedSubgraphs: {},
    } },
    dependencies: { nodePacks: [], subgraphs: [], plugins: {}, externalProviders: {} },
    overrides: {},
    recipes: [defaultBatchRecipe('/exports')],
    checkpointPolicy: 'after_each_item',
    items: [{
      id: 'item-1', sourcePath: '/photos/one.jpg', displayName: 'one.jpg', overrides: {}, testSet: true,
      state: 'waiting', attempts: 0, failure: null, outputs: [],
    }],
    state: 'draft',
  };
}

describe('Tauri batch platform', () => {
  beforeEach(() => vi.resetAllMocks());

  it('maps create, preflight, dry-run, and lifecycle command payloads', async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === 'batch_dry_run') return {
        workflowRevision: 4, workflowHash: 'hash', itemIds: ['item-1'], recipes: [],
      };
      if (command === 'batch_preflight') return { diagnostics: [] };
      if (command === 'create_batch_job') return {
        schema_version: 1, id: 'job-1', workflow: { revision: 4, hash: 'hash', definition: job().workflow.definition },
        dependencies: { node_packs: [], subgraphs: [], plugins: {}, external_providers: {} }, overrides: {},
        recipes: [{ format: 'jpeg', destination: '/exports', filename_template: '{stem}-{index}', quality: 92,
          resolution: 'original', bit_depth: 'eight', color_space: 'srgb', icc_profile: null, ocio_transform: null,
          metadata_policy: 'preserve', sharpening: 'None', compression: 'default', collision_policy: 'suffix' }],
        checkpoint_policy: 'after_each_item', items: [{ id: 'item-1', source_path: '/photos/one.jpg', display_name: 'one.jpg',
          overrides: {}, test_set: true, state: 'waiting', attempts: 0, failure: null, outputs: [] }], state: 'draft',
      };
      return undefined;
    });
    const platform = createTauriBatchPlatform();

    await platform.createJob(job(), { statePath: '/state/job.json', maxWorkers: 2 });
    await platform.preflight('job-1');
    await platform.dryRun('job-1', { kind: 'selected', itemIds: ['item-1'] });
    await platform.retrySelected('job-1', ['item-1']);

    expect(mockedInvoke).toHaveBeenNthCalledWith(1, 'create_batch_job', expect.objectContaining({
      request: expect.objectContaining({ statePath: '/state/job.json', maxWorkers: 2 }),
    }));
    expect(mockedInvoke).toHaveBeenNthCalledWith(2, 'batch_preflight', { jobId: 'job-1', options: undefined });
    expect(mockedInvoke).toHaveBeenNthCalledWith(3, 'batch_dry_run', {
      jobId: 'job-1', subset: { selected: ['item-1'] },
    });
    expect(mockedInvoke).toHaveBeenNthCalledWith(4, 'retry_selected_batch', {
      jobId: 'job-1', itemIds: ['item-1'],
    });
  });

  it('keeps memory jobs isolated and applies item operations', async () => {
    const platform = createMemoryBatchPlatform();
    await platform.createJob(job());
    await platform.skip('job-1', ['item-1']);
    const snapshot = await platform.snapshot('job-1');
    expect(snapshot.items[0].state).toBe('skipped');
    expect((await platform.dryRun('job-1', { kind: 'test-set' })).itemIds).toEqual(['item-1']);
  });
});
