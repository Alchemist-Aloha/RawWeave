import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { describe, expect, it, vi } from 'vitest';
import { NodeLibrary } from './NodeLibrary';
import type { NodeDescriptor } from '../editor/types';

const descriptors: NodeDescriptor[] = [
  {
    typeId: 'core.exposure', name: 'Exposure', version: 1,
    inputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: true }],
    outputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: false }],
    parameters: [],
  },
  {
    typeId: 'core.constant', name: 'Constant', version: 1,
    inputs: [], outputs: [{ id: 'value', name: 'Value', dataType: 'value.Float', required: false }],
    parameters: [],
  },
];

describe('NodeLibrary', () => {
  it('creates the first filtered node from the keyboard', async () => {
    const onAdd = vi.fn();
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => root.render(<NodeLibrary descriptors={descriptors} onAdd={onAdd} />));

    const search = host.querySelector<HTMLInputElement>('input[type="search"]');
    expect(search).not.toBeNull();
    await act(async () => {
      search?.focus();
      search?.dispatchEvent(new InputEvent('input', { bubbles: true }));
      search?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    });

    expect(onAdd).toHaveBeenCalledWith('core.exposure');
    await act(async () => root.unmount());
    host.remove();
  });

  it('can narrow creation results to compatible input data types', async () => {
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => root.render(
      <NodeLibrary descriptors={descriptors} onAdd={vi.fn()} compatibleDataTypes={['core.Image']} />,
    ));

    expect(host.textContent).toContain('Exposure');
    expect(host.textContent).not.toContain('Constant');
    expect(host.querySelector('[aria-label="Compatible nodes only"]')).not.toBeNull();
    await act(async () => root.unmount());
    host.remove();
  });

  it('treats wildcard and numeric output types as compatible inputs', async () => {
    const wildcardDescriptor: NodeDescriptor = {
      typeId: 'core.any-consumer', name: 'Any Consumer', version: 1,
      inputs: [{ id: 'value', name: 'Value', dataType: 'core.Any', required: true }],
      outputs: [], parameters: [],
    };
    const integerDescriptor: NodeDescriptor = {
      typeId: 'core.integer-consumer', name: 'Integer Consumer', version: 1,
      inputs: [{ id: 'value', name: 'Value', dataType: 'value.Integer', required: true }],
      outputs: [], parameters: [],
    };
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => root.render(
      <NodeLibrary
        descriptors={[...descriptors, wildcardDescriptor, integerDescriptor]}
        onAdd={vi.fn()}
        compatibleDataTypes={['core.Any', 'value.Float']}
      />,
    ));

    expect(host.textContent).toContain('Any Consumer');
    expect(host.textContent).toContain('Integer Consumer');
    await act(async () => root.unmount());
    host.remove();
  });

  it('lets users turn off the compatible-only context filter', async () => {
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => root.render(
      <NodeLibrary descriptors={descriptors} onAdd={vi.fn()} compatibleDataTypes={['core.Image']} />,
    ));

    const filter = host.querySelector<HTMLInputElement>('[aria-label="Compatible nodes only"]');
    expect(filter?.checked).toBe(true);
    await act(async () => filter?.click());

    expect(filter?.checked).toBe(false);
    expect(host.textContent).toContain('Constant');
    expect(host.textContent).toContain('Showing all nodes');
    await act(async () => root.unmount());
    host.remove();
  });
});
