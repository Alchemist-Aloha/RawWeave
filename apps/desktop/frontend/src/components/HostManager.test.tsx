import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { HostManager } from './HostManager';
import type { ExternalHost, HostManagerApi } from '../platform/hosts';

const host: ExternalHost = {
  id: 'fixture',
  executable: 'python3',
  args: ['fixture.py'],
  envAllowlist: [],
  status: 'ready',
  protocolVersion: { major: 1, minor: 0 },
  capabilities: {
    pixelFormats: ['Rgba32Float'],
    roi: false,
    fullFrame: true,
    multiInput: false,
    multiOutput: false,
    threadSafety: 'SingleThreaded',
    gpu: false,
    customUi: false,
    deterministic: true,
    dataPlane: true,
  },
  nodes: [{ typeId: 'external.fixture.node', name: 'Fixture', version: 1 }],
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

describe('HostManager', () => {
  it('shows status, capabilities, discovered nodes, and host actions', async () => {
    const api: HostManagerApi = {
      list: vi.fn().mockResolvedValue([host]),
      add: vi.fn(),
      remove: vi.fn(),
      test: vi.fn().mockResolvedValue(host),
      discover: vi.fn().mockResolvedValue(host),
      discoverAll: vi.fn().mockResolvedValue([host]),
      diagnostics: vi.fn(),
    };
    container = document.createElement('div');
    document.body.append(container);

    await act(async () => {
      root = createRoot(container!);
      root.render(<HostManager api={api} />);
    });

    expect(container.textContent).toContain('External hosts');
    expect(container.textContent).toContain('ready');
    expect(container.textContent).toContain('Protocol 1.0');
    expect(container.textContent).toContain('Fixture');
    expect(container.querySelector('[aria-label="Test host fixture"]')).not.toBeNull();
    expect(container.querySelector('[aria-label="Discover host fixture"]')).not.toBeNull();
    expect(container.querySelector('[aria-label="Remove host fixture"]')).not.toBeNull();
  });

  it('refreshes discovery and exposes the add form', async () => {
    const api: HostManagerApi = {
      list: vi.fn().mockResolvedValue([]),
      add: vi.fn().mockResolvedValue(host),
      remove: vi.fn(),
      test: vi.fn(),
      discover: vi.fn(),
      discoverAll: vi.fn().mockResolvedValue([host]),
      diagnostics: vi.fn(),
    };
    container = document.createElement('div');
    document.body.append(container);

    await act(async () => {
      root = createRoot(container!);
      root.render(<HostManager api={api} />);
    });
    const refresh = container.querySelector('[aria-label="Refresh external hosts"]') as HTMLButtonElement;

    await act(async () => refresh.click());

    expect(api.discoverAll).toHaveBeenCalledOnce();
    expect(container.querySelector('[aria-label="Host id"]')).not.toBeNull();
    expect(container.querySelector('[aria-label="Host executable"]')).not.toBeNull();
    expect(container.querySelector('[aria-label="Host arguments"]')).not.toBeNull();
  });
});
