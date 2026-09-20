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
  private operationEpoch = 0;
  private activeGeneration: { epoch: number; nodeId: string; outputPort: string } | null = null;

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

  public clearError(): void {
    if (this.state.error) this.patch({ error: null });
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
    const generation = this.activeGeneration;
    if (
      !generation
      || generation.nodeId !== event.nodeId
      || generation.outputPort !== event.outputPort
    ) return;
    const current = this.state.statuses.find((candidate) => candidate.nodeId === event.nodeId);
    if (!current || current.outputPort !== event.outputPort) return;
    const progress = event.progress <= 1 ? event.progress * 100 : event.progress;
    const inferredState = event.phase === 'generating' || event.phase === 'committing'
      ? 'generating'
      : event.phase === 'complete'
        ? 'fresh'
        : event.phase;
    const state = event.state ?? inferredState;
    this.setStatus({
      ...current,
      state,
      availability: event.availability ?? (state === 'fresh' ? 'fresh' : current.availability),
      progress: event.phase === 'generating' || event.phase === 'committing' ? progress : null,
      failure: event.phase === 'failed' ? event.message ?? current.failure : current.failure,
    });
    if (event.phase === 'failed') this.patch({ error: event.message ?? current.failure });
  }

  public async refresh(nodeId?: string): Promise<CheckpointStatus[]> {
    const epoch = ++this.operationEpoch;
    this.activeGeneration = null;
    this.patch({ loading: true, operation: 'refresh', error: null });
    try {
      if (nodeId) {
        const next = normalize(await this.platform.status(nodeId));
        if (epoch !== this.operationEpoch) return [];
        this.setStatus(next);
        this.patch({ loading: false, operation: null });
        return [next];
      }
      const statuses = (await this.platform.list()).map(normalize);
      if (epoch !== this.operationEpoch) return [];
      const selected = this.state.status?.nodeId;
      this.patch({
        statuses,
        status: statuses.find((candidate) => candidate.nodeId === selected) ?? statuses[0] ?? null,
        loading: false,
        operation: null,
      });
      return statuses;
    } catch (error) {
      if (epoch !== this.operationEpoch) throw error;
      this.patch({ loading: false, operation: null, error: message(error) });
      throw error;
    }
  }

  public async generate(nodeId: string, outputPort: string): Promise<CheckpointStatus> {
    const epoch = ++this.operationEpoch;
    this.activeGeneration = { epoch, nodeId, outputPort };
    this.patch({ loading: true, operation: 'generate', error: null });
    try {
      const result = normalize(await this.platform.generate(nodeId, outputPort));
      if (epoch !== this.operationEpoch) return result;
      this.setStatus(result);
      this.patch({
        loading: false,
        operation: null,
        error: result.failure,
      });
      return result;
    } catch (error) {
      if (epoch !== this.operationEpoch) throw error;
      this.patch({ loading: false, operation: null, error: message(error) });
      throw error;
    } finally {
      if (this.activeGeneration?.epoch === epoch) this.activeGeneration = null;
    }
  }

  public async cancel(nodeId: string): Promise<CheckpointStatus> {
    const epoch = ++this.operationEpoch;
    this.activeGeneration = null;
    this.patch({ loading: true, operation: 'cancel', error: null });
    try {
      const result = normalize(await this.platform.cancel(nodeId));
      if (epoch !== this.operationEpoch) return result;
      this.setStatus(result);
      this.patch({ loading: false, operation: null, error: result.failure });
      return result;
    } catch (error) {
      if (epoch !== this.operationEpoch) throw error;
      this.patch({ loading: false, operation: null, error: message(error) });
      throw error;
    }
  }
}
