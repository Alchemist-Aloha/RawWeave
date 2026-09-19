import type { BrowserEntry, BrowserFilter, BrowserSort, DirectoryPage, FileOperationResult } from './types';
import type { BrowserSession } from './types';

export interface BrowserPlatform {
  chooseFolder(): Promise<string | null>;
  listDirectory(path: string, offset: number, limit: number): Promise<DirectoryPage>;
  inspectFile(path: string): Promise<Pick<BrowserEntry, 'metadata' | 'thumbnail' | 'rating' | 'flag'>>;
  setFileMarks(path: string, rating: number | null, flag: BrowserEntry['flag']): Promise<void>;
  renameFile(path: string, name: string): Promise<FileOperationResult>;
  moveFile(path: string, destination: string): Promise<FileOperationResult>;
  copyFile(path: string, destination: string): Promise<FileOperationResult>;
  revealFile(path: string): Promise<void>;
  trashFile(path: string): Promise<void>;
  saveSession(session: BrowserSession): Promise<void>;
  loadSession(): Promise<BrowserSession | null>;
}

export interface BrowserState {
  currentFolder: string;
  entries: BrowserEntry[];
  selectedPaths: string[];
  view: { sort: BrowserSort; filter: BrowserFilter; thumbnailSize: 'small' | 'medium' | 'large' };
  nextOffset: number | null;
  loading: boolean;
  error: string | null;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export class BrowserController {
  public state: BrowserState = {
    currentFolder: '',
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

  private readonly listeners = new Set<(state: BrowserState) => void>();
  private loadToken = 0;

  public constructor(private readonly platform: BrowserPlatform) {}

  public subscribe(listener: (state: BrowserState) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private publish(): void {
    for (const listener of this.listeners) listener(this.state);
  }

  private setState(patch: Partial<BrowserState>): void {
    this.state = { ...this.state, ...patch };
    this.publish();
  }

  public async loadFolder(path: string): Promise<void> {
    const token = ++this.loadToken;
    this.setState({ currentFolder: path, entries: [], selectedPaths: [], nextOffset: 0, loading: true, error: null });
    try {
      await this.loadMore(token);
    } catch (error) {
      if (token === this.loadToken) this.setState({ loading: false, error: message(error) });
      throw error;
    }
  }

  public async loadMore(token = this.loadToken): Promise<void> {
    if (this.state.nextOffset === null || this.state.loading && this.state.nextOffset !== 0) return;
    const offset = this.state.nextOffset;
    if (offset === null) return;
    this.setState({ loading: true, error: null });
    try {
      const page = await this.platform.listDirectory(this.state.currentFolder, offset, 100);
      if (token !== this.loadToken || page.path !== this.state.currentFolder) return;
      const existing = new Map(this.state.entries.map((entry) => [entry.path, entry]));
      for (const entry of page.entries) existing.set(entry.path, entry);
      this.setState({ entries: [...existing.values()], nextOffset: page.hasMore ? page.nextOffset : null, loading: false });
      const files = page.entries.filter((entry) => entry.kind === 'file');
      await this.inspectIncrementally(files, token);
    } catch (error) {
      if (token === this.loadToken) this.setState({ loading: false, error: message(error) });
      throw error;
    }
  }

  private async inspectIncrementally(entries: BrowserEntry[], token: number): Promise<void> {
    for (let index = 0; index < entries.length; index += 8) {
      const batch = entries.slice(index, index + 8);
      const inspected = await Promise.all(batch.map(async (entry) => {
        try {
          return { entry, details: await this.platform.inspectFile(entry.path) };
        } catch {
          return null;
        }
      }));
      if (token !== this.loadToken) return;
      const updates = new Map(inspected.filter((item): item is NonNullable<typeof item> => item !== null).map(({ entry, details }) => [entry.path, details]));
      this.setState({ entries: this.state.entries.map((entry) => updates.has(entry.path) ? { ...entry, ...updates.get(entry.path) } : entry) });
    }
  }

  public setView(patch: Partial<BrowserState['view']>): void {
    this.setState({ view: { ...this.state.view, ...patch } });
  }

  public toggleSelection(path: string, additive = true): void {
    const selected = new Set(additive ? this.state.selectedPaths : []);
    if (selected.has(path)) selected.delete(path);
    else selected.add(path);
    this.setState({ selectedPaths: [...selected] });
  }

  public selectAllVisible(paths: string[]): void {
    this.setState({ selectedPaths: [...new Set(paths)] });
  }

  public clearSelection(): void {
    this.setState({ selectedPaths: [] });
  }

  public async mark(path: string, rating: number | null, flag: BrowserEntry['flag']): Promise<void> {
    await this.platform.setFileMarks(path, rating, flag);
    this.setState({ entries: this.state.entries.map((entry) => entry.path === path ? { ...entry, rating, flag } : entry) });
  }

  private updatePath(previousPath: string, result: FileOperationResult): void {
    const name = result.path.split(/[\\\\/]/).at(-1) ?? result.path;
    this.setState({
      entries: this.state.entries.map((entry) => entry.path === previousPath
        ? { ...entry, path: result.path, name }
        : entry),
      selectedPaths: this.state.selectedPaths.map((path) => path === previousPath ? result.path : path),
    });
  }

  public async rename(path: string, name: string): Promise<void> {
    const result = await this.platform.renameFile(path, name);
    this.updatePath(path, result);
  }

  public async move(path: string, destination: string): Promise<void> {
    const result = await this.platform.moveFile(path, destination);
    this.updatePath(path, result);
  }

  public async copy(path: string, destination: string): Promise<void> {
    await this.platform.copyFile(path, destination);
  }

  public async reveal(path: string): Promise<void> {
    await this.platform.revealFile(path);
  }

  public async trash(path: string): Promise<void> {
    await this.platform.trashFile(path);
    this.setState({ entries: this.state.entries.filter((entry) => entry.path !== path), selectedPaths: this.state.selectedPaths.filter((candidate) => candidate !== path) });
  }
}
