import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { createTauriHostManager } from './hosts';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const mockedInvoke = vi.mocked(invoke);

const host = {
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

describe('tauri host manager adapter', () => {
  beforeEach(() => vi.resetAllMocks());

  it('lists hosts through the typed camel-case DTO', async () => {
    mockedInvoke.mockResolvedValue([host]);
    const manager = createTauriHostManager();

    await expect(manager.list()).resolves.toEqual([host]);
    expect(mockedInvoke).toHaveBeenCalledWith('list_external_hosts');
  });

  it('passes bounded host configuration to add and maps errors to Error', async () => {
    mockedInvoke.mockResolvedValue(host);
    const manager = createTauriHostManager();
    const config = {
      id: 'fixture',
      executable: 'python3',
      args: ['fixture.py'],
      envAllowlist: [],
      environment: {},
    };

    await expect(manager.add(config)).resolves.toEqual(host);
    expect(mockedInvoke).toHaveBeenCalledWith('add_external_host', { config });

    mockedInvoke.mockRejectedValueOnce('missing host');
    await expect(manager.test('missing')).rejects.toEqual(new Error('missing host'));
  });

  it('discovers all hosts for a refresh and removes by host id', async () => {
    mockedInvoke.mockResolvedValue([host]);
    const manager = createTauriHostManager();

    await manager.discoverAll();
    expect(mockedInvoke).toHaveBeenCalledWith('discover_external_hosts');
    await manager.remove('fixture');
    expect(mockedInvoke).toHaveBeenLastCalledWith('remove_external_host', { hostId: 'fixture' });
  });
});
