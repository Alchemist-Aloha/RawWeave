import { describe, expect, it } from 'vitest';
import { createMemoryPlatform } from './editor';

describe('memory platform workflow hashes', () => {
  it('uses the canonical SHA-256 hash for an empty workflow', async () => {
    const platform = createMemoryPlatform();

    await expect(platform.workflowHash()).resolves.toBe(
      '6e2d252445f797b5e92c1837d5f381a02b3c0cdf8ce51a2a211ec12b21d11e16',
    );
  });

  it('does not make graph insertion order part of the workflow hash', async () => {
    const first = createMemoryPlatform();
    await first.addNode('output', 'core.output');
    await first.addNode('exposure', 'core.exposure');
    await first.addNode('input', 'core.image-input');
    await first.connect('input', 'image', 'exposure', 'image');
    await first.connect('exposure', 'image', 'output', 'image');

    const second = createMemoryPlatform();
    await second.addNode('input', 'core.image-input');
    await second.addNode('exposure', 'core.exposure');
    await second.addNode('output', 'core.output');
    await second.connect('exposure', 'image', 'output', 'image');
    await second.connect('input', 'image', 'exposure', 'image');

    await expect(second.workflowHash()).resolves.toBe(await first.workflowHash());
  });
});