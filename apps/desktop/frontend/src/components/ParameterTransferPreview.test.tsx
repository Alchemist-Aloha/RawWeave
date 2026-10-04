import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it } from 'vitest';
import type { EditorNode } from '../editor/types';
import { parameterTransferPlot, ParameterTransferPreview } from './ParameterTransferPreview';

function node(typeId: string, parameters: EditorNode['parameters'] = {}): EditorNode {
  return { id: 'test', typeId, parameters, position: { x: 0, y: 0 }, descriptor: {
    typeId, name: typeId === 'core.map-range' ? 'Map Range' : typeId, version: 1, inputs: [], outputs: [], parameters: [],
  } };
}

it('combines Levels black, white and gamma with exact clipping boundaries', () => {
  const plot = parameterTransferPlot(node('core.levels', { black_point: 0.2, white_point: 0.8, gamma: 2 }));
  expect(plot?.points).toContainEqual([Math.fround(0.2), 0]);
  expect(plot?.points).toContainEqual([Math.fround(0.8), 1]);
  expect(plot?.points?.find(([x]) => Math.abs(x - 0.5) < 1e-6)?.[1]).toBeCloseTo(Math.sqrt(0.5));
  expect(plot?.points?.[0]).toEqual([0, 0]);
  expect(plot?.points?.at(-1)).toEqual([1, 1]);
  expect(parameterTransferPlot(node('core.levels', { black_point: -2, white_point: 3 }))?.inputDomain).toEqual([-2, 3]);
});

it('honors descriptor defaults and rejects coupled invalid Levels parameters', () => {
  const levels = node('core.levels');
  levels.descriptor.parameters = [{ id: 'gamma', name: 'Gamma', parameterType: 'Float', default: 2, min: null, max: null }];
  expect(parameterTransferPlot(levels)?.points?.[32]?.[1]).toBeCloseTo(Math.sqrt(0.5));
  const invalid: EditorNode['parameters'][] = [{ white_point: 0 }, { black_point: 2 }, { gamma: 0 }, { gamma: Infinity }, { black_point: 'bad' }];
  for (const parameters of invalid)
    expect(parameterTransferPlot(node('core.levels', parameters))?.error).toBeTruthy();
});

it('shows extrapolation and clamp tails for reversed Map Range endpoints', () => {
  const parameters = { in_min: 1, in_max: 0, out_min: -2, out_max: 2, clamp: false };
  const plot = parameterTransferPlot(node('core.map-range', parameters));
  expect(plot?.inputDomain).toEqual([-0.25, 1.25]);
  expect(plot?.points).toEqual([[-0.25, 3], [0, 2], [1, -2], [1.25, -3]]);
  expect(parameterTransferPlot(node('core.map-range', { ...parameters, clamp: true }))?.points)
    .toEqual([[-0.25, 2], [0, 2], [1, -2], [1.25, -2]]);
  expect(parameterTransferPlot(node('core.map-range', { in_min: 0, in_max: 1e-8 }))?.error).toBeTruthy();
});

it('shows Clamp saturation, including equal and non-unit bounds, without accepting reversed bounds', () => {
  expect(parameterTransferPlot(node('core.clamp', { min: -2, max: 2 }))?.points)
    .toEqual([[-3, -2], [-2, -2], [2, 2], [3, 2]]);
  expect(parameterTransferPlot(node('core.clamp', { min: 2, max: 2 }))?.points)
    .toEqual([[1.5, 2], [2, 2], [2.5, 2]]);
  expect(parameterTransferPlot(node('core.clamp', { min: 2, max: 1 }))?.error).toBeTruthy();
  expect(parameterTransferPlot(node('core.exposure'))).toBeNull();
});

it('renders accessible diagrams and actionable validation instead of misleading fallback curves', async () => {
  const host = document.createElement('div');
  const root = createRoot(host);
  await act(async () => root.render(<ParameterTransferPreview node={node('core.map-range', { out_min: 1, out_max: 0, clamp: true })} />));
  expect(host.querySelector('svg')?.getAttribute('aria-label')).toContain('Map Range');
  expect(host.textContent).toContain('Clamped');
  expect(host.textContent).toContain('Input 0 maps to 1; input 1 maps to 0');
  await act(async () => root.render(<ParameterTransferPreview node={node('core.levels', { black_point: 1, white_point: 0 })} />));
  expect(host.querySelector('svg')).toBeNull();
  expect(host.textContent).toContain('White Point must be greater than Black Point');
  await act(async () => root.unmount());
});
