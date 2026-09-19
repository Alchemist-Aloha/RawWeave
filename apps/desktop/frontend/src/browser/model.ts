import type {
  BrowserEntry,
  BrowserFilter,
  BrowserSort,
  DirectoryBreadcrumb,
  DirectoryPage,
  QueueItem,
  QueueStatusPatch,
  WorkflowBinding,
} from './types';

function compareNullable(left: number | string | null, right: number | string | null): number {
  if (left === right) return 0;
  if (left === null) return -1;
  if (right === null) return 1;
  if (left < right) return -1;
  return 1;
}

export function sortBrowserEntries(
  entries: BrowserEntry[],
  sort: BrowserSort | BrowserSort['by'],
  direction: BrowserSort['direction'] = 'asc',
): BrowserEntry[] {
  const resolvedSort: BrowserSort = typeof sort === 'string' ? { by: sort, direction } : sort;
  return [...entries].sort((left, right) => {
    if (left.kind !== right.kind) return left.kind === 'directory' ? -1 : 1;
    let comparison = 0;
    if (resolvedSort.by === 'name') comparison = left.name.localeCompare(right.name, undefined, { sensitivity: 'base' });
    if (resolvedSort.by === 'modified') comparison = compareNullable(left.modifiedTime, right.modifiedTime);
    if (resolvedSort.by === 'size') comparison = compareNullable(left.size, right.size);
    if (resolvedSort.by === 'rating') comparison = compareNullable(left.rating, right.rating);
    if (comparison === 0) comparison = left.path.localeCompare(right.path);
    return resolvedSort.direction === 'asc' ? comparison : -comparison;
  });
}

export function filterBrowserEntries(entries: BrowserEntry[], filter: BrowserFilter): BrowserEntry[] {
  const query = filter.query.trim().toLocaleLowerCase();
  return entries.filter((entry) => {
    if (entry.kind === 'directory') return !query || entry.name.toLocaleLowerCase().includes(query);
    if (query && !entry.name.toLocaleLowerCase().includes(query)) return false;
    if (filter.rating === 'rated' && entry.rating === null) return false;
    if (filter.rating === 'unrated' && entry.rating !== null) return false;
    if (filter.flag !== 'any' && entry.flag !== filter.flag) return false;
    return true;
  });
}

export function mergeDirectoryPage(entries: BrowserEntry[], page: DirectoryPage): {
  entries: BrowserEntry[];
  nextOffset: number | null;
} {
  const merged = new Map(entries.map((entry) => [entry.path, entry]));
  for (const entry of page.entries) merged.set(entry.path, entry);
  return { entries: [...merged.values()], nextOffset: page.hasMore ? page.nextOffset : null };
}

export function breadcrumbSegments(path: string): DirectoryBreadcrumb[] {
  const separator = path.includes('\\') ? '\\' : '/';
  const normalized = separator === '/' ? path.replaceAll('\\', '/') : path.replaceAll('/', '\\');
  const prefix = separator === '\\' && /^[A-Za-z]:/.test(normalized) ? normalized.slice(0, 2) : '';
  const rawParts = normalized.split(separator).filter(Boolean);
  const result: DirectoryBreadcrumb[] = [];
  let current = separator === '/' && normalized.startsWith('/') ? '/' : prefix;
  for (const [index, part] of rawParts.entries()) {
    if (index === 0 && current === part) {
      result.push({ name: part, path: current });
      continue;
    }
    if (current && current !== separator && !current.endsWith(separator)) current += separator;
    current += part;
    result.push({ name: part, path: current });
  }
  return result;
}

export function createQueueItem(source: BrowserEntry, workflowBinding: WorkflowBinding | null): QueueItem {
  return {
    id: source.path,
    path: source.path,
    name: source.name,
    source,
    rating: source.rating,
    flag: source.flag,
    order: 0,
    workflowBinding: workflowBinding ? { ...workflowBinding } : null,
    overrides: {},
    processingStatus: 'pending',
    outputStatus: 'not-started',
    errors: [],
    warnings: [],
    testSet: false,
  };
}

function normalizeOrder(items: QueueItem[]): QueueItem[] {
  return items.map((item, order) => ({ ...item, order }));
}

export function addQueueSelection(
  items: QueueItem[],
  selection: BrowserEntry[],
  workflowBinding: WorkflowBinding | null,
): QueueItem[] {
  const next = [...items];
  const existing = new Set(items.map((item) => item.path));
  for (const source of selection) {
    if (source.kind !== 'file' || existing.has(source.path)) continue;
    next.push({ ...createQueueItem(source, workflowBinding), order: next.length });
    existing.add(source.path);
  }
  return normalizeOrder(next);
}

export function removeQueueItems(items: QueueItem[], paths: string[]): QueueItem[] {
  const removed = new Set(paths);
  return normalizeOrder(items.filter((item) => !removed.has(item.path)));
}

export function reorderQueue(items: QueueItem[], path: string, targetIndex: number): QueueItem[] {
  const index = items.findIndex((item) => item.path === path);
  if (index < 0) return items;
  const next = [...items];
  const [item] = next.splice(index, 1);
  next.splice(Math.max(0, Math.min(next.length, targetIndex)), 0, item);
  return normalizeOrder(next);
}

export function updateQueueStatus(items: QueueItem[], path: string, patch: QueueStatusPatch): QueueItem[] {
  return items.map((item) => item.path === path ? { ...item, ...patch } : item);
}

export function setTestMembership(items: QueueItem[], paths: string[], included: boolean): QueueItem[] {
  const selected = new Set(paths);
  return items.map((item) => selected.has(item.path) ? { ...item, testSet: included } : item);
}

export function moveTestCursor(
  items: QueueItem[],
  currentPath: string | null,
  direction: 'previous' | 'next',
): QueueItem | null {
  const testItems = items.filter((item) => item.testSet);
  if (currentPath === null && direction === 'previous') return null;
  const currentIndex = currentPath === null ? -1 : testItems.findIndex((item) => item.path === currentPath);
  const nextIndex = currentIndex + (direction === 'next' ? 1 : -1);
  return testItems[nextIndex] ?? null;
}

export function copyOverridesToSelected(items: QueueItem[], sourcePath: string, selectedPaths: string[]): QueueItem[] {
  const source = items.find((item) => item.path === sourcePath);
  if (!source) return items;
  const selected = new Set(selectedPaths);
  return items.map((item) => selected.has(item.path) ? { ...item, overrides: { ...source.overrides } } : item);
}

export function applyOverridesToAll(items: QueueItem[], sourcePath: string): QueueItem[] {
  const source = items.find((item) => item.path === sourcePath);
  if (!source) return items;
  return items.map((item) => ({ ...item, overrides: { ...source.overrides } }));
}

export function resetOverride(item: QueueItem, parameterId: string): QueueItem {
  const overrides = { ...item.overrides };
  delete overrides[parameterId];
  return { ...item, overrides };
}

export function promoteOverrides(item: QueueItem): Record<string, import('../editor/types').ParameterValue> {
  return { ...item.overrides };
}
