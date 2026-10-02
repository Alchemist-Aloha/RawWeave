import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, expect, it, vi } from 'vitest';
import { ImageNodeControls } from './ImageNodeControls';
import { ViewerController } from '../viewer/controller';
import type { EditorNode } from '../editor/types';
import type { ImageDimensions, PreviewTarget, ViewerId } from '../viewer/types';

const target: PreviewTarget = {
  nodeId: 'upstream',
  nodeName: 'Input',
  outputPort: 'image',
  outputName: 'Image',
};
const cleanups: Array<() => Promise<void>> = [];
afterEach(async () => {
  for (const cleanup of cleanups.splice(0)) await cleanup();
});

function imageNode(typeId = 'core.resize'): EditorNode {
  return {
    id: 'node',
    typeId,
    parameters: {},
    position: { x: 0, y: 0 },
    descriptor: {
      typeId,
      name: 'Image node',
      version: 1,
      inputs: [],
      outputs: [],
      parameters: [],
    },
  };
}

function viewer(size: ImageDimensions = { width: 800, height: 600 }) {
  const controller = new ViewerController({
    requestPreview: async (request) => ({
      ...request,
      url: 'blob:input',
      width: 200,
      height: 150,
      fullWidth: size.width,
      fullHeight: size.height,
      mimeType: 'image/png',
    }),
    cancelPreview: async () => undefined,
    releasePreview: async () => undefined,
  });
  controller.setSourceDimensions({ width: 4000, height: 3000 });
  return controller;
}

async function mount(
  node = imageNode(),
  controller = viewer(),
  inputSize?: ImageDimensions,
  inputTarget: PreviewTarget | null = target,
) {
  const host = document.createElement('div');
  document.body.append(host);
  const root = createRoot(host);
  const onChange = vi.fn();
  const onDraw = vi.fn();
  const render = async () =>
    act(async () =>
      root.render(
        <ImageNodeControls
          node={node}
          target={inputTarget}
          inputSize={inputSize}
          controller={controller}
          onDraw={onDraw}
          onChange={onChange}
        />,
      ),
    );
  cleanups.push(async () => {
    await act(async () => root.unmount());
    host.remove();
  });
  await render();
  return {
    host,
    onChange,
    onDraw,
    render,
    button: (label: string) => {
      const button = Array.from(host.querySelectorAll('button')).find(
        (element) => element.textContent === label,
      );
      expect(button, label).toBeDefined();
      return button!;
    },
  };
}

it.each<ViewerId>(['A', 'B'])(
  'reads evaluated full dimensions in pane %s after request correction at 100%',
  async (pane) => {
    const controller = viewer();
    const { host, button, onChange } = await mount(imageNode(), controller, {
      width: 4000,
      height: 3000,
    });
    await act(async () => {
      controller.setViewport(pane, { width: 200, height: 150 });
      controller.viewAt100(pane);
      controller.setTarget(pane, target);
    });
    expect(controller.state.panes[pane].imageRegion).toMatchObject({
      width: 800,
      height: 600,
    });
    expect(host.textContent).toContain('Input resolution: 800 × 600 px');
    await act(async () => button('Half size').click());
    expect(onChange).toHaveBeenCalledExactlyOnceWith({
      width: 400,
      height: 300,
    });
  },
);

it('hides stale dimensions and uses only an explicitly provided input-size fallback', async () => {
  const controller = viewer();
  const { host, button, onChange } = await mount(imageNode(), controller);
  await act(async () => controller.setTarget('A', target));
  expect(host.textContent).toContain('800 × 600');
  await act(async () =>
    controller.setTarget('A', { ...target, outputPort: 'other' }),
  );
  expect(host.textContent).not.toContain('800 × 600');
  expect(button('Original size').disabled).toBe(true);
  await act(async () => controller.setTarget('A', null));
  const fallback = await mount(imageNode(), controller, {
    width: 4000,
    height: 3000,
  });
  expect(fallback.host.textContent).toContain(
    'Input resolution: 4000 × 3000 px',
  );
  expect(onChange).not.toHaveBeenCalled();
});

it('applies centered maximum-fitting integer crop presets in a single parameter batch', async () => {
  const node = imageNode('core.crop');
  node.parameters = { x: 5, y: 10, width: 300, height: 200 };
  const { host, onChange, button, render } = await mount(node, viewer(), {
    width: 801,
    height: 603,
  });
  expect(host.textContent).toContain('Crop result resolution: 300 × 200 px');
  const select = host.querySelector<HTMLSelectElement>(
    'select[aria-label="Crop preset"]',
  )!;
  expect(select).not.toBeNull();
  for (const [value, expected] of [
    ['full', { x: 0, y: 0, width: 801, height: 603 }],
    ['1:1', { x: 99, y: 0, width: 603, height: 603 }],
    ['3:2', { x: 0, y: 34, width: 801, height: 534 }],
    ['4:3', { x: 0, y: 1, width: 801, height: 600 }],
    ['16:9', { x: 0, y: 76, width: 801, height: 450 }],
  ] as const) {
    onChange.mockClear();
    await act(async () => {
      select.value = value;
      select.dispatchEvent(new Event('change', { bubbles: true }));
    });
    expect(onChange).toHaveBeenCalledExactlyOnceWith(expected);
    expect(Object.values(expected).every(Number.isInteger)).toBe(true);
    expect(expected.x + expected.width).toBeLessThanOrEqual(801);
    expect(expected.y + expected.height).toBeLessThanOrEqual(603);
    node.parameters = expected;
    await render();
    expect(host.textContent).toContain(
      `Crop result resolution: ${expected.width} × ${expected.height} px`,
    );
  }
  onChange.mockClear();
  await act(async () => button('Use full image').click());
  expect(onChange).toHaveBeenCalledExactlyOnceWith({
    x: 0,
    y: 0,
    width: 801,
    height: 603,
  });
});

it.each([
  { width: 103, height: 801 },
  { width: 1, height: 1 },
])(
  'keeps portrait and tiny crop presets inside $width × $height input',
  async (size) => {
    const { host, onChange } = await mount(
      imageNode('core.crop'),
      viewer(),
      size,
    );
    const select = host.querySelector('select')!;
    for (const option of Array.from(select.options).filter(
      (option) => option.value && !option.disabled,
    )) {
      onChange.mockClear();
      await act(async () => {
        select.value = option.value;
        select.dispatchEvent(new Event('change', { bubbles: true }));
      });
      expect(onChange).toHaveBeenCalledTimes(1);
      const region = onChange.mock.calls[0][0];
      expect(Object.values(region).every(Number.isInteger)).toBe(true);
      expect(region.width).toBeGreaterThan(0);
      expect(region.height).toBeGreaterThan(0);
      expect(region.x).toBe(Math.floor((size.width - region.width) / 2));
      expect(region.y).toBe(Math.floor((size.height - region.height) / 2));
      expect(region.x + region.width).toBeLessThanOrEqual(size.width);
      expect(region.y + region.height).toBeLessThanOrEqual(size.height);
    }
  },
);

it.each([
  { width: 101, height: 3 },
  { width: 1, height: 1 },
])(
  'offers safe resize downscales and a current output readout for $width × $height input',
  async (size) => {
    const node = imageNode();
    node.parameters = { width: 31, height: 17 };
    const { host, button, onChange, render } = await mount(
      node,
      viewer(),
      size,
    );
    expect(host.textContent).toContain('Output resolution: 31 × 17 px');
    for (const [label, expected] of [
      ['Original size', size],
      ['Half size', size.width === 1 ? size : { width: 51, height: 2 }],
      ['Quarter size', size.width === 1 ? size : { width: 25, height: 1 }],
      [
        'Three-quarter size',
        size.width === 1 ? size : { width: 76, height: 2 },
      ],
    ] as const) {
      onChange.mockClear();
      await act(async () => button(label).click());
      expect(onChange).toHaveBeenCalledExactlyOnceWith(expected);
      expect(expected.width).toBeLessThanOrEqual(size.width);
      expect(expected.height).toBeLessThanOrEqual(size.height);
      node.parameters = { ...expected };
      await render();
      expect(host.textContent).toContain(
        `Output resolution: ${expected.width} × ${expected.height} px`,
      );
    }
  },
);

it('keeps unknown inputs honest, actions disabled, and the exposed-port hint visible', async () => {
  const node = imageNode('core.crop');
  node.parameters = { x: 0, y: 0, width: 500, height: 300 };
  node.exposedParameters = ['width'];
  const { host, button } = await mount(node);
  expect(host.textContent).toContain('Preview input to read its resolution');
  expect(host.textContent).toContain('Crop result resolution: unknown');
  expect(host.textContent).toContain(
    'Exposed parameter ports may override these values.',
  );
  expect(host.querySelector('select')!.disabled).toBe(true);
  expect(button('Use full image').disabled).toBe(true);
  expect(button('Draw crop region').disabled).toBe(false);
  const disconnected = await mount(imageNode(), viewer(), undefined, null);
  expect(disconnected.host.textContent).toContain('Connect an image input');
  expect(disconnected.button('View input size').disabled).toBe(true);
  for (const label of [
    'Original size',
    'Half size',
    'Quarter size',
    'Three-quarter size',
  ])
    expect(disconnected.button(label).disabled).toBe(true);
});

it.each([NaN, Infinity, 0, -1])(
  'rejects invalid dimension %s from the getter and fallback',
  async (width) => {
    const controller = viewer();
    Object.assign(controller, {
      getTargetDimensions: () => ({ width, height: 600 }),
    });
    const { host, button, onChange } = await mount(imageNode(), controller, {
      width,
      height: 600,
    });
    expect(host.textContent).toContain('Preview input to read its resolution');
    expect(button('Original size').disabled).toBe(true);
    expect(onChange).not.toHaveBeenCalled();
  },
);

it('does not claim a result resolution for an out-of-bounds crop or invalid output parameters', async () => {
  const node = imageNode('core.crop');
  node.parameters = { x: 700, y: 0, width: 300, height: 200 };
  const { host, render } = await mount(node, viewer(), {
    width: 800,
    height: 600,
  });
  expect(host.textContent).toContain('Crop result resolution: unknown');
  node.typeId = 'core.resize';
  node.parameters = { width: NaN, height: 0 };
  await render();
  expect(host.textContent).toContain('Output resolution: unknown');
});

it('uses descriptor defaults for resolution readouts', async () => {
  const node = imageNode();
  node.descriptor.parameters = ['width', 'height'].map((id) => ({
    id,
    name: id,
    parameterType: 'Float',
    default: id === 'width' ? 800 : 600,
    min: 1,
    max: null,
  }));
  const { host } = await mount(node);
  expect(host.textContent).toContain('Output resolution: 800 × 600 px');
});

it('offers Preview input for any image-input node without adding geometric controls', async () => {
  const node = imageNode('core.exposure');
  node.descriptor.inputs = [
    { id: 'image', name: 'Image', dataType: 'core.Image', required: true },
  ];
  const { host, button, onDraw } = await mount(node);
  await act(async () => button('Preview input').click());
  expect(onDraw).toHaveBeenCalledTimes(1);
  expect(host.textContent).not.toContain('Draw gradient');
  expect(host.querySelector('select')).toBeNull();
  const unrelated = await mount(imageNode('core.float'));
  expect(unrelated.host.textContent).toBe('');
});

it.each(['core.mask-linear-gradient', 'core.mask-radial-gradient'])(
  'retains drawing for %s even with an empty descriptor',
  async (typeId) => {
    const { button, onDraw } = await mount(imageNode(typeId));
    await act(async () => button('Draw gradient').click());
    expect(onDraw).toHaveBeenCalledTimes(1);
  },
);
