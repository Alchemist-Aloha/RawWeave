import { describe, expect, it } from 'vitest';
import {
  addQueueSelection,
  applyOverridesToAll,
  copyOverridesToSelected,
  createQueueItem,
  moveTestCursor,
  promoteOverrides,
  reorderQueue,
  resetOverride,
  setTestMembership,
  updateQueueStatus,
} from './model';
import type { BrowserEntry } from '../browser/types';

function entry(name: string): BrowserEntry {
  return {
    path: `/photos/${name}`,
    name,
    kind: 'file',
    extension: 'jpg',
    size: 100,
    modifiedTime: null,
    rating: null,
    flag: 'none',
    metadata: null,
    thumbnail: null,
  };
}

describe('working queue model', () => {
  it('adds selected files once while preserving queue order', () => {
    const one = createQueueItem(entry('one.jpg'), { id: 'workflow', version: '1.0.0', hash: 'abc' });
    const two = createQueueItem(entry('two.jpg'), null);

    const next = addQueueSelection([one], [entry('two.jpg'), entry('one.jpg')], null);
    expect(next.map((item) => item.path)).toEqual(['/photos/one.jpg', '/photos/two.jpg']);
    expect(next[1].order).toBe(1);
    expect(two.path).toBe('/photos/two.jpg');
  });

  it('reorders and updates processing/output status immutably', () => {
    const items = [createQueueItem(entry('a.jpg'), null), createQueueItem(entry('b.jpg'), null)];
    const reordered = reorderQueue(items, '/photos/b.jpg', 0);
    const updated = updateQueueStatus(reordered, '/photos/b.jpg', {
      processingStatus: 'complete',
      outputStatus: 'written',
      warnings: ['8-bit output'],
    });

    expect(reordered.map((item) => item.path)).toEqual(['/photos/b.jpg', '/photos/a.jpg']);
    expect(updated[0]).toMatchObject({ processingStatus: 'complete', outputStatus: 'written' });
    expect(updated[0].warnings).toEqual(['8-bit output']);
  });

  it('navigates a bounded test set and keeps membership separate from queue order', () => {
    const items = [createQueueItem(entry('a.jpg'), null), createQueueItem(entry('b.jpg'), null)];
    const marked = setTestMembership(items, ['/photos/b.jpg'], true);
    expect(marked.map((item) => item.testSet)).toEqual([false, true]);
    expect(moveTestCursor(marked, null, 'next')?.path).toBe('/photos/b.jpg');
    expect(moveTestCursor(marked, '/photos/b.jpg', 'next')).toBeNull();
    expect(moveTestCursor(marked, null, 'previous')).toBeNull();
  });

  it('copies, applies, resets, and promotes overrides without cloning queue workflow graphs', () => {
    const items = [createQueueItem(entry('a.jpg'), null), createQueueItem(entry('b.jpg'), null)];
    const source = { ...items[0], overrides: { 'exposure:exposure': 1.5 } };
    const copied = copyOverridesToSelected([source, items[1]], source.path, [items[1].path]);
    expect(copied[1].overrides).toEqual(source.overrides);
    expect(copied[1]).not.toBe(source);

    const applied = applyOverridesToAll(copied, source.path);
    expect(applied.every((item) => item.overrides['exposure:exposure'] === 1.5)).toBe(true);
    expect(resetOverride(applied[0], 'exposure:exposure').overrides).toEqual({});
    expect(promoteOverrides(source)).toEqual({ 'exposure:exposure': 1.5 });
  });
});
