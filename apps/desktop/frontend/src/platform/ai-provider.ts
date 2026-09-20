import { invoke } from '@tauri-apps/api/core';
import type {
  AiProvider,
  AiProviderConfig,
  AiProviderPlatform,
  AiProviderStatus,
} from './ai-provider-types';

type RustAiProvider = Omit<AiProvider, 'baseUrl' | 'clientId' | 'status'> & {
  base_url?: string;
  baseUrl?: string;
  client_id?: string;
  clientId?: string;
  status?: string;
};

function message(error: unknown): Error {
  if (error instanceof Error) return error;
  if (typeof error === 'string') return new Error(error);
  try {
    return new Error(JSON.stringify(error));
  } catch {
    return new Error(String(error));
  }
}

function mapStatus(value: string | undefined): AiProviderStatus {
  return value === 'ready' || value === 'error' ? value : 'configured';
}

function mapProvider(value: RustAiProvider): AiProvider {
  return {
    ...value,
    baseUrl: value.base_url ?? value.baseUrl ?? '',
    clientId: value.client_id ?? value.clientId,
    status: mapStatus(value.status),
    error: value.error ?? null,
    capabilities: value.capabilities ?? null,
  };
}

function rustConfig(config: AiProviderConfig): Record<string, unknown> {
  return {
    id: config.id,
    name: config.name,
    kind: config.kind,
    baseUrl: config.baseUrl,
    clientId: config.clientId ?? null,
    manifest: config.manifest ?? null,
  };
}

export function createTauriAiProviderPlatform(): AiProviderPlatform {
  return {
    async list() {
      try {
        const providers = await invoke<RustAiProvider[]>('list_ai_providers');
        return providers.map(mapProvider);
      } catch (error) {
        throw message(error);
      }
    },
    async add(config) {
      try {
        const provider = await invoke<RustAiProvider>('add_ai_provider', { config: rustConfig(config) });
        return mapProvider(provider);
      } catch (error) {
        throw message(error);
      }
    },
    async remove(providerId) {
      try {
        await invoke('remove_ai_provider', { providerId });
      } catch (error) {
        throw message(error);
      }
    },
    async test(providerId) {
      try {
        const provider = await invoke<RustAiProvider>('test_ai_provider', { providerId });
        return mapProvider(provider);
      } catch (error) {
        throw message(error);
      }
    },
  };
}

export function createMemoryAiProviderPlatform(initial: AiProviderConfig[] = []): AiProviderPlatform {
  const providers = new Map<string, AiProvider>(initial.map((config) => [
    config.id,
    { ...config, status: 'configured', error: null, capabilities: null },
  ]));
  return {
    async list() {
      return [...providers.values()].map((provider) => structuredClone(provider));
    },
    async add(config) {
      if (!config.id.trim()) throw new Error('AI provider id cannot be empty');
      if (providers.has(config.id)) throw new Error(`AI provider '${config.id}' already exists`);
      const provider: AiProvider = { ...config, status: 'configured', error: null, capabilities: null };
      providers.set(config.id, provider);
      return structuredClone(provider);
    },
    async remove(providerId) {
      if (!providers.delete(providerId)) throw new Error(`AI provider '${providerId}' does not exist`);
    },
    async test(providerId) {
      const provider = providers.get(providerId);
      if (!provider) throw new Error(`AI provider '${providerId}' does not exist`);
      const ready = { ...provider, status: 'ready' as const, error: null };
      providers.set(providerId, ready);
      return structuredClone(ready);
    },
  };
}

export function createAiProviderPlatform(initial: AiProviderConfig[] = []): AiProviderPlatform {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) return createTauriAiProviderPlatform();
  return createMemoryAiProviderPlatform(initial);
}
