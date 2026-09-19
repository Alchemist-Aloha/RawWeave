import { describe, expect, it } from 'vitest';
import { CheckpointController } from './controller';
import type { CheckpointPlatform, CheckpointStatus } from './types';

const status = (state: CheckpointStatus['state']): CheckpointStatus => ({
  nodeId: 'manual-1',
  outputPort: 'image',
  state,
  availability: state === 'fresh' ? 'fresh' : state === 'stale' ? 'stale' : 'missing',
  currentDependencyHash: state === 'stale' ? 'current-hash' : 'committed-hash',
  committedDependencyHash: state === 'ungenerated' ? null : 'committed-hash',
  committedArtifactId: state === 'ungenerated' ? null : 'artifact-1',
  generation: state === 'ungenerated' ? null : { generationRevision: 3, generatedAt: '2026-09-19T12:00:00Z' },
  provenance: state === 'ungenerated' ? null : { dependencyHash: 'committed-hash', nodeVersion: 1, upstreamHashes: {} },
  failure: state === 'failed' ? 'provider unavailable' : null,
  progress: state === 'generating' ? 42 : null,
});

describe('CheckpointController', () => {
  it('keeps the committed artifact usable while a checkpoint is stale', async () => {
    let current = status('stale');
    const platform: CheckpointPlatform = {
      async list() { return [current]; },
      async status() { return current; },
      async generate() { current = status('fresh'); return current; },
      async cancel() { current = status('cancelled'); return current; },
    };
    const controller = new CheckpointController(platform);

    await controller.refresh('manual-1');

    expect(controller.state.status?.state).toBe('stale');
    expect(controller.state.status?.committedArtifactId).toBe('artifact-1');
    expect(controller.state.status?.canUseCommitted).toBe(true);
  });

  it('surfaces generate progress and failures through explicit actions', async () => {
    let current = status('ungenerated');
    const platform: CheckpointPlatform = {
      async list() { return [current]; },
      async status() { return current; },
      async generate() { current = status('failed'); return current; },
      async cancel() { current = status('cancelled'); return current; },
    };
    const controller = new CheckpointController(platform);

    await controller.generate('manual-1', 'image');

    expect(controller.state.operation).toBe(null);
    expect(controller.state.status?.state).toBe('failed');
    expect(controller.state.status?.failure).toBe('provider unavailable');
    expect(controller.state.error).toBe('provider unavailable');
  });
});
