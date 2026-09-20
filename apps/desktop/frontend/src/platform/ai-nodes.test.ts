import { describe, expect, it } from 'vitest';
import { aiNodeDescriptors } from './ai-nodes';

describe('AI checkpoint node descriptors', () => {
  it('exposes the four Step 11 operations as manual checkpoints', () => {
    expect(aiNodeDescriptors.map((descriptor) => descriptor.typeId)).toEqual([
      'ai.img2img',
      'ai.inpaint',
      'ai.generative-fill',
      'ai.upscale',
    ]);
    expect(aiNodeDescriptors).toHaveLength(4);
    for (const descriptor of aiNodeDescriptors) {
      expect(descriptor.evaluationPolicy).toBe('manual_checkpoint');
      expect(descriptor.outputs).toEqual([
        { id: 'image', name: 'Image', dataType: 'core.Image', required: false },
      ]);
      expect(descriptor.parameters.map((parameter) => parameter.id)).toEqual(
        expect.arrayContaining(['provider_id', 'workflow_id', 'prompt']),
      );
    }
  });

  it('requires a mask for inpaint and generative fill without making it required for img2img', () => {
    expect(aiNodeDescriptors.find((descriptor) => descriptor.typeId === 'ai.img2img')?.inputs).toEqual([
      { id: 'image', name: 'Image', dataType: 'core.Image', required: true },
    ]);
    for (const typeId of ['ai.inpaint', 'ai.generative-fill']) {
      expect(aiNodeDescriptors.find((descriptor) => descriptor.typeId === typeId)?.inputs).toEqual([
        { id: 'image', name: 'Image', dataType: 'core.Image', required: true },
        { id: 'mask', name: 'Mask', dataType: 'core.Mask', required: true },
      ]);
    }
  });
});
