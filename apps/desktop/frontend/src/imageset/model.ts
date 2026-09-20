import type { BrowserEntry, ImageMetadata } from '../browser/types';

export type ImageSetOrder = 'ordered' | 'unordered';
export type ImageSetAlignment = { state: 'unaligned' } | { state: 'aligned'; referenceMember: string };

export interface ImageSetMember {
  id: string;
  path: string;
  name: string;
  order: number;
  metadata: ImageMetadata | null;
  thumbnail: string | null;
  error: string | null;
}

export interface ImageSetSharedMetadata {
  camera: string | null;
  lens: string | null;
  iso: number | null;
  aperture: number | null;
  shutter: number | null;
  focalLength: number | null;
  captureTime: string | null;
  orientation: string | null;
}

export interface ImageSetCollection {
  id: string;
  name: string;
  order: ImageSetOrder;
  members: ImageSetMember[];
  sharedMetadata: ImageSetSharedMetadata;
  alignment: ImageSetAlignment;
}

const MAX_IMAGE_SET_MEMBERS = 256;

function memberId(entry: BrowserEntry): string {
  if (entry.path.trim().length === 0) throw new Error('image-set member path cannot be empty');
  if (entry.path.length > 256) throw new Error(`image-set member '${entry.name}' path exceeds 256 bytes`);
  return entry.path;
}

function common<T>(values: Array<T | null>): T | null {
  if (values.length === 0 || values.some((value) => value === null)) return null;
  const first = values[0];
  return values.every((value) => value === first) ? first : null;
}

export function imageSetSharedMetadata(entries: BrowserEntry[]): ImageSetSharedMetadata {
  const metadata = entries.map((entry) => entry.metadata);
  return {
    camera: common(metadata.map((value) => value?.camera ?? null)),
    lens: common(metadata.map((value) => value?.lens ?? null)),
    iso: common(metadata.map((value) => value?.iso ?? null)),
    aperture: common(metadata.map((value) => value?.aperture ?? null)),
    shutter: common(metadata.map((value) => value?.shutter ?? null)),
    focalLength: common(metadata.map((value) => value?.focalLength ?? null)),
    captureTime: common(metadata.map((value) => value?.captureTime ?? null)),
    orientation: common(metadata.map((value) => value?.orientation ?? null)),
  };
}

function normalizeMembers(members: ImageSetMember[], order: ImageSetOrder): ImageSetMember[] {
  const normalized = order === 'unordered'
    ? [...members].sort((left, right) => left.id.localeCompare(right.id))
    : [...members];
  return normalized.map((member, index) => ({ ...member, order: index }));
}

export function createImageSet(entries: BrowserEntry[], order: ImageSetOrder, name = 'Image Set'): ImageSetCollection {
  const files = entries.filter((entry) => entry.kind === 'file');
  if (files.length === 0) throw new Error('image set must contain at least one file');
  if (files.length > MAX_IMAGE_SET_MEMBERS) throw new Error(`image set cannot contain more than ${MAX_IMAGE_SET_MEMBERS} members`);
  const members = files.map((entry) => ({
    id: memberId(entry),
    path: entry.path,
    name: entry.name,
    order: 0,
    metadata: entry.metadata,
    thumbnail: entry.thumbnail,
    error: null,
  }));
  const ids = new Set<string>();
  for (const member of members) {
    if (ids.has(member.id)) throw new Error(`image-set member id '${member.id}' is duplicated`);
    ids.add(member.id);
  }
  const normalized = normalizeMembers(members, order);
  return {
    id: `imageset:${normalized.map((member) => member.id).join('|')}`,
    name,
    order,
    members: normalized,
    sharedMetadata: imageSetSharedMetadata(files),
    alignment: { state: 'unaligned' },
  };
}

export function reorderImageSetMembers(collection: ImageSetCollection, id: string, targetIndex: number): ImageSetCollection {
  if (collection.order === 'unordered') return collection;
  const currentIndex = collection.members.findIndex((member) => member.id === id);
  if (currentIndex < 0) return collection;
  const members = [...collection.members];
  const [member] = members.splice(currentIndex, 1);
  members.splice(Math.max(0, Math.min(members.length, targetIndex)), 0, member);
  return { ...collection, members: normalizeMembers(members, collection.order) };
}

export function setImageSetAlignment(collection: ImageSetCollection, referenceMember: string | null): ImageSetCollection {
  if (referenceMember !== null && !collection.members.some((member) => member.id === referenceMember)) {
    throw new Error(`alignment reference member '${referenceMember}' is missing from image set`);
  }
  return { ...collection, alignment: referenceMember === null ? { state: 'unaligned' } : { state: 'aligned', referenceMember } };
}

export function updateImageSetMemberError(collection: ImageSetCollection, memberId: string, error: string | null): ImageSetCollection {
  if (!collection.members.some((member) => member.id === memberId)) return collection;
  return {
    ...collection,
    members: collection.members.map((member) => member.id === memberId ? { ...member, error } : member),
  };
}

export function imageSetMemberMetadata(member: ImageSetMember): ImageMetadata | null {
  return member.metadata;
}
