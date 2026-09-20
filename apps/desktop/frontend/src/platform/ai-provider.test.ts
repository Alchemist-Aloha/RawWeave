import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  createMemoryAiProviderPlatform,
  createTauriAiProviderPlatform,
} from './ai-provider';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const mockedInvoke = vi.mocked(invoke);

const comfy = {
  id: 'local-comfy',
  name: 'Local ComfyUI',
  kind: 'comfy_ui' as const,
  baseUrl: 'http://127.0.0.1:8188',
  clientId: 'rawweave',
};

describe('AI provider platform', () => {
  beforeEach(() => vi.resetAllMocks());

  it('keeps provider configuration separate from workflow data in memory', async () => {
    const platform = createMemoryAiProviderPlatform([comfy]);

    await expect(platform.list()).resolves.toMatchObject([comfy]);
    await expect(platform.add({
      id: 'lan-comfy',
      name: 'LAN ComfyUI',
      kind: 'comfy_ui',
      baseUrl: 'http://192.168.1.50:8188',
      clientId: 'rawweave',
    })).resolves.toMatchObject({ id: 'lan-comfy', status: 'configured' });
    await expect(platform.list()).resolves.toHaveLength(2);
    await expect(platform.remove('lan-comfy')).resolves.toBeUndefined();
    await expect(platform.list()).resolves.toMatchObject([comfy]);
  });

  it('maps tauri provider commands and never asks the frontend to hold secrets', async () => {
    mockedInvoke
      .mockResolvedValueOnce([{ ...comfy, status: 'ready', capabilities: [] }])
      .mockResolvedValueOnce({ ...comfy, status: 'ready', capabilities: [] });
    const platform = createTauriAiProviderPlatform();

    await expect(platform.list()).resolves.toMatchObject([{ id: 'local-comfy', status: 'ready' }]);
    expect(mockedInvoke).toHaveBeenCalledWith('list_ai_providers');
    expect(JSON.stringify(mockedInvoke.mock.calls)).not.toContain('apiKey');

    await platform.test('local-comfy');
    expect(mockedInvoke).toHaveBeenLastCalledWith('test_ai_provider', { providerId: 'local-comfy' });
  });
});
