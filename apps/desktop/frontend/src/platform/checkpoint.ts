import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  CheckpointGeneration,
  CheckpointPlatform,
  CheckpointProgressEvent,
  CheckpointProvenance,
  CheckpointState,
  CheckpointStatus,
} from '../checkpoint/types';

interface RustCheckpointStatus {
  node_id?: string;
  nodeId?: string;
  output_port?: string;
  outputPort?: string;
  state: string;
  availability?: string;
  current_dependency_hash?: string | null;
  currentDependencyHash?: string | null;
  committed_dependency_hash?: string | null;
  committedDependencyHash?: string | null;
  committed_artifact_id?: string | null;
  committedArtifactId?: string | null;
  generation?: RustCheckpointGeneration | null;
  provenance?: RustCheckpointProvenance | null;
  failure?: string | null;
  progress?: number | null;
  can_use_committed?: boolean;
  canUseCommitted?: boolean;
}

interface RustCheckpointGeneration {
  generation_revision?: number;
  generationRevision?: number;
  generated_at?: string | null;
  generatedAt?: string | null;
  duration_millis?: number | null;
  durationMillis?: number | null;
  generator?: string | null;
}

interface RustCheckpointProvenance {
  dependency_hash?: string;
  dependencyHash?: string;
  upstream_hashes?: Record<string, string>;
  upstreamHashes?: Record<string, string>;
  node_version?: number;
  nodeVersion?: number;
  external_tool?: CheckpointProvenance['externalTool'];
  externalTool?: CheckpointProvenance['externalTool'];
}

interface RustCheckpointProgress {
  node_id?: string;
  nodeId?: string;
  output_port?: string;
  outputPort?: string;
  progress: number;
  phase: CheckpointProgressEvent['phase'];
  message?: string | null;
}

function message(error: unknown): Error {
  if (error instanceof Error) return error;
  return new Error(typeof error === 'string' ? error : JSON.stringify(error));
}

function mapGeneration(value: RustCheckpointGeneration | null | undefined): CheckpointGeneration | null {
  if (!value) return null;
  return {
    generationRevision: value.generation_revision ?? value.generationRevision ?? 0,
    generatedAt: value.generated_at ?? value.generatedAt ?? null,
    durationMillis: value.duration_millis ?? value.durationMillis ?? null,
    generator: value.generator ?? null,
  };
}

function mapProvenance(value: RustCheckpointProvenance | null | undefined): CheckpointProvenance | null {
  if (!value) return null;
  return {
    dependencyHash: value.dependency_hash ?? value.dependencyHash ?? '',
    upstreamHashes: value.upstream_hashes ?? value.upstreamHashes ?? {},
    nodeVersion: value.node_version ?? value.nodeVersion ?? 0,
    externalTool: value.external_tool ?? value.externalTool ?? null,
  };
}

function mapState(value: string): CheckpointState {
  switch (value) {
    case 'current': return 'fresh';
    case 'generating': return 'generating';
    case 'stale': return 'stale';
    case 'failed': return 'failed';
    case 'cancelled': return 'cancelled';
    default: return 'ungenerated';
  }
}

function mapStatus(value: RustCheckpointStatus): CheckpointStatus {
  const committedArtifactId = value.committed_artifact_id ?? value.committedArtifactId ?? null;
  const availability = value.availability === 'fresh' || value.availability === 'stale' || value.availability === 'missing' || value.availability === 'incompatible'
    ? value.availability
    : committedArtifactId ? 'stale' : 'missing';
  return {
    nodeId: value.node_id ?? value.nodeId ?? '',
    outputPort: value.output_port ?? value.outputPort ?? 'image',
    state: mapState(value.state),
    availability,
    currentDependencyHash: value.current_dependency_hash ?? value.currentDependencyHash ?? null,
    committedDependencyHash: value.committed_dependency_hash ?? value.committedDependencyHash ?? null,
    committedArtifactId,
    generation: mapGeneration(value.generation),
    provenance: mapProvenance(value.provenance),
    failure: value.failure ?? null,
    progress: value.progress ?? null,
    canUseCommitted: value.can_use_committed ?? value.canUseCommitted ?? Boolean(committedArtifactId),
  };
}

export function createTauriCheckpointPlatform(): CheckpointPlatform {
  const platform: CheckpointPlatform = {
    async list() {
      try {
        return (await invoke<RustCheckpointStatus[]>('checkpoint_list')).map(mapStatus);
      } catch (error) {
        throw message(error);
      }
    },
    async status(nodeId) {
      try {
        return mapStatus(await invoke<RustCheckpointStatus>('checkpoint_status', { nodeId }));
      } catch (error) {
        throw message(error);
      }
    },
    async generate(nodeId, outputPort) {
      try {
        return mapStatus(await invoke<RustCheckpointStatus>('generate_checkpoint', { nodeId, outputPort }));
      } catch (error) {
        throw message(error);
      }
    },
    async cancel(nodeId) {
      try {
        return mapStatus(await invoke<RustCheckpointStatus>('cancel_checkpoint', { nodeId }));
      } catch (error) {
        throw message(error);
      }
    },
    subscribeProgress(listener) {
      let active = true;
      let unlisten: (() => void) | null = null;
      void listen<RustCheckpointProgress>('checkpoint-progress', (event) => {
        const payload = event.payload;
        const nodeId = payload.node_id ?? payload.nodeId;
        const outputPort = payload.output_port ?? payload.outputPort;
        if (!nodeId || !outputPort) return;
        listener({
          nodeId,
          outputPort,
          progress: payload.progress,
          phase: payload.phase,
          message: payload.message,
        });
      }).then((remove) => {
        if (active) unlisten = remove;
        else remove();
      }).catch(() => undefined);
      return () => {
        active = false;
        unlisten?.();
      };
    },
  };
  return platform;
}

export function createMemoryCheckpointPlatform(initial: CheckpointStatus[] = []): CheckpointPlatform {
  const statuses = new Map(initial.map((status) => [status.nodeId, structuredClone(status)]));
  const listeners = new Set<(event: CheckpointProgressEvent) => void>();
  return {
    async list() { return [...statuses.values()].map((status) => structuredClone(status)); },
    async status(nodeId) {
      const status = statuses.get(nodeId);
      if (!status) throw new Error(`checkpoint '${nodeId}' is not registered`);
      return structuredClone(status);
    },
    async generate(nodeId, outputPort) {
      const current = statuses.get(nodeId) ?? {
        nodeId, outputPort, state: 'ungenerated' as const, availability: 'missing' as const,
        currentDependencyHash: null, committedDependencyHash: null, committedArtifactId: null,
        generation: null, provenance: null, failure: null, progress: null,
      };
      const generating = { ...current, outputPort, state: 'generating' as const, progress: 0, failure: null };
      statuses.set(nodeId, generating);
      listeners.forEach((listener) => listener({ nodeId, outputPort, progress: 0, phase: 'generating' }));
      const fresh = {
        ...generating,
        state: 'fresh' as const,
        availability: 'fresh' as const,
        progress: null,
        committedArtifactId: generating.committedArtifactId ?? `memory:${nodeId}`,
        committedDependencyHash: generating.currentDependencyHash ?? 'memory-dependency',
        currentDependencyHash: generating.currentDependencyHash ?? 'memory-dependency',
        generation: generating.generation ?? { generationRevision: 1 },
        canUseCommitted: true,
      };
      statuses.set(nodeId, fresh);
      listeners.forEach((listener) => listener({ nodeId, outputPort, progress: 100, phase: 'complete' }));
      return structuredClone(fresh);
    },
    async cancel(nodeId) {
      const current = statuses.get(nodeId);
      if (!current) throw new Error(`checkpoint '${nodeId}' is not registered`);
      const cancelled = { ...current, state: 'cancelled' as const, progress: null };
      statuses.set(nodeId, cancelled);
      listeners.forEach((listener) => listener({ nodeId, outputPort: current.outputPort, progress: 0, phase: 'cancelled' }));
      return structuredClone(cancelled);
    },
    subscribeProgress(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

export function createCheckpointPlatform(initial: CheckpointStatus[] = []): CheckpointPlatform {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) return createTauriCheckpointPlatform();
  return createMemoryCheckpointPlatform(initial);
}
