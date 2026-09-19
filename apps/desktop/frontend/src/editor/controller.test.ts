import { describe, expect, it } from 'vitest';
import { EditorController } from './controller';
import { createMemoryPlatform } from '../platform/editor';
import type { EditorPlatform, OpenImageResult } from './types';

async function controller() {
  const editor = new EditorController(createMemoryPlatform());
  await editor.initialize();
  return editor;
}

function rawOpenResult(): OpenImageResult {
  return {
    kind: 'raw',
    width: 4,
    height: 2,
    revision: 17,
    metadata: {
      camera: 'Canon EOS R5',
      lens: null,
      iso: null,
      aperture: null,
      shutter: null,
      focalLength: null,
      captureTime: null,
      orientation: 'Normal',
      dimensions: { width: 4, height: 2 },
      exif: {},
    },
  };
}

async function rawPlatform(): Promise<{ platform: EditorPlatform; result: OpenImageResult }> {
  const base = createMemoryPlatform();
  const result = rawOpenResult();
  const platform: EditorPlatform = {
    ...base,
    async openImage() {
      const existing = await base.snapshot();
      for (const node of existing.nodes) await base.removeNode(node.id);
      for (const [id, typeId] of [
        ['raw-decode', 'raw.decode'],
        ['black-level', 'raw.black-level'],
        ['white-balance', 'raw.white-balance'],
        ['highlight-reconstruction', 'raw.highlight-reconstruction'],
        ['demosaic', 'raw.demosaic'],
        ['camera-transform', 'raw.camera-transform'],
        ['lens-correction', 'raw.lens-correction'],
        ['display-transform', 'raw.display-transform'],
      ]) {
        await base.addNode(id, typeId);
      }
      for (const [fromNode, fromPort, toNode, toPort] of [
        ['raw-decode', 'frame', 'black-level', 'frame'],
        ['black-level', 'mosaic', 'white-balance', 'mosaic'],
        ['white-balance', 'mosaic', 'highlight-reconstruction', 'mosaic'],
        ['highlight-reconstruction', 'mosaic', 'demosaic', 'mosaic'],
        ['demosaic', 'scene', 'camera-transform', 'scene'],
        ['raw-decode', 'camera_profile', 'camera-transform', 'camera_profile'],
        ['camera-transform', 'scene', 'lens-correction', 'scene'],
        ['raw-decode', 'lens_profile', 'lens-correction', 'lens_profile'],
        ['lens-correction', 'scene', 'display-transform', 'scene'],
      ]) {
        await base.connect(fromNode, fromPort, toNode, toPort);
      }
      return result;
    },
  };
  return { platform, result };
}

describe('editor controller', () => {
  it('creates a node from the node library', async () => {
    const editor = await controller();

    await editor.createNode('core.image-input');

    expect(editor.state.nodes).toHaveLength(1);
    expect(editor.state.nodes[0].typeId).toBe('core.image-input');
  });

  it('connects nodes and keeps the connection in the graph view', async () => {
    const editor = await controller();
    await editor.createNode('core.image-input', 'input');
    await editor.createNode('core.output', 'output');

    await editor.connect('input', 'image', 'output', 'image');

    expect(editor.state.edges).toEqual([
      expect.objectContaining({
        source: 'input',
        sourceHandle: 'image',
        target: 'output',
        targetHandle: 'image',
      }),
    ]);
  });

  it('changes a selected node parameter through the platform command', async () => {
    const editor = await controller();
    await editor.createNode('core.exposure', 'exposure');

    await editor.setParameter('exposure', 'exposure', 1.25);

    expect(editor.state.nodes[0].parameters.exposure).toBe(1.25);
  });

  it('exposes a parameter as a connectable input port', async () => {
    const editor = await controller();
    await editor.createNode('core.blur', 'blur');

    await editor.exposeParameter('blur', 'radius');

    expect(editor.state.nodes[0].exposedParameters).toEqual(['radius']);
  });

  it('routes a value into an exposed parameter and restores the literal when hidden', async () => {
    const editor = await controller();
    await editor.createNode('core.blur', 'blur');
    await editor.createNode('core.constant-float', 'constant');
    await editor.setParameter('blur', 'radius', 2);
    await editor.exposeParameter('blur', 'radius');

    await editor.connect('constant', 'value', 'blur', 'radius');

    expect(editor.state.edges).toHaveLength(1);
    // The stored literal survives the temporary connection.
    expect(editor.state.nodes.find((node) => node.id === 'blur')?.parameters.radius).toBe(2);

    await editor.unexposeParameter('blur', 'radius');

    expect(editor.state.edges).toHaveLength(0);
    expect(editor.state.nodes.find((node) => node.id === 'blur')?.parameters.radius).toBe(2);
    expect(editor.state.nodes.find((node) => node.id === 'blur')?.exposedParameters).toEqual([]);
  });

  it('rejects a type-mismatched connection to an exposed parameter', async () => {
    const editor = await controller();
    await editor.createNode('core.blur', 'blur');
    await editor.createNode('core.image-input', 'input');
    await editor.exposeParameter('blur', 'radius');

    await expect(editor.connect('input', 'image', 'blur', 'radius')).rejects.toThrow();
    expect(editor.state.error).toMatch(/cannot connect|type/i);
  });

  it('rejects exposing an unknown parameter', async () => {
    const editor = await controller();
    await editor.createNode('core.blur', 'blur');

    await expect(editor.exposeParameter('blur', 'missing')).rejects.toThrow();
  });

  it('saves and loads workflow state without losing topology', async () => {
    const editor = await controller();
    await editor.createNode('core.image-input', 'input');
    await editor.createNode('core.output', 'output');
    await editor.connect('input', 'image', 'output', 'image');
    const saved = await editor.saveWorkflow();

    const reopened = await controller();
    await reopened.loadWorkflow(saved);

    expect(reopened.state.nodes.map((node) => node.id)).toEqual(['input', 'output']);
    expect(reopened.state.edges).toHaveLength(1);
  });

  it('exposes backend errors without throwing from the UI command', async () => {
    const editor = await controller();
    await editor.createNode('core.constant-float', 'constant');
    await editor.createNode('core.output', 'output');

    await expect(editor.connect('constant', 'value', 'output', 'image')).rejects.toThrow();
    expect(editor.state.error).toMatch(/cannot connect|type/i);
  });

  it('opens RAW sources and synchronizes the default RAW graph', async () => {
    const { platform, result } = await rawPlatform();
    const editor = new EditorController(platform);
    await editor.initialize();

    await expect(editor.openImage('fixture.dng')).resolves.toEqual(result);

    expect(editor.state.source).toEqual(result);
    expect(editor.state.descriptors.filter((descriptor) => descriptor.typeId.startsWith('raw.'))).toHaveLength(8);
    expect(editor.state.nodes).toHaveLength(8);
    expect(editor.state.nodes.map((node) => node.typeId)).toEqual([
      'raw.decode',
      'raw.black-level',
      'raw.white-balance',
      'raw.highlight-reconstruction',
      'raw.demosaic',
      'raw.camera-transform',
      'raw.lens-correction',
      'raw.display-transform',
    ]);
    expect(editor.state.edges).toHaveLength(9);
    expect(editor.state.revision).toBe(result.revision);
  });

  it('notifies that a loaded RAW workflow needs its source selected again', async () => {
    const { platform, result } = await rawPlatform();
    const editor = new EditorController(platform);
    await editor.initialize();
    await editor.openImage('fixture.dng');
    const workflow = await editor.saveWorkflow();

    await editor.loadWorkflow(workflow);

    expect(editor.state.source).toBeNull();
    expect(editor.state.notification).toMatch(/select the source RAW file again/i);

    await expect(editor.openImage('fixture.dng')).resolves.toEqual(result);
    expect(editor.state.source).toEqual(result);
    expect(editor.state.nodes).toHaveLength(8);
    expect(editor.state.notification).toMatch(/display transform is ready/i);
  });

  it('registers the Step 4 control and logic nodes', async () => {
    const editor = await controller();
    const typeIds = editor.state.descriptors.map((descriptor) => descriptor.typeId);
    for (const typeId of [
      'core.metadata',
      'core.switch',
      'core.select',
      'core.enum-select',
      'core.compare',
      'core.curve',
      'core.expression',
      'core.map-range',
      'core.constant-integer',
      'core.constant-boolean',
      'core.constant-string',
    ]) {
      expect(typeIds).toContain(typeId);
    }
  });

  it('opening an ordinary image resets the graph to the standard image workflow', async () => {
    const editor = new EditorController(createMemoryPlatform());
    await editor.initialize();

    const rawEditor = await rawPlatform();
    const rawController = new EditorController(rawEditor.platform);
    await rawController.initialize();
    await rawController.openImage('fixture.dng');

    const ordinary = await editor.openImage('photo.png');

    expect(ordinary.kind).toBe('ordinary');
    expect(editor.state.nodes.map((node) => node.typeId)).toEqual([
      'core.image-input',
      'core.output',
    ]);
    expect(editor.state.nodes.every((node) => !node.typeId.startsWith('raw.'))).toBe(true);
    expect(editor.state.edges).toHaveLength(1);
  });
});
