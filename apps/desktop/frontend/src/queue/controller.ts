import type { ParameterValue } from '../editor/types';
import {
  addQueueSelection,
  applyOverridesToAll,
  copyOverridesToSelected,
  moveTestCursor,
  promoteOverrides,
  removeQueueItems,
  reorderQueue,
  resetOverride,
  setTestMembership,
  updateQueueStatus,
} from '../browser/model';
import type { BrowserEntry, QueueItem, QueueStatusPatch, WorkflowBinding } from '../browser/types';

export interface QueueState {
  items: QueueItem[];
  selectedPaths: string[];
  currentPath: string | null;
  testSetCurrentPath: string | null;
}

export class QueueController {
  public state: QueueState = { items: [], selectedPaths: [], currentPath: null, testSetCurrentPath: null };
  private readonly listeners = new Set<(state: QueueState) => void>();

  public subscribe(listener: (state: QueueState) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private publish(): void {
    for (const listener of this.listeners) listener(this.state);
  }

  private setState(patch: Partial<QueueState>): void {
    this.state = { ...this.state, ...patch };
    this.publish();
  }

  public restore(state: Partial<QueueState>): void {
    this.setState({
      items: state.items ?? [],
      selectedPaths: state.selectedPaths ?? [],
      currentPath: state.currentPath ?? null,
      testSetCurrentPath: state.testSetCurrentPath ?? null,
    });
  }

  public addSelection(entries: BrowserEntry[], workflowBinding: WorkflowBinding | null): void {
    const items = addQueueSelection(this.state.items, entries, workflowBinding);
    const selectedPaths = [...new Set(entries.filter((entry) => entry.kind === 'file').map((entry) => entry.path))];
    this.setState({ items, selectedPaths, currentPath: this.state.currentPath ?? items[0]?.path ?? null });
  }

  public remove(paths = this.state.selectedPaths): void {
    const items = removeQueueItems(this.state.items, paths);
    const removed = new Set(paths);
    this.setState({
      items,
      selectedPaths: this.state.selectedPaths.filter((path) => !removed.has(path)),
      currentPath: removed.has(this.state.currentPath ?? '') ? items[0]?.path ?? null : this.state.currentPath,
      testSetCurrentPath: removed.has(this.state.testSetCurrentPath ?? '') ? null : this.state.testSetCurrentPath,
    });
  }

  public clear(): void {
    this.setState({ items: [], selectedPaths: [], currentPath: null, testSetCurrentPath: null });
  }

  public select(paths: string[]): void {
    const valid = new Set(this.state.items.map((item) => item.path));
    this.setState({ selectedPaths: [...new Set(paths)].filter((path) => valid.has(path)) });
  }

  public choose(path: string): void {
    if (this.state.items.some((item) => item.path === path)) this.setState({ currentPath: path });
  }

  public reorder(path: string, targetIndex: number): void {
    this.setState({ items: reorderQueue(this.state.items, path, targetIndex) });
  }

  public setStatus(path: string, patch: QueueStatusPatch): void {
    this.setState({ items: updateQueueStatus(this.state.items, path, patch) });
  }

  public updateSourceMarks(path: string, rating: BrowserEntry['rating'], flag: BrowserEntry['flag']): void {
    this.setState({
      items: this.state.items.map((item) => item.path === path
        ? { ...item, rating, flag, source: { ...item.source, rating, flag } }
        : item),
    });
  }

  public setTestSet(paths: string[], included: boolean): void {
    const items = setTestMembership(this.state.items, paths, included);
    // The Quick-compare cursor must stay inside the test set: leaving it on an
    // item that just left the set made the header name a non-member and turned
    // "previous" into a no-op.
    const current = this.state.testSetCurrentPath;
    const stillInSet = current !== null && items.some((item) => item.path === current && item.testSet);
    this.setState({ items, testSetCurrentPath: stillInSet ? current : null });
  }

  public moveTest(direction: 'previous' | 'next'): QueueItem | null {
    const item = moveTestCursor(this.state.items, this.state.testSetCurrentPath, direction);
    this.setState({ testSetCurrentPath: item?.path ?? this.state.testSetCurrentPath });
    if (item) this.setState({ currentPath: item.path });
    return item;
  }

  public setOverride(path: string, parameterId: string, value: ParameterValue | null): void {
    this.setState({ items: this.state.items.map((item) => {
      if (item.path !== path) return item;
      if (value === null) return resetOverride(item, parameterId);
      return { ...item, overrides: { ...item.overrides, [parameterId]: value } };
    }) });
  }

  public resetOverride(path: string, parameterId: string): void {
    this.setOverride(path, parameterId, null);
  }

  public copyToSelected(path = this.state.currentPath): void {
    if (path) this.setState({ items: copyOverridesToSelected(this.state.items, path, this.state.selectedPaths) });
  }

  public applyToAll(path = this.state.currentPath): void {
    if (path) this.setState({ items: applyOverridesToAll(this.state.items, path) });
  }

  public promote(path = this.state.currentPath): Record<string, ParameterValue> {
    const item = this.state.items.find((candidate) => candidate.path === path);
    return item ? promoteOverrides(item) : {};
  }
}
