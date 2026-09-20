export type CheckpointState = 'ungenerated' | 'generating' | 'fresh' | 'stale' | 'failed' | 'cancelled';
export type CheckpointAvailability = 'fresh' | 'stale' | 'missing' | 'incompatible';

export interface CheckpointGeneration {
  generationRevision: number;
  generatedAt?: string | null;
  durationMillis?: number | null;
  generator?: string | null;
}

export interface CheckpointProvenance {
  dependencyHash: string;
  upstreamHashes: Record<string, string>;
  nodeVersion: number;
  externalTool?: {
    id: string;
    version: string;
    model?: string | null;
    parameters?: Record<string, string>;
  } | null;
}

export interface CheckpointStatus {
  nodeId: string;
  outputPort: string;
  state: CheckpointState;
  availability: CheckpointAvailability;
  currentDependencyHash: string | null;
  committedDependencyHash: string | null;
  committedArtifactId: string | null;
  generation: CheckpointGeneration | null;
  provenance: CheckpointProvenance | null;
  failure: string | null;
  progress: number | null;
  canUseCommitted?: boolean;
}

export interface CheckpointProgressEvent {
  nodeId: string;
  outputPort: string;
  progress: number;
  phase: 'generating' | 'committing' | 'complete' | 'failed' | 'cancelled';
  message?: string | null;
  /** The backend may report a state that is more authoritative than the phase. */
  state?: CheckpointState;
  availability?: CheckpointAvailability;
}

export interface CheckpointPlatform {
  list(): Promise<CheckpointStatus[]>;
  status(nodeId: string): Promise<CheckpointStatus>;
  generate(nodeId: string, outputPort: string): Promise<CheckpointStatus>;
  cancel(nodeId: string): Promise<CheckpointStatus>;
  subscribeProgress?(listener: (event: CheckpointProgressEvent) => void): () => void;
}

export interface CheckpointControllerState {
  statuses: CheckpointStatus[];
  status: CheckpointStatus | null;
  loading: boolean;
  operation: 'refresh' | 'generate' | 'cancel' | null;
  error: string | null;
}
