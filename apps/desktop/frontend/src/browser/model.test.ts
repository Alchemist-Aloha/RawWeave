import { describe, expect, it } from 'vitest';
import {
  breadcrumbSegments,
  filterBrowserEntries,
  mergeDirectoryPage,
  sortBrowserEntries,
} from './model';
import type { BrowserEntry } from './types';

function entry(name: string, overrides: Partial<BrowserEntry> = {}): BrowserEntry {
  return {
    path: `/photos/${name}`,
    name,
    kind: 'file',
    extension: name.split('.').at(-1) ?? '',
    size: 100,
    modifiedTime: '2026-01-01T00:00:00.000Z',
    rating: null,
    flag: 'none',
    metadata: null,
    thumbnail: null,
    ...overrides,
  };
}

describe('file browser model', () => {
  it('sorts directories first and then photos by the selected key', () => {
    const entries = [
      entry('zeta.jpg', { modifiedTime: '2026-01-03T00:00:00.000Z' }),
      entry('alpha.jpg', { modifiedTime: '2026-01-02T00:00:00.000Z' }),
      entry('Trips', { kind: 'directory', extension: '' }),
    ];

    expect(sortBrowserEntries(entries, 'name', 'asc').map((item) => item.name)).toEqual([
      'Trips',
      'alpha.jpg',
      'zeta.jpg',
    ]);
    expect(sortBrowserEntries(entries, 'modified', 'desc').map((item) => item.name)).toEqual([
      'Trips',
      'zeta.jpg',
      'alpha.jpg',
    ]);
  });

  it('filters by text, rating and flag without mutating the loaded page', () => {
    const entries = [
      entry('picked.jpg', { rating: 5, flag: 'pick' }),
      entry('rejected.jpg', { rating: 1, flag: 'reject' }),
    ];

    expect(filterBrowserEntries(entries, { query: 'pick', rating: 'any', flag: 'any' })).toHaveLength(1);
    expect(filterBrowserEntries(entries, { query: '', rating: 'rated', flag: 'any' })).toHaveLength(2);
    expect(filterBrowserEntries(entries, { query: '', rating: 'any', flag: 'reject' })[0].name).toBe('rejected.jpg');
    expect(entries.map((item) => item.name)).toEqual(['picked.jpg', 'rejected.jpg']);
  });

  it('merges incremental pages by stable path and reports the next cursor', () => {
    const first = mergeDirectoryPage([], {
      path: '/photos',
      entries: [entry('a.jpg'), entry('b.jpg')],
      offset: 0,
      nextOffset: 2,
      hasMore: true,
    });
    const second = mergeDirectoryPage(first.entries, {
      path: '/photos',
      entries: [entry('b.jpg', { size: 200 }), entry('c.jpg')],
      offset: 2,
      nextOffset: null,
      hasMore: false,
    });

    expect(first.nextOffset).toBe(2);
    expect(second.entries.map((item) => [item.name, item.size])).toEqual([
      ['a.jpg', 100],
      ['b.jpg', 200],
      ['c.jpg', 100],
    ]);
    expect(second.nextOffset).toBeNull();
  });

  it('creates platform-neutral breadcrumb segments', () => {
    expect(breadcrumbSegments('/home/likun/photos')).toEqual([
      { name: 'home', path: '/home' },
      { name: 'likun', path: '/home/likun' },
      { name: 'photos', path: '/home/likun/photos' },
    ]);
    expect(breadcrumbSegments('C:\\Photos\\Trips')).toEqual([
      { name: 'C:', path: 'C:' },
      { name: 'Photos', path: 'C:\\Photos' },
      { name: 'Trips', path: 'C:\\Photos\\Trips' },
    ]);
  });
});
