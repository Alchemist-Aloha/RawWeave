import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { describe, expect, it, vi } from 'vitest';
import { ImageSetInspector } from './ImageSetInspector';
import type { ImageSetCollection } from '../imageset/model';

const collection: ImageSetCollection = {
  id: 'imageset:/photos/a.jpg|/photos/b.jpg',
  name: 'Bracket',
  order: 'ordered',
  members: [
    { id: '/photos/a.jpg', path: '/photos/a.jpg', name: 'a.jpg', order: 0, metadata: null, thumbnail: 'a', error: null },
    { id: '/photos/b.jpg', path: '/photos/b.jpg', name: 'b.jpg', order: 1, metadata: null, thumbnail: 'b', error: 'decode failed' },
  ],
  sharedMetadata: {
    camera: 'Camera',
    lens: null,
    iso: 100,
    aperture: null,
    shutter: null,
    focalLength: null,
    captureTime: null,
    orientation: null,
  },
  alignment: { state: 'unaligned' },
};

describe('ImageSetInspector', () => {
  it('renders members and routes reorder and alignment reference controls', async () => {
    const onReorder = vi.fn();
    const onAlignmentChange = vi.fn();
    const container = document.createElement('div');
    document.body.appendChild(container);
    const root = createRoot(container);

    await act(async () => {
      root.render(
        <ImageSetInspector
          collection={collection}
          onAlignmentChange={onAlignmentChange}
          onReorder={onReorder}
        />,
      );
    });

    expect(container.textContent).toContain('Bracket');
    expect(container.textContent).toContain('decode failed');
    expect(container.querySelector('[aria-label="Move b.jpg up"]')).not.toBeNull();
    expect(container.querySelector('[aria-label="Move a.jpg down"]')).not.toBeNull();

    await act(async () => {
      container.querySelector<HTMLButtonElement>('[aria-label="Move b.jpg up"]')?.click();
    });
    expect(onReorder).toHaveBeenCalledWith('/photos/b.jpg', 0);

    await act(async () => {
      const select = container.querySelector<HTMLSelectElement>('[aria-label="Alignment reference"]');
      if (!select) throw new Error('alignment control missing');
      select.value = '/photos/b.jpg';
      select.dispatchEvent(new Event('change', { bubbles: true }));
    });
    expect(onAlignmentChange).toHaveBeenCalledWith('/photos/b.jpg');

    await act(async () => root.unmount());
    container.remove();
  });
});
