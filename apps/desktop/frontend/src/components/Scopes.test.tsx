import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { describe, expect, it } from 'vitest';
import { analyzeImageData } from '../viewer/analysis';
import { Scopes } from './Scopes';

describe('Scopes', () => {
  it('exposes all reusable image-derived scope views', () => {
    const host = document.createElement('div');
    document.body.appendChild(host);
    const root = createRoot(host);
    const analysis = analyzeImageData({
      data: new Uint8ClampedArray([255, 0, 0, 255]),
      width: 1,
      height: 1,
    } as ImageData);

    act(() => root.render(<Scopes analysis={analysis} />));

    expect(host.querySelector('[aria-label="Histogram"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Waveform"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="RGB Parade"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Vectorscope"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="False Color"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Gamut Warning"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Pixel Inspector"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Zebra"]')).not.toBeNull();

    act(() => root.unmount());
    host.remove();
  });
});
