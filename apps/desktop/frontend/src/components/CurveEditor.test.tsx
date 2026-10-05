import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it, vi } from 'vitest';
import { CurveEditor, moveCurvePoint, serializeCurve } from './CurveEditor';
import { curvePlot, type Point } from './CurvePreview';

it('keeps x ordered at f32 precision, pins endpoint x, and bounds drags without clipping loaded HDR points', () => {
  const points: Point[] = [[-1, -2], [0, 0], [2, 3]];
  const domain = { xMin: -1, xMax: 2, yMin: -2, yMax: 3 };
  const moved = moveCurvePoint(points, 1, [5, 9], domain);
  expect(moved[1][0]).toBeLessThan(2);
  expect(moved[1][1]).toBe(3);
  expect(moveCurvePoint(points, 0, [1, 1], domain)[0]).toEqual([-1, 1]);
  expect(points).toEqual([[-1, -2], [0, 0], [2, 3]]);
  expect(curvePlot('core.curve', serializeCurve(moved))).not.toBeNull();
  expect(serializeCurve(curvePlot('core.curve', '0,0;0.5,0.7;1,1')!)).toBe('0,0;0.5,0.7;1,1');
  const close = curvePlot('core.curve', '0,0;0.000000001,0.5;1,1')!;
  expect(moveCurvePoint(close, 1, [close[1][0], 0.6], { xMin: 0, xMax: 1, yMin: 0, yMax: 1 })[1][0]).toBe(close[1][0]);
});

async function mount(typeId = 'core.curve', initial = '0,0;1,1') {
  const host = document.createElement('div');
  document.body.append(host);
  const root = createRoot(host);
  const onCommit = vi.fn();
  const onCancel = vi.fn();
  const onBegin = vi.fn();
  let value = initial;
  let committed = initial;
  const render = () => root.render(<CurveEditor typeId={typeId} value={value} committedValue={committed}
    onBegin={onBegin} onDraft={(next) => { value = next; render(); }} onCommit={onCommit}
    onCancel={() => { value = committed; onCancel(); render(); }} />);
  await act(async () => render());
  const svg = host.querySelector('svg')!;
  // Screen-to-SVG mapping includes zoom and a translated/letterboxed viewport.
  if (svg) Object.assign(svg, {
    getScreenCTM: () => ({ a: 2, b: 0, c: 0, d: 2, e: 100, f: 50, inverse: () => ({ a: 0.5, b: 0, c: 0, d: 0.5, e: -50, f: -25 }) }),
    setPointerCapture: vi.fn(), releasePointerCapture: vi.fn(), hasPointerCapture: () => true,
  });
  const pointer = async (type: string, x: number, y: number, id = 1) => act(async () => {
    const event = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: 100 + 2 * x, clientY: 50 + 2 * y, button: 0 });
    Object.defineProperty(event, 'pointerId', { value: id });
    svg.dispatchEvent(event);
  });
  return { host, svg, pointer, onCommit, onCancel, onBegin, value: () => value,
    external: async (next: string) => act(async () => { committed = next; value = next; render(); }),
    ack: async (next: string) => act(async () => { committed = next; render(); }),
    cleanup: async () => { await act(async () => root.unmount()); host.remove(); } };
}

it('click-drags a new point with local feedback, fixed axes and one release commit', async () => {
  const view = await mount();
  await view.pointer('pointerdown', 100, 60);
  expect(curvePlot('core.curve', view.value())).toEqual([[0, 0], [0.5, 0.5], [1, 1]]);
  expect(view.onBegin).toHaveBeenCalledOnce();
  expect(view.svg.setPointerCapture).toHaveBeenCalledWith(1);
  await view.pointer('pointermove', 100, 39.2);
  expect(curvePlot('core.curve', view.value())?.[1][1]).toBeCloseTo(0.7);
  expect(view.host.textContent).toContain('Mapped control value: 0 to 1');
  expect(view.onCommit).not.toHaveBeenCalled();
  await view.pointer('pointerup', 100, 39.2);
  await view.pointer('lostpointercapture', 100, 39.2);
  expect(view.onCommit).toHaveBeenCalledOnce();
  expect(view.onCancel).not.toHaveBeenCalled();
  await view.cleanup();
});

it('moves existing points, cancels Escape/pointer cancellation and ignores unrelated pointers', async () => {
  const view = await mount('pro.lut', '0,0;0.5,0.7;1,1');
  await view.pointer('pointerdown', 100, 39.2);
  await view.pointer('pointermove', 120, 25, 2);
  expect(view.value()).toBe('0,0;0.5,0.7;1,1');
  await view.pointer('pointermove', 120, 25);
  expect(curvePlot('pro.lut', view.value())).toHaveLength(3);
  await act(async () => view.svg.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })));
  expect(view.value()).toBe('0,0;0.5,0.7;1,1');
  await view.pointer('pointerup', 120, 25);
  expect(view.onCommit).not.toHaveBeenCalled();
  await view.pointer('pointerdown', 70, 60);
  await view.pointer('pointercancel', 70, 60);
  expect(view.onCancel).toHaveBeenCalledTimes(2);
  expect(view.onCommit).not.toHaveBeenCalled();
  await view.cleanup();
});

it('provides keyboard point addition, grouped nudging and interior deletion with protected endpoints', async () => {
  const view = await mount();
  await act(async () => view.host.querySelector<HTMLButtonElement>('[aria-label="Add curve point"]')!.click());
  expect(curvePlot('core.curve', view.value())).toHaveLength(3);
  expect(view.onCommit).toHaveBeenCalledTimes(1);
  await act(async () => {
    view.svg.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true, cancelable: true }));
    view.svg.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', repeat: true, bubbles: true, cancelable: true }));
  });
  expect(view.onCommit).toHaveBeenCalledTimes(1);
  await act(async () => view.svg.dispatchEvent(new KeyboardEvent('keyup', { key: 'ArrowUp', bubbles: true })));
  expect(view.onCommit).toHaveBeenCalledTimes(2);
  // A handle's key event owns that point even if focus state has not refreshed.
  await act(async () => view.host.querySelector('.curve-editor__point')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true, cancelable: true })));
  expect(curvePlot('core.curve', view.value())).toHaveLength(3);
  expect(view.onCommit).toHaveBeenCalledTimes(2);
  await act(async () => view.host.querySelectorAll('.curve-editor__point')[1].dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true, cancelable: true })));
  expect(curvePlot('core.curve', view.value())).toHaveLength(2);
  expect(view.onCommit).toHaveBeenCalledTimes(3);
  await act(async () => view.svg.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true, cancelable: true })));
  expect(view.onCommit).toHaveBeenCalledTimes(3);
  await view.cleanup();
});

it('preserves the grab offset, final release position and external undo during a drag', async () => {
  const view = await mount('core.curve', '0,0;0.5,0.7;1,1');
  await view.pointer('pointerdown', 102, 40.2);
  await view.pointer('pointerup', 102, 29.8);
  expect(curvePlot('core.curve', view.value())?.[1][0]).toBe(0.5);
  expect(curvePlot('core.curve', view.value())?.[1][1]).toBeCloseTo(0.8);
  expect(view.onCommit).toHaveBeenCalledOnce();
  const baseline = view.value();
  await view.pointer('pointerdown', 100, 28.8);
  await view.pointer('pointermove', 130, 50);
  const live = view.value();
  await view.ack(baseline);
  expect(view.value()).toBe(live);
  await view.external('0,0;1,1');
  await view.pointer('pointerup', 130, 50);
  expect(view.value()).toBe('0,0;1,1');
  expect(view.onCommit).toHaveBeenCalledOnce();
  await view.cleanup();
});

it('edits selected coordinates locally, validates ordering and f32, commits once and cancels invalid edits', async () => {
  const view = await mount('pro.film-simulation', '0,0;0.5,0.7;1,1');
  const picker = view.host.querySelector<HTMLSelectElement>('[aria-label="Selected curve point"]')!;
  await act(async () => { picker.value = '1'; picker.dispatchEvent(new Event('change', { bubbles: true })); });
  const input = view.host.querySelector<HTMLInputElement>('[aria-label="Point input"]')!;
  const output = view.host.querySelector<HTMLInputElement>('[aria-label="Point output"]')!;
  const type = async (field: HTMLInputElement, text: string) => act(async () => {
    field.focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(field, text);
    field.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await type(output, '2.5');
  expect(curvePlot('pro.film-simulation', view.value())?.[1][1]).toBe(2.5);
  expect(view.onCommit).not.toHaveBeenCalled();
  await act(async () => output.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));
  expect(view.onCommit).toHaveBeenCalledOnce();
  expect(view.host.textContent).toContain('RGB level after curve: 0 to 2.5');
  await view.ack(view.value());
  await type(input, '0.25');
  await act(async () => input.blur());
  expect(curvePlot('pro.film-simulation', view.value())?.[1][0]).toBe(0.25);
  expect(view.onCommit).toHaveBeenCalledTimes(2);
  await view.ack(view.value());
  await type(input, '1');
  expect(input.getAttribute('aria-invalid')).toBe('true');
  expect(view.host.querySelector('[role="status"]')?.textContent).toContain('neighbor');
  await act(async () => input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));
  expect(view.onCommit).toHaveBeenCalledTimes(2);
  expect(curvePlot('pro.film-simulation', view.value())?.[1][0]).toBe(0.25);
  await type(output, '1e39');
  expect(output.getAttribute('aria-invalid')).toBe('true');
  await act(async () => output.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
  expect(view.onCommit).toHaveBeenCalledTimes(2);
  expect(curvePlot('pro.film-simulation', view.value())?.[1][1]).toBe(2.5);
  await view.cleanup();
});

it('does not move a clicked handle when layout changes its screen matrix before release', async () => {
  const view = await mount('core.curve', '0,0;0.5,0.7;1,1');
  await view.pointer('pointerdown', 100, 39.2);
  Object.assign(view.svg, { getScreenCTM: () => ({ inverse: () => ({ a: 0.5, b: 0, c: 0, d: 0.5, e: -51, f: -25 }) }) });
  await view.pointer('pointerup', 100, 39.2);
  expect(view.value()).toBe('0,0;0.5,0.7;1,1');
  expect(view.onCommit).not.toHaveBeenCalled();
  await view.cleanup();
});

it('starts a chart gesture directly from an unchanged coordinate focus', async () => {
  const view = await mount();
  await act(async () => view.host.querySelector<HTMLInputElement>('[aria-label="Point output"]')!.focus());
  await view.pointer('pointerdown', 100, 60);
  await view.pointer('pointermove', 100, 39.2);
  expect(curvePlot('core.curve', view.value())).toHaveLength(3);
  expect(view.onCommit).not.toHaveBeenCalled();
  await view.pointer('pointerup', 100, 39.2);
  expect(view.onCommit).toHaveBeenCalledOnce();
  await view.cleanup();
});

it('deletes an interior handle with Alt-click and a button but protects endpoints', async () => {
  const view = await mount('core.curve', '0,0;0.5,0.7;1,1');
  await act(async () => view.svg.dispatchEvent(new MouseEvent('pointerdown', {
    bubbles: true, cancelable: true, button: 0, altKey: true, clientX: 300, clientY: 128.4,
  })));
  expect(curvePlot('core.curve', view.value())).toHaveLength(2);
  expect(view.onCommit).toHaveBeenCalledOnce();
  await view.ack(view.value());
  await act(async () => view.host.querySelector<HTMLButtonElement>('[aria-label="Add curve point"]')!.click());
  await act(async () => view.host.querySelector<HTMLButtonElement>('[aria-label="Delete curve point"]')!.click());
  expect(curvePlot('core.curve', view.value())).toHaveLength(2);
  expect(view.onCommit).toHaveBeenCalledTimes(3);
  await view.pointer('pointerdown', 8, 112);
  await view.pointer('pointerup', 8, 112);
  expect(view.host.querySelector<HTMLInputElement>('[aria-label="Point input"]')?.disabled).toBe(true);
  expect(view.host.querySelector<HTMLButtonElement>('[aria-label="Delete curve point"]')?.disabled).toBe(true);
  await view.cleanup();
});

it('skips no-op clicks, invalid curves and additions beyond the point limit', async () => {
  const view = await mount();
  await view.pointer('pointerdown', 8, 112);
  await view.pointer('pointerup', 8, 112);
  expect(view.onCommit).not.toHaveBeenCalled();
  await view.cleanup();
  const invalid = await mount('core.curve', 'broken');
  expect(invalid.host.querySelector('svg')).toBeNull();
  await invalid.cleanup();
  const full = await mount('core.curve', Array.from({ length: 4096 }, (_, i) => `${(i / 4095).toFixed(6)},0`).join(';'));
  expect(full.host.querySelector<HTMLButtonElement>('[aria-label="Add curve point"]')?.disabled).toBe(true);
  await full.cleanup();
});
