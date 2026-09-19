import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { MaskPainter } from './MaskPainter';
import type { EditorNode, ParameterValue } from '../editor/types';

const node: EditorNode = {
  id: 'painted',
  typeId: 'core.mask-painted',
  parameters: { size: 20, hardness: 1, opacity: 1, mode: 'add', points: '' },
  exposedParameters: [],
  position: { x: 0, y: 0 },
  descriptor: {
    typeId: 'core.mask-painted',
    name: 'Painted Mask',
    version: 1,
    inputs: [],
    outputs: [{ id: 'mask', name: 'Mask', dataType: 'core.Mask', required: false }],
    parameters: [],
  },
};

let root: Root | null = null;
let container: HTMLDivElement | null = null;

afterEach(() => {
  if (root) {
    act(() => root?.unmount());
    root = null;
  }
  container?.remove();
  container = null;
});

describe('MaskPainter', () => {
  it('shows brush controls and writes a zoom-aware stroke to Painted Mask parameters', async () => {
    const image = document.createElement('img');
    vi.spyOn(image, 'getBoundingClientRect').mockReturnValue({
      left: 100,
      top: 50,
      width: 200,
      height: 100,
      right: 300,
      bottom: 150,
      x: 100,
      y: 50,
      toJSON: () => ({}),
    });
    const imageRef = { current: image };
    const writes: Array<[string, ParameterValue]> = [];
    container = document.createElement('div');
    document.body.append(container);
    root = createRoot(container);

    await act(async () => {
      root?.render(
        <MaskPainter
          imageOrigin={{ x: 10, y: 20 }}
          imageRef={imageRef}
          imageRegion={{ x: 0, y: 0, width: 400, height: 200 }}
          node={node}
          onChange={(parameter, value) => writes.push([parameter, value])}
        />,
      );
    });

    expect(container.querySelector('[aria-label="Brush size"]')).not.toBeNull();
    expect(container.querySelector('[aria-label="Brush hardness"]')).not.toBeNull();
    expect(container.querySelector('[aria-label="Brush opacity"]')).not.toBeNull();

    await act(async () => {
      container?.querySelector<HTMLButtonElement>('[aria-label="Subtract mask"]')?.click();
    });
    expect(writes).toContainEqual(['mode', 'subtract']);

    const surface = container.querySelector<HTMLDivElement>('[aria-label="Paint mask"]');
    expect(surface).not.toBeNull();
    await act(async () => {
      surface?.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, clientX: 150, clientY: 100, button: 0 }));
      surface?.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, clientX: 150, clientY: 100, button: 0 }));
    });

    expect(writes).toContainEqual(['points', '110,120']);
  });

  it('exposes undo and redo for committed strokes', async () => {
    const imageRef = { current: null };
    const writes: Array<[string, ParameterValue]> = [];
    container = document.createElement('div');
    document.body.append(container);
    root = createRoot(container);
    await act(async () => {
      root?.render(
        <MaskPainter
          imageOrigin={{ x: 0, y: 0 }}
          imageRef={imageRef}
          imageRegion={{ x: 0, y: 0, width: 100, height: 100 }}
          node={node}
          onChange={(parameter, value) => writes.push([parameter, value])}
        />,
      );
    });

    const undo = container.querySelector<HTMLButtonElement>('[aria-label="Undo mask stroke"]');
    const redo = container.querySelector<HTMLButtonElement>('[aria-label="Redo mask stroke"]');
    expect(undo?.disabled).toBe(true);
    expect(redo?.disabled).toBe(true);
  });
});
