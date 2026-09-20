import { describe, expect, it } from 'vitest';
import { targetsFor } from './Viewer';
import type { EditorNode } from '../editor/types';

function node(id: string, outputs: Array<{ id: string; name: string; dataType: string }>): EditorNode {
  return {
    id,
    typeId: `test.${id}`,
    parameters: {},
    position: { x: 0, y: 0 },
    descriptor: {
      typeId: `test.${id}`,
      name: id,
      version: 1,
      inputs: [],
      outputs: outputs.map((output) => ({ ...output, required: false })),
      parameters: [],
    },
  };
}

describe('Viewer', () => {
  it('offers ordinary and spatial graph values as preview targets', () => {
    const targets = targetsFor([
      node('segmentation', [
        { id: 'label_map', name: 'Label Map', dataType: 'core.LabelMap' },
        { id: 'confidence', name: 'Confidence', dataType: 'core.ConfidenceMap' },
        { id: 'regions', name: 'Regions', dataType: 'core.RegionSet' },
        { id: 'masks', name: 'Masks', dataType: 'core.MaskSet' },
        { id: 'depth', name: 'Depth', dataType: 'core.DepthMap' },
      ]),
    ]);

    expect(targets.map((target) => target.dataType)).toEqual([
      'core.LabelMap',
      'core.ConfidenceMap',
      'core.RegionSet',
      'core.MaskSet',
      'core.DepthMap',
    ]);
  });
});
