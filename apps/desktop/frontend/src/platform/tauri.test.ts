import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { createTauriPlatform } from './tauri';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

const mockedInvoke = vi.mocked(invoke);

function definition(defaultValue: number) {
  return {
    identity: { id: 'nested.look', version: '1.0.0' },
    graph: {
      nodes: [{
        id: 'exposure',
        typeId: 'core.exposure',
        parameters: { exposure: { Float: defaultValue } },
        exposedParameters: [],
      }],
      edges: [],
      revision: 1,
    },
    parameters: [{
      id: 'exposure:exposure',
      name: 'Exposure',
      nodeId: 'exposure',
      parameterId: 'exposure',
      parameterType: 'Float',
      default: { Float: defaultValue },
    }],
    inputs: [],
    outputs: [],
    subgraphDependencies: [],
    nodePackDependencies: [],
    metadata: { name: 'Nested look', tags: [] },
    nestedSubgraphs: {},
    hash: 'backend-hash',
  };
}

describe('tauri platform nested blueprint synchronization', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it('refreshes the active definition after a nested graph mutation', async () => {
    let saved = definition(1);
    mockedInvoke.mockImplementation(async (command) => {
      switch (command) {
        case 'load_blueprint':
          return saved;
        case 'set_node_parameter':
          saved = definition(2);
          return undefined;
        case 'save_blueprint':
          return JSON.stringify(saved);
        case 'save_workflow':
          return JSON.stringify(saved.graph);
        case 'dependency_status':
          return { available: [], missing: [], mismatched: [], disabledNodes: [], statuses: {} };
        case 'workflow_hash':
          return 'backend-hash';
        default:
          throw new Error(`unexpected command ${String(command)}`);
      }
    });

    const platform = createTauriPlatform();
    await platform.loadBlueprint(JSON.stringify(saved));
    await platform.setParameter('exposure', 'exposure', 2);

    const snapshot = await platform.snapshot();
    expect(snapshot.workflowParameters?.[0]?.default).toBe(2);
    expect(mockedInvoke).toHaveBeenCalledWith('save_blueprint');
  });

  it('opens an ordered ImageSet through the native adapter without sending image bytes', async () => {
    mockedInvoke.mockResolvedValue({
      kind: 'imageset',
      order: 'ordered',
      revision: 4,
      members: [{ id: '/photos/a.jpg', path: '/photos/a.jpg', name: 'a.jpg', width: 10, height: 10, metadata: null }],
      sharedMetadata: null,
      alignment: { state: 'unaligned' },
    });
    const platform = createTauriPlatform();
    const result = await platform.openImageSet(['/photos/a.jpg'], 'ordered');

    expect(result.members[0]?.id).toBe('/photos/a.jpg');
    expect(mockedInvoke).toHaveBeenCalledWith('open_image_set', { paths: ['/photos/a.jpg'], order: 'ordered' });
  });
});
