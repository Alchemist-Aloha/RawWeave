import { describe, expect, it } from 'vitest';
import { BrowserController, type BrowserPlatform } from './controller';
import type { BrowserEntry, BrowserSession, DirectoryPage, FileOperationResult } from './types';

function entry(path: string, overrides: Partial<BrowserEntry> = {}): BrowserEntry {
  const name = path.split('/').at(-1) ?? path;
  return {
    path,
    name,
    kind: 'file',
    extension: name.split('.').at(-1) ?? '',
    size: 100,
    modifiedTime: null,
    rating: null,
    flag: 'none',
    metadata: null,
    thumbnail: null,
    ...overrides,
  };
}

function platformWith(entries: BrowserEntry[]): BrowserPlatform {
  const values = new Map(entries.map((item) => [item.path, item]));
  const parent = (path: string) => path.split('/').slice(0, -1).join('/') || '/';
  return {
    async chooseFolder() { return '/photos'; },
    async listDirectory(path, offset, limit): Promise<DirectoryPage> {
      const children = [...values.values()].filter((item) => parent(item.path) === path);
      const pageEntries = children.slice(offset, offset + limit);
      const nextOffset = offset + pageEntries.length;
      return {
        path,
        entries: pageEntries,
        offset,
        nextOffset: nextOffset < children.length ? nextOffset : null,
        hasMore: nextOffset < children.length,
      };
    },
    async inspectFile(path) {
      const item = values.get(path);
      return {
        metadata: item?.metadata ?? null,
        thumbnail: item?.thumbnail ?? null,
        rating: item?.rating ?? null,
        flag: item?.flag ?? 'none',
      };
    },
    async setFileMarks(path, rating, flag) {
      const item = values.get(path);
      if (item) values.set(path, { ...item, rating, flag });
    },
    async renameFile(path, name): Promise<FileOperationResult> {
      const item = values.get(path);
      if (!item) throw new Error('missing');
      const nextPath = `${parent(path)}/${name}`;
      values.delete(path);
      values.set(nextPath, { ...item, path: nextPath, name });
      return { path: nextPath, previousPath: path };
    },
    async moveFile(path) { return this.renameFile(path, 'moved.jpg'); },
    async copyFile() { return { path: '/photos/copy.jpg' }; },
    async revealFile() {},
    async trashFile(path) { values.delete(path); },
    async saveSession() {},
    async loadSession(): Promise<BrowserSession | null> { return null; },
  };
}

describe('browser controller', () => {
  it('loads directory pages and applies file inspection incrementally', async () => {
    const first = entry('/photos/first.jpg', { thumbnail: 'data:image/jpeg;base64,one' });
    const second = entry('/photos/second.jpg', { rating: 4, flag: 'pick' });
    const controller = new BrowserController(platformWith([first, second]));

    await controller.loadFolder('/photos');

    expect(controller.state.entries.map((item) => item.path)).toEqual(['/photos/first.jpg', '/photos/second.jpg']);
    expect(controller.state.entries[0].thumbnail).toContain('data:image');
    expect(controller.state.entries[1].rating).toBe(4);
    expect(controller.state.loading).toBe(false);
  });

  it('updates the loaded entry and selection when a file is renamed', async () => {
    const controller = new BrowserController(platformWith([entry('/photos/old.jpg')]));
    await controller.loadFolder('/photos');
    controller.selectAllVisible(['/photos/old.jpg']);

    await controller.rename('/photos/old.jpg', 'new.jpg');

    expect(controller.state.entries.map((item) => item.path)).toEqual(['/photos/new.jpg']);
    expect(controller.state.selectedPaths).toEqual(['/photos/new.jpg']);
  });

  it('keeps later pages available for large folders', async () => {
    const entries = Array.from({ length: 101 }, (_, index) => entry(`/photos/${index}.jpg`));
    const controller = new BrowserController(platformWith(entries));

    await controller.loadFolder('/photos');
    expect(controller.state.entries).toHaveLength(100);
    expect(controller.state.nextOffset).toBe(100);

    await controller.loadMore();
    expect(controller.state.entries).toHaveLength(101);
    expect(controller.state.nextOffset).toBeNull();
  });

  it('does not abort a folder load when one metadata inspection fails', async () => {
    const platform = platformWith([entry('/photos/broken.raw'), entry('/photos/ok.jpg')]);
    platform.inspectFile = async (path) => {
      if (path.endsWith('broken.raw')) throw new Error('unsupported RAW');
      return { metadata: null, thumbnail: 'data:image/jpeg;base64,ok', rating: null, flag: 'none' };
    };
    const controller = new BrowserController(platform);

    await expect(controller.loadFolder('/photos')).resolves.toBeUndefined();
    expect(controller.state.entries).toHaveLength(2);
    expect(controller.state.entries.find((item) => item.name === 'ok.jpg')?.thumbnail).toContain('data:image');
  });
});
