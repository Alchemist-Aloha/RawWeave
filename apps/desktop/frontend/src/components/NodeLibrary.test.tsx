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

  it('groups nodes by category as a collapsible tree', async () => {
    const upscale: NodeDescriptor = {
      typeId: 'ai.upscale', name: 'AI Upscale', version: 1,
      inputs: [], outputs: [], parameters: [],
    };
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => root.render(<NodeLibrary descriptors={[...descriptors, upscale]} onAdd={vi.fn()} />));

    const groups = [...host.querySelectorAll<HTMLDetailsElement>('details.node-library__group')];
    expect(groups.map((group) => group.querySelector('summary')?.textContent)).toEqual(['Tone & exposure1', 'Values1', 'AI editing1']);
    expect(groups.every((group) => group.open)).toBe(true);
    expect(groups[0].querySelectorAll('.node-library__item')).toHaveLength(1);

    // Collapsing a branch keeps its nodes out of the way without a search.
    await act(async () => {
      groups[0].open = false;
      groups[0].dispatchEvent(new Event('toggle'));
    });
    expect(host.querySelectorAll<HTMLDetailsElement>('details.node-library__group')[0].open).toBe(false);

    // Filtering must still surface a match inside a collapsed branch.
    const search = host.querySelector<HTMLInputElement>('input[type="search"]');
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
      setter?.call(search, 'Exposure');
      search?.dispatchEvent(new Event('input', { bubbles: true }));
    });
    expect(host.querySelectorAll<HTMLDetailsElement>('details.node-library__group')[0].open).toBe(true);
    expect(host.textContent).toContain('Exposure');
    expect(host.textContent).not.toContain('AI Upscale');
    await act(async () => root.unmount());
    host.remove();
  });

  it('organizes packs by task, keeps unknown nodes, and searches category names', async () => {
    const nodes = [
      ['vendor.custom', 'Custom'], ['pro.grain', 'Grain'],
      ['core.mask-add', 'Mask Add'], ['core.mask-painted', 'Painted Mask'],
      ['core.mask-feather', 'Mask Feather'], ['pro.sharpen', 'Sharpen'],
      ['core.crop', 'Crop'], ['raw.decode', 'RAW Decode'],
      ['core.image-input', 'Image Input'], ['pro.color-zones', 'Color Zones'],
      ['core.expression', 'Expression'], ['core.compare', 'Compare'],
      ['core.hdr-merge', 'HDR Merge'], ['pro.histogram', 'Histogram'],
      ['ai.sky-mask', 'Sky Mask'], ['ai.scene-analysis', 'Scene Analysis'],
    ].map(([typeId, name]) => ({ ...descriptors[0], typeId, name }));
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    const onAdd = vi.fn();
    await act(async () => root.render(<NodeLibrary descriptors={nodes} onAdd={onAdd} />));
    expect([...host.querySelectorAll('summary')].map((summary) => summary.textContent)).toEqual([
      'Input & output1', 'RAW development1', 'Color1', 'Geometry & lens1', 'Detail & noise1',
      'Film & effects1', 'Mask sources1', 'Mask combine1', 'Mask refine1', 'Multi-image1',
      'Math & expressions1', 'Logic & routing1', 'Analysis1', 'AI masks1', 'AI analysis1', 'Vendor1',
    ]);
    expect(host.querySelectorAll('.node-library__item')).toHaveLength(nodes.length);
    await act(async () => {
      const search = host.querySelector('input[type="search"]');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(search, 'geometry');
      search?.dispatchEvent(new Event('input', { bubbles: true }));
    });
    expect(host.querySelectorAll('.node-library__item')).toHaveLength(1);
    expect(host.textContent).toContain('Crop');
    await act(async () => host.querySelector<HTMLButtonElement>('.node-library__item')?.click());
    expect(onAdd).toHaveBeenCalledWith('core.crop');
    await act(async () => root.unmount());
    host.remove();
  });

  it('prioritizes name matches over category matches for keyboard creation', async () => {
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    const onAdd = vi.fn();
    await act(async () => root.render(<NodeLibrary descriptors={[
      { ...descriptors[0], typeId: 'core.curves', name: 'Curves' }, descriptors[0],
    ]} onAdd={onAdd} />));
    const search = host.querySelector('input[type="search"]');
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(search, 'Exposure');
      search?.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await act(async () => search?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));
    expect(onAdd).toHaveBeenCalledWith('core.exposure');
    expect(host.querySelector('.node-library__item strong')?.textContent).toBe('Exposure');
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
