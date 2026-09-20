import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AiProviderManager } from './AiProviderManager';
import type { AiProvider, AiProviderPlatform } from '../platform/ai-provider-types';

const provider: AiProvider = {
  id: 'local-comfy',
  name: 'Local ComfyUI',
  kind: 'comfy_ui',
  baseUrl: 'http://127.0.0.1:8188',
  clientId: 'rawweave',
  status: 'ready',
  capabilities: null,
  error: null,
};

let root: Root | null = null;
let container: HTMLDivElement | null = null;

afterEach(() => {
  if (root) act(() => root?.unmount());
  root = null;
  container?.remove();
  container = null;
});

describe('AiProviderManager', () => {
  it('lists providers, exposes provider configuration, and tests a provider', async () => {
    const api: AiProviderPlatform = {
      list: vi.fn().mockResolvedValue([provider]),
      add: vi.fn(),
      remove: vi.fn(),
      test: vi.fn().mockResolvedValue(provider),
    };
    container = document.createElement('div');
    document.body.append(container);

    await act(async () => {
      root = createRoot(container!);
      root.render(<AiProviderManager api={api} />);
    });

    expect(container.textContent).toContain('AI providers');
    expect(container.textContent).toContain('Local ComfyUI');
    expect(container.querySelector('[aria-label="Test AI provider local-comfy"]')).not.toBeNull();
    expect(container.querySelector('[aria-label="AI provider id"]')).not.toBeNull();

    await act(async () => {
      (container!.querySelector('[aria-label="Test AI provider local-comfy"]') as HTMLButtonElement).click();
    });
    expect(api.test).toHaveBeenCalledWith('local-comfy');
  });
});
