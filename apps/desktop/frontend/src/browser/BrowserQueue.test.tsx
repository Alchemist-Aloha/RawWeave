import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { BrowserQueue, createBrowserSession } from './BrowserQueue';
import type { BrowserPlatform, BrowserState } from './controller';
import type { QueueState } from '../queue/controller';
import type { BrowserEntry, BrowserSession, DirectoryPage, FileOperationResult } from './types';
import type { WorkflowParameter } from '../editor/types';

function entry(path: string, overrides: Partial<BrowserEntry> = {}): BrowserEntry {
  const name = path.split('/').at(-1) ?? path;
  return {
    path,
    name,
    kind: 'file',
    extension: name.endsWith('.jpg') ? 'jpg' : '',
    size: 100,
    modifiedTime: null,
    rating: null,
    flag: 'none',
    metadata: null,
    thumbnail: `data:image/png;base64,${name}`,
    ...overrides,
  };
}

function fakePlatform(): BrowserPlatform {
  const files = new Map<string, BrowserEntry>([
    ['/photos', entry('/photos', { name: 'photos', kind: 'directory', thumbnail: null })],
    ['/photos/Trips', entry('/photos/Trips', { name: 'Trips', kind: 'directory', thumbnail: null })],
    ['/photos/one.jpg', entry('/photos/one.jpg')],
    ['/photos/two.jpg', entry('/photos/two.jpg', { rating: 4, flag: 'pick' })],
  ]);
  let saved: BrowserSession | null = null;
  const parent = (path: string) => path.split('/').slice(0, -1).join('/') || '/';
  return {
    async chooseFolder() { return '/photos'; },
    async listDirectory(path, offset, limit): Promise<DirectoryPage> {
      const children = [...files.values()].filter((item) => parent(item.path) === path);
      const pageEntries = children.slice(offset, offset + limit);
      const next = offset + pageEntries.length;
      return { path, entries: pageEntries, offset, nextOffset: next < children.length ? next : null, hasMore: next < children.length };
    },
    async inspectFile(path) {
      const item = files.get(path);
      return { metadata: item?.metadata ?? null, thumbnail: item?.thumbnail ?? null, rating: item?.rating ?? null, flag: item?.flag ?? 'none' };
    },
    async setFileMarks(path, rating, flag) {
      const item = files.get(path);
      if (item) files.set(path, { ...item, rating, flag });
    },
    async renameFile(path, name): Promise<FileOperationResult> {
      const item = files.get(path);
      if (!item) throw new Error('missing');
      const next = `${parent(path)}/${name}`;
      files.delete(path);
      files.set(next, { ...item, path: next, name });
      return { path: next, previousPath: path };
    },
    async moveFile(path) { return this.renameFile(path, 'moved.jpg'); },
    async copyFile(path, destination) { return { path: `${destination}/${path.split('/').at(-1)}` }; },
    async revealFile() {},
    async trashFile(path) { files.delete(path); },
    async saveSession(session) { saved = structuredClone(session); },
    async loadSession() { return saved ? structuredClone(saved) : null; },
  };
}

let root: Root | null = null;
let container: HTMLDivElement | null = null;

afterEach(() => {
  if (root) {
    act(() => root?.unmount());
    root = null;
  }
  container?.remove();
  container = null;
});

async function renderQueue(platform: BrowserPlatform, props: Partial<React.ComponentProps<typeof BrowserQueue>> = {}) {
  container = document.createElement('div');
  document.body.append(container);
  root = createRoot(container);
  await act(async () => {
    root?.render(<BrowserQueue platform={platform} {...props} />);
  });
  return container;
}

describe('BrowserQueue', () => {
  it('persists workflow, viewer, and panel session metadata alongside queue state', () => {
    const browser: BrowserState = {
      currentFolder: '/photos',
      entries: [],
      selectedPaths: [],
      view: {
        sort: { by: 'name', direction: 'asc' },
        filter: { query: '', rating: 'any', flag: 'any' },
        thumbnailSize: 'medium',
      },
      nextOffset: null,
      loading: false,
      error: null,
    };
    const queue: QueueState = { items: [], selectedPaths: [], currentPath: null, testSetCurrentPath: null };

    expect(createBrowserSession(browser, queue, {
      workflowBinding: { id: 'workflow', version: '1.0.0', hash: 'hash' },
      unsavedWorkflowWorkingCopy: '{"version":1}',
      viewerTargets: {
        A: { nodeId: 'display', outputPort: 'display' },
        B: null,
      },
      panelLayout: 'split',
    })).toMatchObject({
      workflow: {
        selected: { id: 'workflow', version: '1.0.0', hash: 'hash' },
        unsavedWorkingCopy: '{"version":1}',
      },
      viewer: { targets: { A: { nodeId: 'display', outputPort: 'display' }, B: null } },
      panelLayout: 'split',
    });
  });

  it('shows the browser, navigates folders, and switches the selected preview', async () => {
    const view = await renderQueue(fakePlatform());
    await act(async () => {
      view.querySelector<HTMLButtonElement>('[aria-label="Choose folder"]')?.click();
    });

    expect(view.querySelector('[aria-label="File browser"]')).not.toBeNull();
    expect(view.textContent).toContain('one.jpg');
    expect(view.textContent).toContain('Trips');

    await act(async () => {
      view.querySelector<HTMLButtonElement>('[aria-label="Open folder Trips"]')?.click();
    });
    expect(view.textContent).toContain('Folder is empty');

    await act(async () => {
      view.querySelector<HTMLButtonElement>('[aria-label="Choose folder"]')?.click();
    });
    await act(async () => {
      view.querySelector<HTMLButtonElement>('[aria-label="Select one.jpg"]')?.click();
    });
    expect(view.querySelector('img[alt="Preview one.jpg"]')).not.toBeNull();
  });

  it('adds browser selection to the queue and marks a test item', async () => {
    const view = await renderQueue(fakePlatform());
    await act(async () => {
      view.querySelector<HTMLButtonElement>('[aria-label="Choose folder"]')?.click();
    });
    await act(async () => {
      view.querySelector<HTMLButtonElement>('[aria-label="Select one.jpg"]')?.click();
      view.querySelector<HTMLButtonElement>('[aria-label="Select two.jpg"]')?.dispatchEvent(new MouseEvent('click', { bubbles: true, ctrlKey: true }));
    });
    await act(async () => {
      view.querySelector<HTMLButtonElement>('[aria-label="Add selected to queue"]')?.click();
    });

    expect(view.textContent).toContain('Working Queue');
    expect(view.textContent).toContain('2 queued');
    await act(async () => {
      view.querySelector<HTMLInputElement>('[aria-label="Include one.jpg in test set"]')?.click();
    });
    expect(view.textContent).toContain('Test Set · 1');
  });

  it('edits current workflow parameters per image and exposes override actions', async () => {
    const promoted: Record<string, unknown>[] = [];
    const workflowParameters: WorkflowParameter[] = [{
      id: 'exposure:enabled',
      name: 'Enabled',
      nodeId: 'exposure',
      parameterId: 'enabled',
      parameterType: 'Boolean',
      default: false,
    }];
    const onPromoteOverrides = vi.fn(async (overrides: Record<string, unknown>) => {
      promoted.push(overrides);
    });
    const view = await renderQueue(fakePlatform(), { workflowParameters, onPromoteOverrides });
    await act(async () => {
      view.querySelector<HTMLButtonElement>('[aria-label="Choose folder"]')?.click();
    });
    await act(async () => {
      view.querySelector<HTMLButtonElement>('[aria-label="Select one.jpg"]')?.click();
    });
    await act(async () => {
      view.querySelector<HTMLButtonElement>('[aria-label="Add selected to queue"]')?.click();
    });

    const override = view.querySelector<HTMLInputElement>('[aria-label="Override Enabled"]');
    expect(override).not.toBeNull();
    expect(view.querySelector('[aria-label="Copy overrides to selected"]')).not.toBeNull();
    expect(view.querySelector('[aria-label="Apply overrides to all"]')).not.toBeNull();
    expect(view.querySelector('[aria-label="Promote overrides to workflow default"]')).not.toBeNull();

    await act(async () => {
      override?.click();
    });
    expect(view.textContent).toContain('1 override');

    await act(async () => {
      view.querySelector<HTMLButtonElement>('[aria-label="Promote overrides to workflow default"]')?.click();
    });
    expect(onPromoteOverrides).toHaveBeenCalledWith({ 'exposure:enabled': true });
    expect(promoted).toEqual([{ 'exposure:enabled': true }]);

    await act(async () => {
      view.querySelector<HTMLButtonElement>('[aria-label="Reset Enabled override"]')?.click();
    });
    expect(view.textContent).toContain('workflow default');
  });
});
