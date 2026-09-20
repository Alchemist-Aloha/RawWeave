import { describe, expect, it } from 'vitest';
import { CheckpointController } from './controller';
import type { CheckpointPlatform, CheckpointProgressEvent, CheckpointStatus } from './types';

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

  it('ignores progress for a different output port on the same node', async () => {
    let progressListener: ((event: CheckpointProgressEvent) => void) | undefined;
    const platform: CheckpointPlatform = {
      async list() { return [status('generating')]; },
      async status() { return status('generating'); },
      async generate() { return status('fresh'); },
      async cancel() { return status('cancelled'); },
      subscribeProgress(listener) {
        progressListener = listener;
        return () => undefined;
      },
    };
    const controller = new CheckpointController(platform);
    await controller.refresh('manual-1');
    expect(controller.state.status?.progress).toBe(42);

    progressListener?.({
      nodeId: 'manual-1',
      outputPort: 'mask',
      progress: 0.9,
      phase: 'committing',
    });

    expect(controller.state.status?.outputPort).toBe('image');
    expect(controller.state.status?.progress).toBe(42);
  });

  it('keeps a completed generation stale when the authoritative event says inputs changed', async () => {
    let progressListener: ((event: CheckpointProgressEvent) => void) | undefined;
    let resolveGeneration: ((value: CheckpointStatus) => void) | undefined;
    const stale = status('stale');
    const platform: CheckpointPlatform = {
      async list() { return [stale]; },
      async status() { return stale; },
      async generate() {
        return new Promise<CheckpointStatus>((resolve) => { resolveGeneration = resolve; });
      },
      async cancel() { return status('cancelled'); },
      subscribeProgress(listener) {
        progressListener = listener;
        return () => undefined;
      },
    };
    const controller = new CheckpointController(platform);
    await controller.refresh('manual-1');
    const pending = controller.generate('manual-1', 'image');

    progressListener?.({
      nodeId: 'manual-1',
      outputPort: 'image',
      progress: 1,
      phase: 'complete',
      message: 'checkpoint inputs changed while generating',
      ...({ state: 'stale', availability: 'stale' } as Partial<CheckpointProgressEvent>),
    });

    expect(controller.state.status?.state).toBe('stale');
    expect(controller.state.status?.availability).toBe('stale');

    resolveGeneration?.(stale);
    await pending;
  });

  it('can dismiss an operation error after it has been rendered', async () => {
    const platform: CheckpointPlatform = {
      async list() { throw new Error('checkpoint service unavailable'); },
      async status() { return status('ungenerated'); },
      async generate() { return status('fresh'); },
      async cancel() { return status('cancelled'); },
    };
    const controller = new CheckpointController(platform);

    await expect(controller.refresh()).rejects.toThrow('checkpoint service unavailable');
    expect(controller.state.error).toBe('checkpoint service unavailable');

    controller.clearError();

    expect(controller.state.error).toBeNull();
  });
});
