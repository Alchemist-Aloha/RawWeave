import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { createTauriCheckpointPlatform } from './checkpoint';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const mockedInvoke = vi.mocked(invoke);

describe('Tauri checkpoint platform', () => {
  beforeEach(() => vi.resetAllMocks());

  it('maps backend current state to a fresh UI status and preserves provenance', async () => {
    mockedInvoke.mockResolvedValue({
      node_id: 'manual-1', output_port: 'image', state: 'current', availability: 'fresh',
      current_dependency_hash: 'hash-a', committed_dependency_hash: 'hash-a',
      committed_artifact_id: 'sha256:artifact',
      generation: { generation_revision: 7, generated_at: '2026-09-19T12:00:00Z' },
      provenance: { dependency_hash: 'hash-a', upstream_hashes: { input: 'upstream-a' }, node_version: 2 },
      failure: null, progress: null,
    });
    const platform = createTauriCheckpointPlatform();

    const result = await platform.status('manual-1');

    expect(result.state).toBe('fresh');
    expect(result.canUseCommitted).toBe(true);
    expect(result.provenance?.upstreamHashes).toEqual({ input: 'upstream-a' });
    expect(mockedInvoke).toHaveBeenCalledWith('checkpoint_status', { nodeId: 'manual-1' });
  });

  it('uses explicit generate and cancel commands', async () => {
    mockedInvoke
      .mockResolvedValueOnce({ node_id: 'manual-1', output_port: 'image', state: 'generating', availability: 'missing', progress: 10 })
      .mockResolvedValueOnce({ node_id: 'manual-1', output_port: 'image', state: 'cancelled', availability: 'missing', progress: null });
    const platform = createTauriCheckpointPlatform();

    await platform.generate('manual-1', 'image');
    await platform.cancel('manual-1');

    expect(mockedInvoke).toHaveBeenNthCalledWith(1, 'generate_checkpoint', { nodeId: 'manual-1', outputPort: 'image' });
    expect(mockedInvoke).toHaveBeenNthCalledWith(2, 'cancel_checkpoint', { nodeId: 'manual-1' });
  });
});
