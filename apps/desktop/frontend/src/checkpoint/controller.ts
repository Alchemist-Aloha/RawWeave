import type {
  CheckpointControllerState,
  CheckpointPlatform,
  CheckpointProgressEvent,
  CheckpointStatus,
} from './types';

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function normalize(status: CheckpointStatus): CheckpointStatus {
  return {
    ...status,
    canUseCommitted: status.canUseCommitted ?? Boolean(status.committedArtifactId),
  };
}

export class CheckpointController {
  public state: CheckpointControllerState = {
    statuses: [],
    status: null,
    loading: false,
    operation: null,
    error: null,
  };

  private readonly listeners = new Set<(state: CheckpointControllerState) => void>();
  private readonly unsubscribeProgress: (() => void) | null;

  public constructor(private readonly platform: CheckpointPlatform) {
    this.unsubscribeProgress = platform.subscribeProgress?.((event) => this.onProgress(event)) ?? null;
  }

  public dispose(): void {
    this.unsubscribeProgress?.();
  }

  public subscribe(listener: (state: CheckpointControllerState) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private publish(): void {
    for (const listener of this.listeners) listener(this.state);
  }

  private patch(patch: Partial<CheckpointControllerState>): void {
    this.state = { ...this.state, ...patch };
    this.publish();
  }

  private setStatus(status: CheckpointStatus): CheckpointStatus {
    const next = normalize(status);
    const statuses = this.state.statuses.filter((candidate) => candidate.nodeId !== next.nodeId);
    statuses.push(next);
    statuses.sort((left, right) => left.nodeId.localeCompare(right.nodeId));
    const selected = this.state.status?.nodeId;
    this.patch({
      statuses,
      status: !selected || selected === next.nodeId ? next : this.state.status,
    });
    return next;
  }

  private onProgress(event: CheckpointProgressEvent): void {
    const current = this.state.statuses.find((candidate) => candidate.nodeId === event.nodeId);
    if (!current) return;
    const progress = event.progress <= 1 ? event.progress * 100 : event.progress;
    const state = event.phase === 'generating' || event.phase === 'committing'
      ? 'generating'
      : event.phase === 'complete'
        ? 'fresh'
        : event.phase;
    this.setStatus({
      ...current,
      state,
      availability: state === 'fresh' ? 'fresh' : current.availability,
      progress: event.phase === 'generating' || event.phase === 'committing' ? progress : null,
      failure: event.phase === 'failed' ? event.message ?? current.failure : current.failure,
    });
    if (event.phase === 'failed') this.patch({ error: event.message ?? current.failure });
  }

  public async refresh(nodeId?: string): Promise<CheckpointStatus[]> {
    this.patch({ loading: true, operation: 'refresh', error: null });
    try {
      if (nodeId) {
        const next = this.setStatus(await this.platform.status(nodeId));
        this.patch({ loading: false, operation: null });
        return [next];
      }
      const statuses = (await this.platform.list()).map(normalize);
      const selected = this.state.status?.nodeId;
      this.patch({
        statuses,
        status: statuses.find((candidate) => candidate.nodeId === selected) ?? statuses[0] ?? null,
        loading: false,
        operation: null,
      });
      return statuses;
    } catch (error) {
      this.patch({ loading: false, operation: null, error: message(error) });
      throw error;
    }
  }

  public async generate(nodeId: string, outputPort: string): Promise<CheckpointStatus> {
    this.patch({ loading: true, operation: 'generate', error: null });
    try {
      const result = this.setStatus(await this.platform.generate(nodeId, outputPort));
      this.patch({
        loading: false,
        operation: null,
        error: result.failure,
      });
      return result;
    } catch (error) {
      this.patch({ loading: false, operation: null, error: message(error) });
      throw error;
    }
  }

  public async cancel(nodeId: string): Promise<CheckpointStatus> {
    this.patch({ loading: true, operation: 'cancel', error: null });
    try {
      const result = this.setStatus(await this.platform.cancel(nodeId));
      this.patch({ loading: false, operation: null, error: result.failure });
      return result;
    } catch (error) {
      this.patch({ loading: false, operation: null, error: message(error) });
      throw error;
    }
  }
}
