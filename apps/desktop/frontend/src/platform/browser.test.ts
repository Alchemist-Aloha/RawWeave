import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { createTauriBrowserPlatform } from './browser';
import { defaultSession } from '../browser/session';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

const mockedInvoke = vi.mocked(invoke);
const mockedOpen = vi.mocked(open);

beforeEach(() => {
  vi.resetAllMocks();
});

describe('Tauri browser platform', () => {
  it('maps paginated directory entries to the frontend browser model', async () => {
    mockedInvoke.mockResolvedValue({
      path: '/photos',
      entries: [{
        path: '/photos/one.jpg',
        name: 'one.jpg',
        kind: 'file',
        extension: 'jpg',
        size: 10,
        modifiedTime: '1',
        rating: null,
        flag: 'none',
      }],
      offset: 0,
      nextOffset: 1,
      hasMore: true,
    });
    const platform = createTauriBrowserPlatform();

    const page = await platform.listDirectory('/photos', 0, 100);

    expect(page.entries[0]).toMatchObject({ path: '/photos/one.jpg', metadata: null, thumbnail: null });
    expect(mockedInvoke).toHaveBeenCalledWith('list_directory', { path: '/photos', offset: 0, limit: 100 });
  });

  it('uses the native folder picker and persists a serialized session through Tauri', async () => {
    mockedOpen.mockResolvedValue('/photos');
    mockedInvoke.mockResolvedValue(null);
    const platform = createTauriBrowserPlatform();
    const session = defaultSession('/photos');

    await expect(platform.chooseFolder()).resolves.toBe('/photos');
    await platform.saveSession(session);
    await platform.loadSession();

    expect(mockedOpen).toHaveBeenCalledWith({ directory: true, multiple: false });
    expect(mockedInvoke).toHaveBeenNthCalledWith(1, 'save_session', { session: expect.stringContaining('"currentFolder": "/photos"') });
    expect(mockedInvoke).toHaveBeenNthCalledWith(2, 'load_session');
  });
});
