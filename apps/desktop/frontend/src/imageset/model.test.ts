import { describe, expect, it } from 'vitest';
import { createImageSet, imageSetSharedMetadata, reorderImageSetMembers, setImageSetAlignment } from './model';
import type { BrowserEntry } from '../browser/types';

function entry(path: string, metadata: BrowserEntry['metadata'] = null): BrowserEntry {
  return {
    path,
    name: path.split('/').at(-1) ?? path,
    kind: 'file',
    extension: 'jpg',
    size: 12,
    modifiedTime: null,
    rating: null,
    flag: 'none',
    metadata,
    thumbnail: `thumb:${path}`,
    ...{},
  };
}

const metadata = (camera: string) => ({
  width: 100,
  height: 80,
  camera,
  lens: 'Lens',
  iso: 100,
  aperture: 2.8,
  shutter: 0.01,
  focalLength: 50,
  captureTime: '2026-01-01T00:00:00Z',
  orientation: 'Normal',
  exif: { Camera: camera },
});

describe('ImageSet model', () => {
  it('creates an ordered collection with stable member identities and shared metadata', () => {
    const collection = createImageSet([entry('/photos/near.jpg', metadata('Camera')), entry('/photos/far.jpg', metadata('Camera'))], 'ordered');

    expect(collection.order).toBe('ordered');
    expect(collection.members.map((member) => member.id)).toEqual(['/photos/near.jpg', '/photos/far.jpg']);
    expect(collection.sharedMetadata.camera).toBe('Camera');
    expect(collection.alignment).toEqual({ state: 'unaligned' });
  });

  it('canonicalizes unordered members while preserving their member ids', () => {
    const collection = createImageSet([entry('/photos/z.jpg'), entry('/photos/a.jpg')], 'unordered');

    expect(collection.members.map((member) => member.id)).toEqual(['/photos/a.jpg', '/photos/z.jpg']);
  });

  it('rejects empty selections and exposes member-specific alignment errors', () => {
    expect(() => createImageSet([], 'ordered')).toThrow(/at least one/i);
    const collection = createImageSet([entry('/photos/a.jpg')], 'ordered');
    expect(() => setImageSetAlignment(collection, 'missing')).toThrow(/missing/);
  });

  it('reorders ordered members and keeps unordered members canonical', () => {
    const ordered = createImageSet([entry('/a.jpg'), entry('/b.jpg'), entry('/c.jpg')], 'ordered');
    expect(reorderImageSetMembers(ordered, '/c.jpg', 0).members.map((member) => member.id)).toEqual(['/c.jpg', '/a.jpg', '/b.jpg']);
    const unordered = createImageSet([entry('/a.jpg'), entry('/b.jpg')], 'unordered');
    expect(reorderImageSetMembers(unordered, '/b.jpg', 0).members.map((member) => member.id)).toEqual(['/a.jpg', '/b.jpg']);
  });

  it('computes shared metadata only where all members agree', () => {
    const first = entry('/a.jpg', metadata('A'));
    const second = entry('/b.jpg', metadata('B'));
    expect(imageSetSharedMetadata([first, second]).camera).toBeNull();
    expect(imageSetSharedMetadata([first, second]).lens).toBe('Lens');
  });
});
