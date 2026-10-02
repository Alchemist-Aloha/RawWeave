import { act, createRef } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import { GeometryOverlay } from './GeometryOverlay';

it('draws against image bounds, commits only on release, and cancels without changes', async () => {
  const host = document.createElement('div'); document.body.append(host);
  const root = createRoot(host);
  const image = createRef<HTMLImageElement>();
  const commit = vi.fn();
  const cancel = vi.fn();
  await act(async () => root.render(<><img ref={image} /><GeometryOverlay typeId="core.crop" imageRef={image} size={{ width: 1000, height: 500 }} origin={{ x: 20, y: 30 }} onChange={commit} onCancel={cancel} /></>));
  image.current!.getBoundingClientRect = () => ({ left: 100, top: 50, width: 500, height: 250 } as DOMRect);
  const surface = host.querySelector('[aria-label="Draw image region"]')!;
  Object.defineProperty(surface, 'setPointerCapture', { value: () => { throw new DOMException('inactive pointer', 'NotFoundError'); } });
  const pointer = async (type: string, x: number, y: number) => {
    const event = new MouseEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0 });
    await act(async () => surface.dispatchEvent(event));
  };
  await pointer('pointerdown', 150, 100);
  await pointer('pointermove', 350, 200);
  expect(commit).not.toHaveBeenCalled();
  await pointer('pointerup', 350, 200);
  expect(commit).toHaveBeenCalledWith({ x: 100, y: 100, width: 400, height: 200 });
  commit.mockClear();
  await pointer('pointerdown', 150, 100);
  await pointer('pointercancel', 350, 200);
  expect(commit).not.toHaveBeenCalled();
  await act(async () => root.unmount()); host.remove();
});
