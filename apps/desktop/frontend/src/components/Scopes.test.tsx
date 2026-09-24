import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { describe, expect, it } from 'vitest';
import { analyzeImageData } from '../viewer/analysis';
import { Scopes } from './Scopes';

function analysisFixture() {
  return analyzeImageData({
    data: new Uint8ClampedArray([255, 0, 0, 255]),
    width: 1,
    height: 1,
  } as ImageData);
}

describe('Scopes', () => {
  it('exposes all reusable image-derived scope views', () => {
    const host = document.createElement('div');
    document.body.appendChild(host);
    const root = createRoot(host);
    const analysis = analysisFixture();

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

  it('shows one scope at a time when compact', () => {
    const host = document.createElement('div');
    document.body.appendChild(host);
    const root = createRoot(host);
    const analysis = analysisFixture();

    act(() => root.render(<Scopes analysis={analysis} compact />));

    const tabs = [...host.querySelectorAll('.scope-tab')];
    expect(tabs).toHaveLength(8);
    expect(tabs[0].getAttribute('aria-selected')).toBe('true');
    // Only the selected scope is mounted, so the narrow dock has no dead space.
    expect(host.querySelectorAll('.scope-card')).toHaveLength(1);
    expect(host.querySelector('[aria-label="Histogram"]')).not.toBeNull();

    const vectorscope = tabs.find((tab) => tab.textContent === 'Vector') as HTMLButtonElement;
    act(() => vectorscope.click());

    expect(host.querySelectorAll('.scope-card')).toHaveLength(1);
    expect(host.querySelector('[aria-label="Vectorscope"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Histogram"]')).toBeNull();
    expect(vectorscope.getAttribute('aria-selected')).toBe('true');

    act(() => root.unmount());
    host.remove();
  });
});
