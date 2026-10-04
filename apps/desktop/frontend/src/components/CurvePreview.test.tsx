import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { expect, it } from 'vitest';
import { CurvePreview, curvePlot, TransferPlot } from './CurvePreview';

it('sorts scalar points without clamping HDR or non-unit coordinates', () => {
  expect(curvePlot('core.curve', '2,3;-1,-2;0,0')).toEqual([[-1, -2], [0, 0], [2, 3]]);
});

it('matches gamma response, including the neutral diagonal', () => {
  expect(curvePlot('core.curves', 1)?.[32]).toEqual([0.5, 0.5]);
  expect(curvePlot('core.curves', 2)?.[32]?.[1]).toBeCloseTo(Math.sqrt(0.5));
});

it('rejects invalid curves instead of drawing a neutral fallback', () => {
  for (const value of ['', '0,0', '0,0;0,1', '0,0;1,NaN', '0,0;1,1,2', '0,0;1,Infinity'])
    expect(curvePlot('core.curve', value)).toBeNull();
  for (const value of [0, -1, Infinity, NaN]) expect(curvePlot('core.curves', value)).toBeNull();
  expect(curvePlot('pro.lut', '-1,0;1,1')).toBeNull();
  expect(curvePlot('pro.film-curve', '0,0;1,2;')).toEqual([[0, 0], [1, 2]]);
});

it('matches Rust float precision and bounds display allocations', () => {
  expect(curvePlot('core.curve', '0,0;1,1;1.000000001,2')).toBeNull();
  expect(curvePlot('core.curve', '0,0;1,1e40')).toBeNull();
  expect(curvePlot('core.curve', '0,0;' + '1,1;'.repeat(4096))).toBeNull();
  expect(curvePlot('core.curve', ' '.repeat(65537))).toBeNull();
  for (const typeId of ['pro.lut', 'pro.lut-tools', 'pro.film-curve']) {
    expect(curvePlot(typeId, '1,2;0,0;')).toEqual([[0, 0], [1, 2]]);
    expect(curvePlot(typeId, '0,0;2,1')).toBeNull();
  }
});

it('omits the identity reference when input and output domains do not intersect', async () => {
  const host = document.createElement('div');
  const root = createRoot(host);
  await act(async () => root.render(<TransferPlot points={[[-20, 0], [-10, 1]]} inputDomain={[-20, -10]} />));
  expect(host.querySelector('.curve-preview__neutral')).toBeNull();
  expect(host.querySelector('polyline')?.getAttribute('points')).not.toContain('NaN');
  await act(async () => root.unmount());
});

it('labels the plot and its actual domain, with an honest invalid state', async () => {
  const host = document.createElement('div');
  const root = createRoot(host);
  await act(async () => root.render(<CurvePreview typeId="core.curve" value="-1,-2;2,3" />));
  expect(host.querySelector('svg')?.getAttribute('aria-label')).toContain('Transfer curve');
  expect(host.textContent).toContain('Input −1 to 2');
  expect(host.textContent).toContain('Output −2 to 3');
  await act(async () => root.render(<CurvePreview typeId="core.curve" value="broken" />));
  expect(host.querySelector('svg')).toBeNull();
  expect(host.textContent).toContain('Enter at least two finite x,y pairs with distinct input values');
  await act(async () => root.unmount());
});
