import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { describe, expect, it, vi } from 'vitest';
import { Inspector } from './Inspector';
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
    inputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: true }],
    outputs: [{ id: 'image', name: 'Image', dataType: 'core.Image', required: false }],
    parameters: [{ id: 'exposure', name: 'Exposure', parameterType: 'Float', default: 0, min: null, max: null }],
  },
};

describe('Inspector workflow ports', () => {
  it('renders input and output exposure controls and reports toggles', async () => {
    const onToggleInput = vi.fn();
    const container = document.createElement('div');
    document.body.appendChild(container);
    const root = createRoot(container);

    await act(async () => {
      root.render(
        <Inspector
          node={node}
          workflowInputs={[]}
          workflowOutputs={[{
            id: 'output:exposure:image',
            name: 'Image',
            direction: 'Output',
            nodeId: 'exposure',
            portId: 'image',
            dataType: 'core.Image',
            required: false,
          }]}
          onChange={vi.fn()}
          onToggleExposed={vi.fn()}
          onToggleInput={onToggleInput}
          onDelete={vi.fn()}
        />,
      );
    });

    expect(container.querySelector('[aria-label="Workflow ports"]')).not.toBeNull();
    expect(container.textContent).toContain('Expose');
    expect(container.textContent).toContain('Hide');

    const inputToggle = container.querySelector('button[aria-pressed="false"]');
    expect(inputToggle).not.toBeNull();
    await act(async () => {
      inputToggle?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });

    expect(onToggleInput).toHaveBeenCalledWith('exposure', 'image', 'Input', true);
    await act(async () => {
      root.unmount();
    });
    container.remove();
  });
});