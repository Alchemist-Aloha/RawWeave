import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import { ImageNodeControls } from './ImageNodeControls';
import { ViewerController } from '../viewer/controller';
import type { EditorNode } from '../editor/types';

it('uses evaluated input dimensions for resize presets and hides stale dimensions', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const root = createRoot(host);
  const controller = new ViewerController({
    requestPreview: async (request) => ({ ...request, url: 'blob:input', width: 800, height: 600, fullWidth: 800, fullHeight: 600, mimeType: 'image/png' }),
    cancelPreview: async () => undefined, releasePreview: async () => undefined,
  });
  controller.setSourceDimensions({ width: 4000, height: 3000 });
  controller.setViewport('A', { width: 800, height: 600 });
  const target = { nodeId: 'upstream', nodeName: 'Resize', outputPort: 'image', outputName: 'Image' };
  const node: EditorNode = { id: 'resize', typeId: 'core.resize', parameters: {}, position: { x: 0, y: 0 }, descriptor: { typeId: 'core.resize', name: 'Resize', version: 1, inputs: [], outputs: [], parameters: [] } };
  const change = vi.fn();
  await act(async () => {
    root.render(<ImageNodeControls node={node} target={target} controller={controller} onDraw={() => undefined} onChange={change} />);
    controller.setTarget('A', target);
  });
  expect(host.textContent).toContain('Input resolution: 800 × 600 px');
  await act(async () => Array.from(host.querySelectorAll('button')).find((button) => button.textContent === 'Half size')!.click());
  expect(change).toHaveBeenCalledWith({ width: 400, height: 300 });
  await act(async () => controller.setTarget('A', null));
  expect(host.textContent).not.toContain('800 × 600');
  await act(async () => root.render(<ImageNodeControls node={node} target={target} inputSize={{ width: 4000, height: 3000 }} controller={controller} onDraw={() => undefined} onChange={change} />));
  expect(host.textContent).toContain('Input resolution: 4000 × 3000 px');
  await act(async () => root.unmount()); host.remove();
});
