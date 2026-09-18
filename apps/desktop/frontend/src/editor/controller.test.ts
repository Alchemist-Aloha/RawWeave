import { describe, expect, it } from 'vitest';
import { EditorController } from './controller';
import { createMemoryPlatform } from '../platform/editor';

async function controller() {
  const editor = new EditorController(createMemoryPlatform());
  await editor.initialize();
  return editor;
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
});
