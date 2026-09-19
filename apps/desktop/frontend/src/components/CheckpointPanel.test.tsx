import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it } from 'vitest';
import { CheckpointPanel } from './CheckpointPanel';
import type { CheckpointStatus } from '../checkpoint/types';

const stale: CheckpointStatus = {
  nodeId: 'manual-1', outputPort: 'image', state: 'stale', availability: 'stale',
  currentDependencyHash: 'current-hash', committedDependencyHash: 'committed-hash',
  committedArtifactId: 'artifact-1', generation: { generationRevision: 4, generatedAt: '2026-09-19T12:00:00Z' },
  provenance: { dependencyHash: 'committed-hash', nodeVersion: 1, upstreamHashes: { input: 'upstream-hash' } },
  failure: null, progress: null, canUseCommitted: true,
};

let root: Root | null = null;
let host: HTMLDivElement | null = null;
afterEach(() => {
  if (root) act(() => root?.unmount());
  root = null;
  host?.remove();
  host = null;
});

describe('CheckpointPanel', () => {
  it('shows stale provenance, a usable committed result, and an explicit regenerate action', async () => {
    host = document.createElement('div');
    document.body.append(host);
    await act(async () => {
      root = createRoot(host!);
      root.render(<CheckpointPanel status={stale} loading={false} onGenerate={() => undefined} onCancel={() => undefined} />);
    });

    expect(host.textContent).toContain('Stale');
    expect(host.textContent).toContain('Committed result remains usable');
    expect(host.textContent).toContain('committed-hash');
    expect(host.querySelector('[aria-label="Regenerate checkpoint"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Checkpoint provenance"]')).not.toBeNull();
  });
});
