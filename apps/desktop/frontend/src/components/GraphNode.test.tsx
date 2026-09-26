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
    // Every keystroke comes back as a new value prop. The field must keep what
    // was typed instead of being reset to the last committed number.
    await act(async () => {
      type('1');
      type('1.');
      type('1.5');
    });
    expect(input.value).toBe('1.5');
    expect(value.onParameterChange).toHaveBeenCalledWith('exposure', 'exposure', 1.5);

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
