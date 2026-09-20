import type { BrowserSession, BrowserViewSettings } from './types';
import type { ImageSetCollection } from '../imageset/model';

export const SESSION_VERSION = 1 as const;

export function defaultViewSettings(): BrowserViewSettings {
  return {
    sort: { by: 'name', direction: 'asc' },
    filter: { query: '', rating: 'any', flag: 'any' },
    thumbnailSize: 'medium',
  };
}

export function defaultSession(currentFolder = ''): BrowserSession {
  return {
    version: SESSION_VERSION,
    browser: {
      currentFolder,
      view: defaultViewSettings(),
      selectedPaths: [],
    },
    queue: { items: [], currentPath: null, selectedPaths: [] },
    testSet: { currentPath: null },
    workflow: { selected: null, unsavedWorkingCopy: null },
    viewer: { targets: { A: null, B: null } },
    batch: { jobId: null, statePath: null },
    imageSets: [],
    activeImageSetId: null,
    panelLayout: 'default',
  };
}

export function serializeSession(session: BrowserSession): string {
  return JSON.stringify(session, null, 2);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isNullableString(value: unknown): value is string | null {
  return value === null || typeof value === 'string';
}

function isBatchReference(value: unknown): value is { jobId: string | null; statePath: string | null } {
  return isRecord(value) && isNullableString(value.jobId) && isNullableString(value.statePath);
}

function isViewSettings(value: unknown): value is BrowserViewSettings {
  if (!isRecord(value)) return false;
  if (!isRecord(value.sort) || !isRecord(value.filter)) return false;
  return (value.sort.by === 'name' || value.sort.by === 'modified' || value.sort.by === 'size' || value.sort.by === 'rating')
    && (value.sort.direction === 'asc' || value.sort.direction === 'desc')
    && typeof value.filter.query === 'string'
    && (value.filter.rating === 'any' || value.filter.rating === 'rated' || value.filter.rating === 'unrated')
    && (value.filter.flag === 'any' || value.filter.flag === 'none' || value.filter.flag === 'pick' || value.filter.flag === 'reject')
    && (value.thumbnailSize === 'small' || value.thumbnailSize === 'medium' || value.thumbnailSize === 'large');
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === 'string');
}

function isImageSetCollection(value: unknown): value is ImageSetCollection {
  if (!isRecord(value) || typeof value.id !== 'string' || typeof value.name !== 'string') return false;
  if (value.order !== 'ordered' && value.order !== 'unordered') return false;
  if (!Array.isArray(value.members) || value.members.length === 0) return false;
  if (!isRecord(value.sharedMetadata) || !isRecord(value.alignment)) return false;
  if (value.alignment.state === 'aligned' && typeof value.alignment.referenceMember !== 'string') return false;
  if (value.alignment.state !== 'aligned' && value.alignment.state !== 'unaligned') return false;
  return value.members.every((member) => {
    if (!isRecord(member)) return false;
    return typeof member.id === 'string'
      && typeof member.path === 'string'
      && typeof member.name === 'string'
      && typeof member.order === 'number'
      && (member.metadata === null || isRecord(member.metadata))
      && (member.thumbnail === null || typeof member.thumbnail === 'string')
      && (member.error === null || typeof member.error === 'string');
  });
}

function imageSetCollections(value: unknown): value is ImageSetCollection[] {
  return Array.isArray(value) && value.every(isImageSetCollection);
}

export function parseSession(serialized: string): BrowserSession | null {
  try {
    const parsed: unknown = JSON.parse(serialized);
    if (!isRecord(parsed) || parsed.version !== SESSION_VERSION) return null;
    if (!isRecord(parsed.browser) || typeof parsed.browser.currentFolder !== 'string') return null;
    if (!isViewSettings(parsed.browser.view) || !isStringArray(parsed.browser.selectedPaths)) return null;
    if (!isRecord(parsed.queue) || !Array.isArray(parsed.queue.items) || !isStringArray(parsed.queue.selectedPaths)) return null;
    if (!isNullableString(parsed.queue.currentPath)) return null;
    if (!isRecord(parsed.testSet) || !isNullableString(parsed.testSet.currentPath)) return null;
    if (!isRecord(parsed.workflow) || !isNullableString(parsed.workflow.unsavedWorkingCopy)) return null;
    if (!isRecord(parsed.viewer) || !isRecord(parsed.viewer.targets)) return null;
    if (parsed.batch !== undefined && !isBatchReference(parsed.batch)) return null;
    if (parsed.batch === undefined) parsed.batch = { jobId: null, statePath: null };
    const imageSets = parsed.imageSets === undefined ? [] : parsed.imageSets;
    if (!imageSetCollections(imageSets)) return null;
    parsed.imageSets = imageSets;
    const activeImageSetId = parsed.activeImageSetId === undefined ? null : parsed.activeImageSetId;
    if (!isNullableString(activeImageSetId)) return null;
    parsed.activeImageSetId = activeImageSetId;
    if (activeImageSetId !== null && !imageSets.some((collection) => collection.id === activeImageSetId)) return null;
    if (typeof parsed.panelLayout !== 'string') return null;
    return parsed as unknown as BrowserSession;
  } catch {
    return null;
  }
}
