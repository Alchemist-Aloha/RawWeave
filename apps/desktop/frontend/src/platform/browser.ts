import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { BrowserPlatform } from '../browser/controller';
import { defaultSession, parseSession, serializeSession } from '../browser/session';
import type { BrowserEntry, BrowserSession, DirectoryPage, FileOperationResult } from '../browser/types';
import { isTauriRuntime } from './runtime';

interface RustDirectoryEntry {
  path: string;
  name: string;
  kind: 'file' | 'directory';
  extension: string;
  size: number;
  modifiedTime: string | null;
  rating: number | null;
  flag: BrowserEntry['flag'];
}

interface RustDirectoryPage {
  path: string;
  entries: RustDirectoryEntry[];
  offset: number;
  nextOffset: number | null;
  hasMore: boolean;
}

interface RustFileInspection {
  metadata: BrowserEntry['metadata'];
  thumbnail: string | null;
  rating: number | null;
  flag: BrowserEntry['flag'];
}

function message(error: unknown): Error {
  if (error instanceof Error) return error;
  return new Error(typeof error === 'string' ? error : JSON.stringify(error));
}

function mapEntry(entry: RustDirectoryEntry): BrowserEntry {
  return { ...entry, metadata: null, thumbnail: null };
}

function mapPage(page: RustDirectoryPage): DirectoryPage {
  return { ...page, entries: page.entries.map(mapEntry) };
}

export function createTauriBrowserPlatform(): BrowserPlatform {
  return {
    async chooseFolder() {
      try {
        const selected = await open({ directory: true, multiple: false });
        if (Array.isArray(selected)) return selected[0] ?? null;
        return selected;
      } catch (error) {
        throw message(error);
      }
    },
    async listDirectory(path, offset, limit) {
      try {
        return mapPage(await invoke<RustDirectoryPage>('list_directory', { path, offset, limit }));
      } catch (error) {
        throw message(error);
      }
    },
    async inspectFile(path) {
      try {
        return await invoke<RustFileInspection>('inspect_file', { path });
      } catch (error) {
        throw message(error);
      }
    },
    async setFileMarks(path, rating, flag) {
      try {
        await invoke('set_file_marks', { path, rating, flag });
      } catch (error) {
        throw message(error);
      }
    },
    async renameFile(path, name) {
      try {
        return await invoke<FileOperationResult>('rename_file', { path, name });
      } catch (error) {
        throw message(error);
      }
    },
    async moveFile(path, destination) {
      try {
        return await invoke<FileOperationResult>('move_file', { path, destination });
      } catch (error) {
        throw message(error);
      }
    },
    async copyFile(path, destination) {
      try {
        return await invoke<FileOperationResult>('copy_file', { path, destination });
      } catch (error) {
        throw message(error);
      }
    },
    async revealFile(path) {
      try {
        await invoke('reveal_file', { path });
      } catch (error) {
        throw message(error);
      }
    },
    async trashFile(path) {
      try {
        await invoke('trash_file', { path });
      } catch (error) {
        throw message(error);
      }
    },
    async saveSession(session) {
      try {
        await invoke('save_session', { session: serializeSession(session) });
      } catch (error) {
        throw message(error);
      }
    },
    async loadSession() {
      try {
        const serialized = await invoke<string | null>('load_session');
        return serialized ? parseSession(serialized) : null;
      } catch (error) {
        throw message(error);
      }
    },
  };
}

export function createMemoryBrowserPlatform(initialEntries: BrowserEntry[] = []): BrowserPlatform {
  const entries = new Map(initialEntries.map((entry) => [entry.path, { ...entry }]));
  let saved: BrowserSession | null = null;
  const parent = (path: string): string => path.replace(/[\\/]$/, '').split(/[\\/]/).slice(0, -1).join('/') || '/';
  return {
    async chooseFolder() { return initialEntries.length > 0 ? parent(initialEntries[0].path) : null; },
    async listDirectory(path, offset, limit) {
      const children = [...entries.values()].filter((entry) => parent(entry.path) === path);
      const pageEntries = children.slice(offset, offset + limit);
      const next = offset + pageEntries.length;
      return { path, entries: pageEntries, offset, nextOffset: next < children.length ? next : null, hasMore: next < children.length };
    },
    async inspectFile(path) {
      const entry = entries.get(path);
      return { metadata: entry?.metadata ?? null, thumbnail: entry?.thumbnail ?? null, rating: entry?.rating ?? null, flag: entry?.flag ?? 'none' };
    },
    async setFileMarks(path, rating, flag) { const entry = entries.get(path); if (entry) entries.set(path, { ...entry, rating, flag }); },
    async renameFile(path, name) {
      const entry = entries.get(path);
      if (!entry) throw new Error('file not found');
      const nextPath = `${parent(path).replace(/\/$/, '')}/${name}`;
      entries.delete(path);
      entries.set(nextPath, { ...entry, path: nextPath, name });
      return { path: nextPath, previousPath: path };
    },
    async moveFile(path, destination) {
      const entry = entries.get(path);
      if (!entry) throw new Error('file not found');
      const name = entry.name;
      const nextPath = `${destination.replace(/[\\\\/]$/, '')}/${name}`;
      if (entries.has(nextPath)) throw new Error('destination already exists');
      entries.delete(path);
      entries.set(nextPath, { ...entry, path: nextPath });
      return { path: nextPath, previousPath: path };
    },
    async copyFile(path, destination) {
      const entry = entries.get(path);
      if (!entry) throw new Error('file not found');
      const nextPath = `${destination.replace(/[\\\\/]$/, '')}/${entry.name}`;
      if (entries.has(nextPath)) throw new Error('destination already exists');
      entries.set(nextPath, { ...entry, path: nextPath });
      return { path: nextPath };
    },
    async revealFile() {},
    async trashFile(path) { entries.delete(path); },
    async saveSession(session) { saved = JSON.parse(JSON.stringify(session)) as BrowserSession; },
    async loadSession() { return saved ? JSON.parse(JSON.stringify(saved)) as BrowserSession : defaultSession(); },
  };
}

export function createBrowserPlatform(): BrowserPlatform {
  if (isTauriRuntime()) return createTauriBrowserPlatform();
  return createMemoryBrowserPlatform();
}
