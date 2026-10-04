import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it } from 'vitest';
import type { EditorNode } from '../editor/types';
import { ColorParameterPreview, hueIntervals, referenceRgb } from './ColorParameterPreview';

const node = (typeId: string, parameters: EditorNode['parameters'] = {}): EditorNode => ({
  id: 'color', typeId, parameters, position: { x: 0, y: 0 },
  descriptor: { typeId, name: 'Color node', version: 1, inputs: [], outputs: [], parameters: [] },
});

it('renders RGB values as an explicitly unconverted reference without clipping invalid channels', () => {
  expect(referenceRgb([1, 0, 0.5])).toBe('rgb(100% 0% 50%)');
  for (const channels of [[NaN, 0, 0], [Infinity, 0, 0], [-0.1, 0, 0], [1.1, 0, 0]])
    expect(referenceRgb(channels)).toBeNull();
});

it('wraps the hue radius around red, including endpoint equivalence and a half-circle radius', () => {
  expect(hueIntervals(0, 0.1)).toEqual([[0, 0.1], [0.9, 1]]);
  expect(hueIntervals(1, 0.1)).toEqual(hueIntervals(0, 0.1));
  expect(hueIntervals(0.5, 0.2)).toEqual([[0.3, 0.7]]);
  expect(hueIntervals(0.8, 0.5)).toEqual([[0, 1]]);
  expect(hueIntervals(0.95, 0.1)?.[0]?.[1]).toBeCloseTo(0.05);
  for (const [hue, width] of [[NaN, 0.1], [1.1, 0.1], [0, 0], [0, 0.6], [0, Infinity]])
    expect(hueIntervals(hue, width)).toBeNull();
});

it('uses descriptor defaults and labels the working-space/display boundary beside the swatch', async () => {
  const host = document.createElement('div');
  const root = createRoot(host);
  const qualifier = node('core.mask-color-qualifier');
  qualifier.descriptor.parameters = [{ id: 'target_r', name: 'Target Red', parameterType: 'Float', default: 0.25, min: 0, max: 1 }];
  await act(async () => root.render(<ColorParameterPreview node={qualifier} />));
  expect(host.querySelector<HTMLElement>('.color-reference__swatch')?.style.backgroundColor).toBe('rgb(64, 255, 255)');
  expect(host.textContent).toContain('RGB 0.25, 1, 1');
  expect(host.textContent).toContain('input/working-space RGB');
  expect(host.textContent).toContain('not color-managed');
  qualifier.parameters.target_r = NaN;
  await act(async () => root.render(<ColorParameterPreview node={qualifier} />));
  expect(host.querySelector('.color-reference__swatch')).toBeNull();
  expect(host.textContent).toContain('finite RGB values between 0 and 1');
  qualifier.parameters = { color: '#ff0000' };
  await act(async () => root.render(<ColorParameterPreview node={qualifier} />));
  expect(host.textContent).toContain('Legacy color parameter overrides');
  await act(async () => root.unmount());
});

it('labels hue selection and strength without relying on colors alone', async () => {
  const host = document.createElement('div');
  const root = createRoot(host);
  await act(async () => root.render(<ColorParameterPreview node={node('pro.color-zones', { hue: 0, width: 0.1 })} />));
  expect(host.querySelectorAll('.color-hue__interval')).toHaveLength(2);
  expect(host.querySelector('[role="img"]')?.getAttribute('aria-label')).toContain('Target Hue 0°; radius 36°');
  await act(async () => root.render(<ColorParameterPreview node={node('pro.split-toning')} />));
  expect(host.textContent).toContain('Shadow Hue 216°');
  expect(host.textContent).toContain('Highlight Hue 36°');
  expect(host.textContent).toContain('No tint at 0% saturation');
  expect(host.querySelectorAll('.color-hue__marker')).toHaveLength(2);
  await act(async () => root.render(<ColorParameterPreview node={node('pro.split-toning', { shadow_hue: Infinity })} />));
  expect(host.querySelectorAll('.color-hue__marker')).toHaveLength(1);
  expect(host.textContent).toContain('Shadow Hue requires');
  await act(async () => root.render(<ColorParameterPreview node={node('core.exposure')} />));
  expect(host.textContent).toBe('');
  await act(async () => root.unmount());
});
