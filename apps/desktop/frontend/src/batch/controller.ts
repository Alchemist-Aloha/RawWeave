import type { QueueItem } from '../browser/types';
import { buildBatchJob, defaultBatchRecipe, type BatchWorkflowContext } from './model';
import type {
  BatchCreateOptions,
  BatchCheckpointPolicy,
  BatchDryRunResult,
  BatchItem,
  BatchJob,
  BatchPlatform,
  BatchPreflightOptions,
  BatchRecipe,
  BatchSessionReference,
  BatchState,
  BatchSubset,
} from './types';

export type BatchControllerListener = (state: BatchState) => void;

type IdFactory = () => string;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function defaultId(): string {
  if (typeof globalThis.crypto?.randomUUID === 'function') return globalThis.crypto.randomUUID();
  return `batch-${Date.now().toString(36)}`;
}

export class BatchController {
  public state: BatchState = {
    job: null,
    diagnostics: [],
    dryRun: null,
    loading: false,
    operation: null,
    error: null,
    openedItem: null,
  };

  private readonly listeners = new Set<BatchControllerListener>();
  private statePath: string | null = null;
  private draftRecipe: BatchRecipe = defaultBatchRecipe();
  private draftCheckpointPolicy: BatchCheckpointPolicy = 'after_each_item';

  public constructor(
    private readonly platform: BatchPlatform,
    private readonly idFactory: IdFactory = defaultId,
  ) {}

  public subscribe(listener: BatchControllerListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  public get recipe(): BatchRecipe {
    return this.state.job?.recipes[0] ?? this.draftRecipe;
  }

  public get checkpointPolicy(): BatchCheckpointPolicy {
    return this.state.job?.checkpointPolicy ?? this.draftCheckpointPolicy;
  }

  public get sessionReference(): BatchSessionReference {
    return { jobId: this.state.job?.id ?? null, statePath: this.statePath };
  }

  private publish(): void {
    for (const listener of this.listeners) listener(this.state);
  }

  private setState(patch: Partial<BatchState>): void {
    this.state = { ...this.state, ...patch };
    this.publish();
  }

  private async run<T>(operation: string, action: () => Promise<T>): Promise<T> {
    this.setState({ loading: true, operation, error: null });
    try {
      const result = await action();
      this.setState({ loading: false, operation: null, error: null });
      return result;
    } catch (error) {
      this.setState({ loading: false, operation: null, error: errorMessage(error) });
      throw error;
    }
  }

  private requireJob(): BatchJob {
    if (!this.state.job) throw new Error('create or load a batch job first');
    return this.state.job;
  }

  private setJob(job: BatchJob): void {
    this.draftCheckpointPolicy = job.checkpointPolicy;
    this.setState({ job, openedItem: null });
  }

  public async createFromQueue(
    queueItems: QueueItem[],
    workflow: BatchWorkflowContext,
    subset: BatchSubset,
    recipe: BatchRecipe = this.draftRecipe,
    options: BatchCreateOptions = {},
  ): Promise<BatchJob> {
    const job = buildBatchJob(
      queueItems,
      workflow,
      subset,
      recipe,
      this.idFactory(),
      this.draftCheckpointPolicy,
    );
    return this.run('create', async () => {
      const created = await this.platform.createJob(job, options);
      this.statePath = options.statePath ?? null;
      this.draftRecipe = created.recipes[0] ?? recipe;
      this.draftCheckpointPolicy = created.checkpointPolicy;
      this.setState({ job: created, diagnostics: [], dryRun: null, openedItem: null });
      return created;
    });
  }

  public async load(statePath: string, maxWorkers?: number): Promise<BatchJob> {
    return this.run('load', async () => {
      const job = await this.platform.loadJob(statePath, maxWorkers);
      this.statePath = statePath;
      this.draftRecipe = job.recipes[0] ?? this.draftRecipe;
      this.draftCheckpointPolicy = job.checkpointPolicy;
      this.setState({ job, diagnostics: [], dryRun: null, openedItem: null });
      return job;
    });
  }

  public async refresh(): Promise<BatchJob> {
    const job = this.requireJob();
    return this.run('refresh', async () => {
      const snapshot = await this.platform.snapshot(job.id);
      this.setJob(snapshot);
      return snapshot;
    });
  }

  public async preflight(options?: BatchPreflightOptions) {
    const job = this.requireJob();
    return this.run('preflight', async () => {
      const report = await this.platform.preflight(job.id, options);
      this.setState({ diagnostics: report.diagnostics });
      return report;
    });
  }

  public async dryRun(subset: BatchSubset): Promise<BatchDryRunResult> {
    const job = this.requireJob();
    return this.run('dry-run', async () => {
      const result = await this.platform.dryRun(job.id, subset);
      this.setState({ dryRun: result });
      return result;
    });
  }

  public async start(): Promise<BatchJob> {
    const job = this.requireJob();
    return this.run('start', async () => {
      const result = await this.platform.start(job.id);
      this.setJob(result);
      return result;
    });
  }

  public async pause(): Promise<BatchJob> {
    const job = this.requireJob();
    return this.run('pause', async () => {
      const result = await this.platform.pause(job.id);
      this.setJob(result);
      return result;
    });
  }

  public async resume(): Promise<BatchJob> {
    const job = this.requireJob();
    return this.run('resume', async () => {
      const result = await this.platform.resume(job.id);
      this.setJob(result);
      return result;
    });
  }

  public async cancel(): Promise<BatchJob> {
    const job = this.requireJob();
    return this.run('cancel', async () => {
      const result = await this.platform.cancel(job.id);
      this.setJob(result);
      return result;
    });
  }

  public async retryFailed(): Promise<number> {
    const job = this.requireJob();
    return this.run('retry-failed', async () => {
      const count = await this.platform.retryFailed(job.id);
      await this.refreshAfterOperation(job.id);
      return count;
    });
  }

  public async retrySelected(itemIds: string[]): Promise<number> {
    const job = this.requireJob();
    return this.run('retry-selected', async () => {
      const count = await this.platform.retrySelected(job.id, [...itemIds]);
      await this.refreshAfterOperation(job.id);
      return count;
    });
  }

  public async skip(itemIds: string[]): Promise<number> {
    const job = this.requireJob();
    return this.run('skip', async () => {
      const count = await this.platform.skip(job.id, [...itemIds]);
      await this.refreshAfterOperation(job.id);
      return count;
    });
  }

  public async openFailedItem(itemId: string): Promise<BatchItem> {
    const job = this.requireJob();
    return this.run('open-failed', async () => {
      const item = await this.platform.openFailedItem(job.id, itemId);
      this.setState({ openedItem: item });
      return item;
    });
  }

  public updateRecipe(patch: Partial<BatchRecipe>): BatchRecipe {
    const next = { ...this.recipe, ...patch };
    this.draftRecipe = next;
    if (this.state.job) {
      this.setState({ job: { ...this.state.job, recipes: [next, ...this.state.job.recipes.slice(1)] } });
    } else {
      this.publish();
    }
    return next;
  }

  public updateCheckpointPolicy(policy: BatchCheckpointPolicy): BatchCheckpointPolicy {
    this.draftCheckpointPolicy = policy;
    if (this.state.job) {
      this.setState({ job: { ...this.state.job, checkpointPolicy: policy } });
    } else {
      this.publish();
    }
    return policy;
  }

  public clearError(): void {
    if (this.state.error) this.setState({ error: null });
  }

  private async refreshAfterOperation(jobId: string): Promise<void> {
    const snapshot = await this.platform.snapshot(jobId);
    this.setJob(snapshot);
  }
}
