import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { CheckpointPanel, type CheckpointPreviewActions } from './CheckpointPanel';
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

  it('opens input, committed, and comparison previews through explicit actions', async () => {
    const previews: CheckpointPreviewActions = {
      input: { onClick: () => undefined },
      generated: { onClick: () => undefined },
      difference: { onClick: () => undefined },
    };
    host = document.createElement('div');
    document.body.append(host);
    await act(async () => {
      root = createRoot(host!);
      root.render(
        <CheckpointPanel
          previewActions={previews}
          status={stale}
          loading={false}
          onGenerate={() => undefined}
          onCancel={() => undefined}
        />,
      );
    });

    expect(host.querySelector('[aria-label="Preview checkpoint inputs"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Preview committed checkpoint"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Compare checkpoint previews"]')).not.toBeNull();
  });

  it('lets spatial checkpoints choose a label-map output before generation', async () => {
    const onGenerate = vi.fn();
    host = document.createElement('div');
    document.body.append(host);
    await act(async () => {
      root = createRoot(host!);
      root.render(
        <CheckpointPanel
          outputPorts={[
            { id: 'label_map', name: 'Label Map', dataType: 'core.LabelMap' },
            { id: 'confidence', name: 'Confidence', dataType: 'core.ConfidenceMap' },
          ]}
          status={{ ...stale, outputPort: 'label_map' }}
          loading={false}
          onGenerate={onGenerate}
          onCancel={() => undefined}
        />,
      );
    });

    const output = host.querySelector<HTMLSelectElement>('[aria-label="Checkpoint output"]');
    expect(output).not.toBeNull();
    expect(output?.value).toBe('label_map');
    expect(output?.textContent).toContain('core.LabelMap');
    await act(async () => {
      output!.value = 'confidence';
      output!.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => {
      host?.querySelector<HTMLButtonElement>('[aria-label="Regenerate checkpoint"]')?.click();
    });
    expect(onGenerate).toHaveBeenCalledWith('confidence');
  });
});
