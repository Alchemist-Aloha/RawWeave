import { describe, expect, it } from 'vitest';
import { createMemoryPlatform } from './editor';

describe('memory platform workflow hashes', () => {
  it('connects typed scene processing and conversion without changing image ports', async () => {
    const platform = createMemoryPlatform();
    const descriptors = await platform.nodeDescriptors();
    for (const id of ['core.exposure', 'core.local-exposure', 'core.blur', 'core.resize', 'core.color-matrix', 'core.output']) {
      const descriptor = descriptors.find((node) => node.typeId === id);
      expect(descriptor?.inputs.find((port) => port.id === 'scene')?.dataType).toBe('color.SceneLinearRGB');
      expect(descriptor?.outputs.find((port) => port.id === 'image')?.dataType).toBe('core.Image');
    }
    await platform.addNode('camera', 'raw.camera-transform');
    await platform.addNode('exposure', 'core.exposure');
    await platform.addNode('convert', 'core.scene-linear-to-image');
    await platform.addNode('output', 'core.output');
    await platform.connect('camera', 'scene', 'exposure', 'scene');
    await platform.connect('exposure', 'scene', 'convert', 'scene');
    await platform.connect('convert', 'image', 'output', 'image');
    await expect(platform.connect('camera', 'scene', 'output', 'image')).rejects.toThrow();
  });
  it.each([
    ['core.mask-color-qualifier', 'target_r', 1],
    ['pro.color-zones', 'width', 0.2],
    ['pro.split-toning', 'shadow_hue', 0.6],
  ] as const)('registers %s color controls for browser UI regression coverage', async (typeId, parameterId, defaultValue) => {
    const platform = createMemoryPlatform();
    await platform.addNode('color', typeId);
    const descriptors = await platform.nodeDescriptors();
    expect(descriptors.find((descriptor) => descriptor.typeId === typeId)?.parameters.find((parameter) => parameter.id === parameterId)?.default).toBe(defaultValue);
  });
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