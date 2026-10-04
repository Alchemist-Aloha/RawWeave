import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { ReactFlowProvider, type NodeProps } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';
import {
  GraphNode,
  GraphNodeActionsContext,
  type GraphNodeActions,
  type RawWeaveFlowNode,
} from './GraphNode';
import type { EditorNode } from '../editor/types';

const node: EditorNode = {
  id: 'exposure',
  typeId: 'core.exposure',
  parameters: { exposure: 0 },
  exposedParameters: [],
  position: { x: 0, y: 0 },
  descriptor: {
    typeId: 'core.exposure',
    name: 'Exposure',
    version: 1,
    inputs: [
      { id: 'image', name: 'Image', dataType: 'core.Image', required: true },
      { id: 'exposure', name: 'Exposure', dataType: 'value.Float', required: false },
    ],
    outputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: false }],
    parameters: [{ id: 'exposure', name: 'Exposure', parameterType: 'Float', default: 0, min: null, max: null }],
  },
};

function actions(overrides: Partial<GraphNodeActions> = {}): GraphNodeActions {
  return {
    onParameterChange: vi.fn(),
    onToggleExposed: vi.fn(),
    onTogglePort: vi.fn(),
    onDelete: vi.fn(),
    workflowInputs: [],
    workflowOutputs: [],
    checkpoint: null,
    ...overrides,
  };
}

async function renderNode(editorNode: EditorNode, value: GraphNodeActions, selected = false) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const root = createRoot(container);
  const props = {
    data: { node: editorNode, checkpointStatus: null },
    selected,
  } as unknown as NodeProps<RawWeaveFlowNode>;
  await act(async () => {
    root.render(
      <ReactFlowProvider>
        <GraphNodeActionsContext.Provider value={value}>
          <GraphNode {...props} />
        </GraphNodeActionsContext.Provider>
      </ReactFlowProvider>,
    );
  });
  return { container, root };
}

describe('GraphNode parameters', () => {
  it('shows point curves in primary controls and commits valid point drafts once', async () => {
    const curve: EditorNode = {
      ...node, typeId: 'core.curve', parameters: { points: '0,0;1,1' },
      descriptor: { ...node.descriptor, typeId: 'core.curve', name: 'Curve', parameters: [
        { id: 'points', name: 'Control Points', parameterType: 'String', default: '0,0;1,1', min: null, max: null },
      ] },
    };
    const value = actions();
    const { container, root } = await renderNode(curve, value);
    const input = container.querySelector<HTMLTextAreaElement>('textarea[aria-label="Curve Points"]')!;
    expect(input.closest('.parameter-advanced')).toBeNull();
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!;
    const type = (text: string) => { setter.call(input, text); input.dispatchEvent(new Event('input', { bubbles: true })); };
    await act(async () => { input.focus(); type('0,0;0.5,0.7;1,1'); });
    expect(container.querySelector('.curve-preview polyline')).not.toBeNull();
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));
    expect(value.onParameterChange).toHaveBeenCalledExactlyOnceWith('exposure', 'points', '0,0;0.5,0.7;1,1');
    await act(async () => { input.focus(); type('broken'); input.blur(); });
    expect(value.onParameterChange).toHaveBeenCalledTimes(1);
    await act(async () => { input.focus(); type('0,0;1,0'); input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); input.blur(); });
    expect(value.onParameterChange).toHaveBeenCalledTimes(1);
    await act(async () => root.unmount());
    container.remove();
  });
  it.each(['core.curve', 'pro.lut', 'pro.lut-tools', 'pro.film-curve', 'pro.film-simulation'])('shares %s curve drafts with the text field and publishes exactly once on release', async (typeId) => {
    const curve: EditorNode = { ...node, typeId, parameters: { points: '0,0;1,1' },
      descriptor: { ...node.descriptor, parameters: [{ id: 'points', name: 'Control Points', parameterType: 'String', default: '0,0;1,1', min: null, max: null }] } };
    const value = actions();
    const { container, root } = await renderNode(curve, value);
    const svg = container.querySelector<SVGSVGElement>('.curve-editor svg')!;
    expect(svg).not.toBeNull();
    Object.assign(svg, { getScreenCTM: () => ({ a: 1, b: 0, c: 0, d: 1, e: 0, f: 0, inverse() { return this; } }), setPointerCapture: vi.fn(), hasPointerCapture: () => false });
    const pointer = (type: string, x: number, y: number) => {
      const event = new MouseEvent(type, { bubbles: true, cancelable: true, button: 0, clientX: x, clientY: y });
      Object.defineProperty(event, 'pointerId', { value: 1 });
      svg.dispatchEvent(event);
    };
    await act(async () => { pointer('pointerdown', 100, 60); pointer('pointermove', 100, 39.2); });
    const field = container.querySelector<HTMLTextAreaElement>('textarea')!;
    expect(field.value).toBe('0,0;0.5,0.7;1,1');
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => pointer('pointerup', 100, 39.2));
    expect(value.onParameterChange).toHaveBeenCalledExactlyOnceWith('exposure', 'points', '0,0;0.5,0.7;1,1');
    await act(async () => { field.focus(); field.blur(); });
    expect(value.onParameterChange).toHaveBeenCalledTimes(1);
    await act(async () => root.unmount());
    container.remove();
  });
  it('does not discard valid point-text drafts when selecting a handle without moving it', async () => {
    const curve: EditorNode = { ...node, typeId: 'core.curve', parameters: { points: '0,0;1,1' },
      descriptor: { ...node.descriptor, parameters: [{ id: 'points', name: 'Control Points', parameterType: 'String', default: '0,0;1,1', min: null, max: null }] } };
    const value = actions();
    const { container, root } = await renderNode(curve, value);
    const field = container.querySelector<HTMLTextAreaElement>('textarea')!;
    await act(async () => {
      field.focus();
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(field, '0,0;0.5,0.7;1,1');
      field.dispatchEvent(new Event('input', { bubbles: true }));
    });
    const svg = container.querySelector<SVGSVGElement>('.curve-editor svg')!;
    Object.assign(svg, { getScreenCTM: () => ({ a: 1, b: 0, c: 0, d: 1, e: 0, f: 0, inverse() { return this; } }), setPointerCapture: vi.fn(), hasPointerCapture: () => false });
    await act(async () => {
      for (const type of ['pointerdown', 'pointerup']) {
        const event = new MouseEvent(type, { bubbles: true, cancelable: true, button: 0, clientX: 100, clientY: 39.2 });
        Object.defineProperty(event, 'pointerId', { value: 1 });
        svg.dispatchEvent(event);
      }
    });
    expect(value.onParameterChange).toHaveBeenCalledExactlyOnceWith('exposure', 'points', '0,0;0.5,0.7;1,1');
    expect(field.value).toBe('0,0;0.5,0.7;1,1');
    await act(async () => root.unmount());
    container.remove();
  });
  it('plots the gamma slider draft without publishing it and restores on Escape', async () => {
    const curve: EditorNode = {
      ...node, typeId: 'core.curves', parameters: { gamma: 1 },
      descriptor: { ...node.descriptor, parameters: [
        { id: 'gamma', name: 'Gamma', parameterType: 'Float', default: 1, min: 0.0001, max: null },
      ] },
    };
    const value = actions();
    const { container, root } = await renderNode(curve, value);
    const plot = () => container.querySelector('polyline')!.getAttribute('points');
    const initial = plot();
    const slider = container.querySelector<HTMLInputElement>('input[type="range"]')!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(slider, '2');
      slider.dispatchEvent(new Event('input', { bubbles: true }));
    });
    expect(plot()).not.toBe(initial);
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => slider.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
    expect(plot()).toBe(initial);
    await act(async () => root.unmount());
    container.remove();
  });
  it.each(['core.levels', 'core.map-range', 'core.clamp'])('shows an authored mapping for %s only while parameters are open', async (typeId) => {
    const editorNode = { ...node, typeId };
    const value = actions();
    const { container, root } = await renderNode(editorNode, value);
    expect(container.querySelector('.parameter-transfer-preview')).toBeNull();
    const details = container.querySelector<HTMLDetailsElement>('.graph-node__details')!;
    await act(async () => { details.open = true; details.dispatchEvent(new Event('toggle')); });
    expect(container.querySelector('.parameter-transfer-preview svg')).not.toBeNull();
    expect(container.textContent).toContain('Applied parameter values');
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => { details.open = false; details.dispatchEvent(new Event('toggle')); });
    expect(container.querySelector('.parameter-transfer-preview')).toBeNull();
    await act(async () => root.unmount());
    container.remove();
  });
  it.each(['core.mask-color-qualifier', 'pro.color-zones', 'pro.split-toning'])('shows the color reference for %s without publishing graph edits', async (typeId) => {
    const value = actions();
    const { container, root } = await renderNode({ ...node, typeId }, value);
    expect(container.querySelector('.color-parameter-preview')).toBeNull();
    const details = container.querySelector<HTMLDetailsElement>('.graph-node__details')!;
    await act(async () => { details.open = true; details.dispatchEvent(new Event('toggle')); });
    expect(container.querySelector('.color-parameter-preview [role="img"]')).not.toBeNull();
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => { details.open = false; details.dispatchEvent(new Event('toggle')); });
    expect(container.querySelector('.color-parameter-preview')).toBeNull();
    await act(async () => root.unmount());
    container.remove();
  });
  it('mounts live image helpers only while parameters are open', async () => {
    const imageControls = vi.fn(() => <span>Live image helpers</span>);
    const { container, root } = await renderNode(node, actions({ imageControls }));
    expect(imageControls).not.toHaveBeenCalled();
    const details = container.querySelector('details.graph-node__details') as HTMLDetailsElement;
    await act(async () => {
      details.open = true;
      details.dispatchEvent(new Event('toggle'));
    });
    expect(container.textContent).toContain('Live image helpers');
    await act(async () => {
      details.open = false;
      details.dispatchEvent(new Event('toggle'));
    });
    expect(container.textContent).not.toContain('Live image helpers');
    await act(async () => root.unmount());
    container.remove();
  });
  it('edits parameters and exposes ports from the node card', async () => {
    const value = actions();
    const { container, root } = await renderNode(node, value);

    const details = container.querySelector('details.graph-node__details');
    expect(details).not.toBeNull();
    expect(details?.classList.contains('nodrag')).toBe(true);
    // Folded away until asked for.
    expect(details?.hasAttribute('open')).toBe(false);

    const input = container.querySelector<HTMLInputElement>('.parameter input[type="number"]');
    if (!input) throw new Error('parameter input missing');
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    const type = (text: string) => {
      setter?.call(input, text);
      input.dispatchEvent(new Event('input', { bubbles: true }));
    };
    // A precise edit stays local until committed, rather than creating an
    // undo entry and preview request for each intermediate number.
    await act(async () => {
      type('1');
      type('1.');
      type('1.5');
    });
    expect(input.value).toBe('1.5');
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => { input.focus(); input.blur(); });
    expect(value.onParameterChange).toHaveBeenCalledExactlyOnceWith('exposure', 'exposure', 1.5);

    await act(async () => {
      container.querySelector<HTMLButtonElement>('[aria-label="Expose Exposure port"]')?.click();
    });
    expect(value.onToggleExposed).toHaveBeenCalledWith('exposure', 'exposure', true);

    // The workflow-port expose toggles live here too now.
    await act(async () => {
      container.querySelectorAll<HTMLButtonElement>('section[aria-label="Workflow ports"] .port-row button')[1]?.click();
    });
    expect(value.onTogglePort).toHaveBeenCalledWith('exposure', 'exposure', 'Input', true);

    await act(async () => root.unmount());
    container.remove();
  });

  it('commits on Enter, cancels on Escape, and skips unchanged or invalid numeric drafts', async () => {
    const value = actions();
    const { container, root } = await renderNode(node, value);
    const input = container.querySelector<HTMLInputElement>('input[aria-label="Exposure"]')!;
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
    const type = async (text: string) => act(async () => {
      input.focus();
      setter.call(input, text);
      input.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await type('1.25');
    await act(async () => input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
    expect(input.value).toBe('0');
    await act(async () => input.blur());
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await type('0');
    await act(async () => input.blur());
    await type('');
    expect(input.getAttribute('aria-invalid')).toBe('true');
    await act(async () => input.blur());
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await type('1.25');
    await act(async () => input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));
    expect(value.onParameterChange).toHaveBeenCalledExactlyOnceWith('exposure', 'exposure', 1.25);
    expect(document.activeElement).not.toBe(input);
    await act(async () => root.unmount());
    container.remove();
  });

  it('shows contextual help, a photographic percentage, and reset behavior', async () => {
    const aiNode: EditorNode = {
      ...node,
      id: 'ai-node',
      typeId: 'ai.img2img',
      parameters: { strength: 0.25 },
      descriptor: {
        ...node.descriptor,
        typeId: 'ai.img2img',
        name: 'AI Image to Image',
        parameters: [{ id: 'strength', name: 'Strength', parameterType: 'Float', default: 0.75, min: 0, max: 1 }],
      },
    };
    const value = actions();
    const { container, root } = await renderNode(aiNode, value);
    const field = container.querySelector<HTMLInputElement>('.parameter input[type="number"]');
    expect(field?.value).toBe('25');
    expect(container.querySelector('.parameter__label')?.getAttribute('data-tooltip')).toContain('generated result');
    expect(container.querySelector<HTMLInputElement>('.parameter input[type="range"]')?.max).toBe('100');

    await act(async () => {
      if (!field) throw new Error('parameter input missing');
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
      setter?.call(field, '50');
      field.dispatchEvent(new Event('input', { bubbles: true }));
    });
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => { field?.focus(); field?.blur(); });
    expect(value.onParameterChange).toHaveBeenCalledExactlyOnceWith('ai-node', 'strength', 0.5);
    await act(async () => container.querySelector<HTMLButtonElement>('[aria-label="Reset Generation Strength"]')?.click());
    expect(value.onParameterChange).toHaveBeenLastCalledWith('ai-node', 'strength', 0.75);

    await act(async () => root.unmount());
    container.remove();
  });

  it('provides bounded sliders, precise numeric editing, validation, and reset', async () => {
    const boundedNode: EditorNode = {
      ...node,
      parameters: { amount: 0.6, count: 2 },
      descriptor: {
        ...node.descriptor,
        parameters: [
          { id: 'amount', name: 'Amount', parameterType: 'Float', default: 0.5, min: 0, max: 1 },
          { id: 'count', name: 'Count', parameterType: 'Integer', default: 2, min: 1, max: 5 },
        ],
      },
    };
    const value = actions();
    const { container, root } = await renderNode(boundedNode, value);

    expect(container.querySelector('input[type="range"][aria-label="Effect Strength slider"]')).not.toBeNull();
    expect(container.querySelector('input[type="range"][aria-label="Count slider"]')).not.toBeNull();

    const amount = container.querySelector<HTMLInputElement>('input[aria-label="Effect Strength"]');
    const count = container.querySelector<HTMLInputElement>('input[aria-label="Count"]');
    if (!amount || !count) throw new Error('numeric parameter inputs missing');
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    const type = async (input: HTMLInputElement, text: string) => {
      setter?.call(input, text);
      await act(async () => input.dispatchEvent(new Event('input', { bubbles: true })));
    };

    const slider = container.querySelector<HTMLInputElement>('input[aria-label="Effect Strength slider"]')!;
    await act(async () => slider.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true })));
    await type(slider, '0.8');
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => slider.dispatchEvent(new MouseEvent('pointerup', { bubbles: true })));
    expect(value.onParameterChange).toHaveBeenCalledWith('exposure', 'amount', 0.8);
    vi.mocked(value.onParameterChange).mockClear();
    await type(slider, '0.7');
    expect(amount.value).toBe('0.7');
    vi.mocked(value.onParameterChange).mockClear();
    await type(amount, '');
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await type(amount, '0.75');
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => { amount.focus(); amount.blur(); });
    await type(amount, '2');
    await act(async () => { amount.focus(); amount.blur(); });
    await type(count, '2.5');
    await act(async () => { count.focus(); count.blur(); });
    expect(value.onParameterChange).toHaveBeenCalledWith('exposure', 'amount', 0.75);
    expect(value.onParameterChange).not.toHaveBeenCalledWith('exposure', 'amount', 2);
    expect(value.onParameterChange).not.toHaveBeenCalledWith('exposure', 'count', 2.5);

    await act(async () => {
      container.querySelector<HTMLButtonElement>('[aria-label="Reset Effect Strength"]')?.click();
    });
    expect(value.onParameterChange).toHaveBeenCalledWith('exposure', 'amount', 0.5);

    await act(async () => root.unmount());
    container.remove();
  });

  it('leaves pointer capture to the native range control', async () => {
    const value = actions();
    const { container, root } = await renderNode(node, value);
    const slider = container.querySelector<HTMLInputElement>('input[type="range"]')!;
    const capture = vi.fn();
    slider.setPointerCapture = capture;
    await act(async () => slider.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 })));
    await act(async () => slider.dispatchEvent(new MouseEvent('pointermove', { bubbles: true, buttons: 0 })));
    expect(capture).not.toHaveBeenCalled();
    expect(slider.value).toBe('0');
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => root.unmount()); container.remove();
  });

  it('keeps native slider input events local and commits once on release', async () => {
    const value = actions();
    const { container, root } = await renderNode(node, value);
    const slider = container.querySelector<HTMLInputElement>('input[type="range"]')!;
    const field = container.querySelector<HTMLInputElement>('input[type="number"]')!;
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
    // Native range controls may send input without a React pointerdown.
    for (const text of ['1', '1.2', '1.25']) {
      await act(async () => {
        setter.call(slider, text);
        slider.dispatchEvent(new Event('input', { bubbles: true }));
      });
    }
    expect(value.onParameterChange).not.toHaveBeenCalled();
    expect(field.value).toBe('1.25');
    await act(async () => {
      slider.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));
      slider.dispatchEvent(new MouseEvent('pointerup', { bubbles: true }));
      slider.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
    });
    expect(value.onParameterChange).toHaveBeenCalledExactlyOnceWith('exposure', 'exposure', 1.25);
    // Keep the committed value visible while the backend acknowledges it.
    expect(field.value).toBe('1.25');
    await act(async () => root.unmount()); container.remove();
  });

  it('groups repeated slider keys and numeric stepper edits until release', async () => {
    const value = actions();
    const { container, root } = await renderNode(node, value);
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
    for (const selector of ['input[type="range"]', 'input[type="number"]']) {
      const field = container.querySelector<HTMLInputElement>(selector)!;
      vi.mocked(value.onParameterChange).mockClear();
      for (const text of selector.includes('range') ? ['0.01', '0.02', '0.03'] : ['0.04', '0.05', '0.06']) {
        await act(async () => {
          setter.call(field, text);
          field.dispatchEvent(new Event('input', { bubbles: true }));
        });
      }
      expect(value.onParameterChange).not.toHaveBeenCalled();
      await act(async () => field.dispatchEvent(new KeyboardEvent('keyup', { key: 'ArrowUp', bubbles: true })));
      expect(value.onParameterChange).toHaveBeenCalledTimes(1);
      // Return to the saved value to begin a distinct spinner gesture.
      await act(async () => field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
    }
    const number = container.querySelector<HTMLInputElement>('input[type="number"]')!;
    vi.mocked(value.onParameterChange).mockClear();
    await act(async () => {
      setter.call(number, '0.05');
      number.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await act(async () => number.dispatchEvent(new MouseEvent('mouseup', { bubbles: true })));
    expect(value.onParameterChange).toHaveBeenCalledExactlyOnceWith('exposure', 'exposure', 0.05);
    await act(async () => root.unmount()); container.remove();
  });

  it('does not apply a typed draft when cursor-navigation keys are released', async () => {
    const value = actions();
    const { container, root } = await renderNode(node, value);
    const field = container.querySelector<HTMLInputElement>('input[type="number"]')!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(field, '1.25');
      field.dispatchEvent(new Event('input', { bubbles: true }));
    });
    for (const key of ['ArrowLeft', 'ArrowRight', 'Home', 'End']) {
      await act(async () => field.dispatchEvent(new KeyboardEvent('keyup', { key, bubbles: true })));
    }
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => root.unmount()); container.remove();
  });

  it('cancels range drafts without publishing and skips unchanged releases', async () => {
    const value = actions();
    const { container, root } = await renderNode(node, value);
    const slider = container.querySelector<HTMLInputElement>('input[type="range"]')!;
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
    await act(async () => {
      setter.call(slider, '2'); slider.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await act(async () => slider.dispatchEvent(new MouseEvent('pointercancel', { bubbles: true })));
    await act(async () => slider.dispatchEvent(new MouseEvent('mouseup', { bubbles: true })));
    expect(slider.value).toBe('0');
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => {
      setter.call(slider, '0'); slider.dispatchEvent(new Event('input', { bubbles: true }));
      slider.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));
    });
    expect(value.onParameterChange).not.toHaveBeenCalled();
    await act(async () => root.unmount()); container.remove();
  });

  it('uses whole-pixel editing for crop geometry and multiline fields', async () => {
    const cropNode: EditorNode = {
      ...node,
      typeId: 'core.crop',
      descriptor: {
        ...node.descriptor,
        typeId: 'core.crop',
        parameters: [{ id: 'x', name: 'X', parameterType: 'Float', default: 0, min: 0, max: 10 }],
      },
      parameters: { x: 0 },
    };
    const value = actions();
    const { container, root } = await renderNode(cropNode, value);
    const input = container.querySelector<HTMLInputElement>('input[aria-label="Left Edge"]');
    if (!input) throw new Error('crop input missing');
    expect(input.step).toBe('1');

    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    setter?.call(input, '1.5');
    await act(async () => input.dispatchEvent(new Event('input', { bubbles: true })));
    expect(value.onParameterChange).not.toHaveBeenCalledWith('exposure', 'x', 1.5);

    const multilineNode: EditorNode = {
      ...node,
      descriptor: {
        ...node.descriptor,
        parameters: [{ id: 'prompt', name: 'Prompt', parameterType: 'String', default: '', min: null, max: null }],
      },
      parameters: { prompt: '' },
    };
    await act(async () => root.unmount());
    container.remove();
    const rendered = await renderNode(multilineNode, value);
    expect(rendered.container.querySelector('textarea[aria-label="Prompt"]')).not.toBeNull();
    await act(async () => rendered.root.unmount());
    rendered.container.remove();
  });

  it('uses whole-pixel sliders for blur and mask filter radii', async () => {
    const blur: EditorNode = { ...node, typeId: 'core.blur', parameters: { radius: 1 }, descriptor: { ...node.descriptor, parameters: [{ id: 'radius', name: 'Radius', parameterType: 'Float', default: 1, min: 0, max: 64 }] } };
    const { container, root } = await renderNode(blur, actions());
    expect(container.querySelector('input[type="range"]')?.getAttribute('step')).toBe('1');
    expect(container.querySelector('input[type="number"]')?.getAttribute('step')).toBe('1');
    await act(async () => root.unmount()); container.remove();
  });

  it('offers the backend tone mapping operators without losing imported values', async () => {
    const tone: EditorNode = { ...node, typeId: 'pro.tone-map', parameters: { operator: 'filmic' }, descriptor: { ...node.descriptor, parameters: [{ id: 'operator', name: 'Operator', parameterType: 'String', default: 'reinhard', min: null, max: null }] } };
    const value = actions();
    const { container, root } = await renderNode(tone, value);
    const select = container.querySelector<HTMLSelectElement>('select[aria-label="Tone Mapping Method"]')!;
    expect(Array.from(select.options).map((option) => option.value)).toEqual(['reinhard', 'filmic', 'aces']);
    await act(async () => { select.value = 'aces'; select.dispatchEvent(new Event('change', { bubbles: true })); });
    expect(value.onParameterChange).toHaveBeenCalledWith('exposure', 'operator', 'aces');
    await act(async () => root.unmount()); container.remove();
  });

  it('deletes only on a clean press of the delete button, never on a drag', async () => {
    const value = actions();
    const { container, root } = await renderNode(node, value);
    const remove = container.querySelector<HTMLButtonElement>('[aria-label="Delete Exposure"]');
    if (!remove) throw new Error('delete button missing');
    const pointer = (type: string, x: number, y: number) => new MouseEvent(type, {
      bubbles: true, cancelable: true, clientX: x, clientY: y,
    });

    // A drag that starts on the button (the title row is the drag handle) must
    // not delete the node, even if the engine still synthesises a click.
    await act(async () => {
      remove.dispatchEvent(pointer('pointerdown', 10, 10));
      remove.dispatchEvent(pointer('pointermove', 90, 60));
      remove.dispatchEvent(pointer('click', 90, 60));
    });
    expect(value.onDelete).not.toHaveBeenCalled();

    // A press that stays put does.
    await act(async () => {
      remove.dispatchEvent(pointer('pointerdown', 10, 10));
      remove.dispatchEvent(pointer('pointermove', 11, 11));
      remove.dispatchEvent(pointer('click', 11, 11));
    });
    expect(value.onDelete).toHaveBeenCalledWith('exposure');

    // Keyboard and synthetic activation arrive as a bare click.
    await act(async () => {
      remove.dispatchEvent(pointer('click', 0, 0));
    });
    expect(value.onDelete).toHaveBeenCalledTimes(2);

    await act(async () => root.unmount());
    container.remove();
  });

  it('omits the details fold when no actions are provided', async () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    const root = createRoot(container);
    const props = {
      data: { node, checkpointStatus: null },
      selected: false,
    } as unknown as NodeProps<RawWeaveFlowNode>;
    await act(async () => {
      root.render(
        <ReactFlowProvider>
          <GraphNode {...props} />
        </ReactFlowProvider>,
      );
    });
    expect(container.querySelector('details.graph-node__details')).toBeNull();
    await act(async () => root.unmount());
    container.remove();
  });
});
