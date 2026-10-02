import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ClippingOverlay, targetsFor, Viewer } from './Viewer';
import { analyzeImageData } from '../viewer/analysis';
import type { EditorNode } from '../editor/types';
import { ViewerController } from '../viewer/controller';
import type { PreviewTransport } from '../viewer/transport';

function node(id: string, outputs: Array<{ id: string; name: string; dataType: string }>): EditorNode {
  return {
    id,
    typeId: `test.${id}`,
    parameters: {},
    position: { x: 0, y: 0 },
    descriptor: {
      typeId: `test.${id}`,
      name: id,
      version: 1,
      inputs: [],
      outputs: outputs.map((output) => ({ ...output, required: false })),
      parameters: [],
    },
  };
}

describe('Viewer', () => {
  it('passes DPR and displays a mip bitmap at full-coordinate size without multiplying zoom', async () => {
    const width = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(750);
    const height = vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(500);
    const dpr = vi.spyOn(window, 'devicePixelRatio', 'get').mockReturnValue(2);
    const requestPreview = vi.fn<PreviewTransport['requestPreview']>(async (request) => ({
      requestId: request.requestId, revision: request.revision,
      url: `rawweave-preview://localhost/preview/${request.requestId}.png`,
      width: 1500, height: 1000, fullWidth: 6000, fullHeight: 4000, mimeType: 'image/png',
    }));
    const controller = new ViewerController({ requestPreview, cancelPreview: async () => undefined, releasePreview: async () => undefined });
    controller.setSourceDimensions({ width: 6000, height: 4000 });
    const output = node('output', [{ id: 'image', name: 'Image', dataType: 'core.Image' }]);
    output.typeId = 'core.output';
    const crop = node('crop', []);
    crop.typeId = 'core.crop';
    const editingTarget = { nodeId: 'output', nodeName: 'output', outputPort: 'image', outputName: 'Image' };
    const dimensions = vi.spyOn(controller, 'getTargetDimensions');
    const onChange = vi.fn();
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => root.render(<Viewer controller={controller} nodes={[output]} revision={1}
      source={{ kind: 'ordinary', width: 6000, height: 4000, revision: 1, metadata: null }}
      geometryEditing={{ node: crop, target: editingTarget, onChange, onCancel: () => undefined }} />));
    expect(requestPreview.mock.calls.at(-1)?.[0]).toMatchObject({ mip: 2, region: { x: 0, y: 0, width: 6000, height: 4000 } });
    const image = host.querySelector<HTMLImageElement>('.viewer-pane__image')!;
    expect(image.width).toBe(6000);
    expect(image.height).toBe(4000);
    expect(image.style.transform).toContain('scale(0.125)');
    expect(dimensions).toHaveBeenCalledWith(editingTarget);
    vi.spyOn(image, 'getBoundingClientRect').mockReturnValue({ left: 0, top: 0, width: 750, height: 500 } as DOMRect);
    const overlay = host.querySelector('.geometry-overlay')!;
    const pointer = (type: string, x: number, y: number) => new MouseEvent(type, { bubbles: true, button: 0, clientX: x, clientY: y });
    await act(async () => overlay.dispatchEvent(pointer('pointerdown', 75, 50)));
    await act(async () => overlay.dispatchEvent(pointer('pointerup', 375, 250)));
    expect(onChange).toHaveBeenCalledWith({ x: 600, y: 400, width: 2400, height: 1600 });
    await act(async () => controller.viewAt100('A'));
    expect(requestPreview.mock.calls.at(-1)?.[0].mip).toBe(0);
    expect(image.style.transform).toContain('scale(1)');
    await act(async () => root.unmount());
    host.remove();
    width.mockRestore(); height.mockRestore(); dpr.mockRestore(); dimensions.mockRestore();
  });

  it('draws using evaluated output dimensions after an upstream resize at 100%', async () => {
    const requestPreview = vi.fn<PreviewTransport['requestPreview']>(async (request) => ({
      requestId: request.requestId, revision: request.revision,
      url: `rawweave-preview://localhost/preview/${request.requestId}.png`,
      width: 800, height: 600, fullWidth: 800, fullHeight: 600, mimeType: 'image/png',
    }));
    const controller = new ViewerController({ requestPreview, cancelPreview: async () => undefined, releasePreview: async () => undefined });
    controller.setSourceDimensions({ width: 4000, height: 3000 });
    controller.viewAt100('A');
    const output = node('resize', [{ id: 'image', name: 'Image', dataType: 'core.Image' }]);
    const crop = node('crop', []);
    crop.typeId = 'core.crop';
    const onChange = vi.fn();
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => root.render(<Viewer controller={controller} nodes={[output]} revision={1}
      source={{ kind: 'ordinary', width: 4000, height: 3000, revision: 1, metadata: null }}
      geometryEditing={{ node: crop, target: { nodeId: 'resize', nodeName: 'resize', outputPort: 'image', outputName: 'Image' }, onChange, onCancel: () => undefined }} />));
    expect(requestPreview.mock.calls.map(([request]) => request.region.width)).toEqual([4000, 800]);
    const image = host.querySelector<HTMLImageElement>('.viewer-pane__image')!;
    expect(image.width).toBe(800);
    expect(image.height).toBe(600);
    expect(image.style.transform).toContain('scale(1)');
    expect(host.querySelector('.geometry-overlay__controls')?.textContent).toContain('800 × 600 px');
    vi.spyOn(image, 'getBoundingClientRect').mockReturnValue({ left: 0, top: 0, width: 800, height: 600 } as DOMRect);
    const overlay = host.querySelector('.geometry-overlay')!;
    const pointer = (type: string, x: number, y: number) => new MouseEvent(type, { bubbles: true, button: 0, clientX: x, clientY: y });
    await act(async () => overlay.dispatchEvent(pointer('pointerdown', 80, 60)));
    await act(async () => overlay.dispatchEvent(pointer('pointerup', 400, 300)));
    expect(onChange).toHaveBeenCalledWith({ x: 80, y: 60, width: 320, height: 240 });
    await act(async () => root.unmount());
    host.remove();
  });

  it('selects the ordinary workflow output when an image opens', async () => {
    const requests: string[] = [];
    const transport: PreviewTransport = {
      requestPreview: (request) => {
        requests.push(`${request.nodeId}:${request.outputPort}`);
        return new Promise(() => undefined);
      },
      cancelPreview: async () => undefined,
      releasePreview: async () => undefined,
    };
    const controller = new ViewerController(transport);
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    const input = node('input', [{ id: 'image', name: 'Image', dataType: 'core.Image' }]);
    const output = node('output', [{ id: 'image', name: 'Image', dataType: 'core.Image' }]);
    output.typeId = 'core.output';

    await act(async () => root.render(
      <Viewer
        controller={controller}
        nodes={[input, output]}
        revision={1}
        source={{ kind: 'ordinary', width: 640, height: 480, revision: 1, metadata: null }}
      />,
    ));

    expect(controller.state.panes.A.target?.nodeId).toBe('output');
    expect(requests).toContain('output:image');

    await act(async () => root.unmount());
    host.remove();
  });

  it('selects the RAW display transform when a RAW image opens', async () => {
    const requests: string[] = [];
    const controller = new ViewerController({
      requestPreview: (request) => {
        requests.push(`${request.nodeId}:${request.outputPort}`);
        return new Promise(() => undefined);
      },
      cancelPreview: async () => undefined,
      releasePreview: async () => undefined,
    });
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    const display = node('display', [{ id: 'display', name: 'Display', dataType: 'color.DisplayRGB' }]);
    display.typeId = 'raw.display-transform';

    await act(async () => root.render(
      <Viewer
        controller={controller}
        nodes={[display]}
        revision={1}
        source={{ kind: 'raw', width: 640, height: 480, revision: 1, metadata: null }}
      />,
    ));

    expect(controller.state.panes.A.target?.nodeId).toBe('display');
    expect(requests).toContain('display:display');

    await act(async () => root.unmount());
    host.remove();
  });

  it('reports a preview URL that the webview cannot load', async () => {
    const releasePreview = vi.fn(async () => undefined);
    const controller = new ViewerController({
      requestPreview: async (request) => ({
        requestId: request.requestId,
        revision: request.revision,
        url: `rawweave-preview://localhost/preview/${request.requestId}.png`,
        width: 1,
        height: 1,
        fullWidth: 1,
        fullHeight: 1,
        mimeType: 'image/png',
      }),
      cancelPreview: async () => undefined,
      releasePreview,
    });
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    const output = node('output', [{ id: 'image', name: 'Image', dataType: 'core.Image' }]);
    output.typeId = 'core.output';

    await act(async () => root.render(
      <Viewer
        controller={controller}
        nodes={[output]}
        revision={1}
        source={{ kind: 'ordinary', width: 1, height: 1, revision: 1, metadata: null }}
      />,
    ));

    const image = host.querySelector<HTMLImageElement>('.viewer-pane__image');
    expect(image).not.toBeNull();
    await act(async () => image?.dispatchEvent(new Event('error')));

    expect(host.querySelector('[role="alert"]')?.textContent).toContain('preview image could not be loaded');
    expect(controller.state.panes.A.status).toBe('error');
    expect(releasePreview).toHaveBeenCalledWith(image?.src);
    // The idle hint must not sit behind the error card when a render fails.
    expect(host.querySelector('.viewer-pane__empty')).toBeNull();

    await act(async () => root.unmount());
    host.remove();
  });

  it('keeps the preview image out of the browser native image drag', async () => {
    // A natively draggable <img> hands the pointer to the browser as soon as the
    // drag starts, which fires pointercancel and kills the pan mid-gesture.
    const controller = new ViewerController({
      requestPreview: async (request) => ({
        requestId: request.requestId,
        revision: request.revision,
        url: `rawweave-preview://localhost/preview/${request.requestId}.png`,
        width: 1,
        height: 1,
        fullWidth: 1,
        fullHeight: 1,
        mimeType: 'image/png',
      }),
      cancelPreview: async () => undefined,
      releasePreview: async () => undefined,
    });
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    const output = node('output', [{ id: 'image', name: 'Image', dataType: 'core.Image' }]);
    output.typeId = 'core.output';

    await act(async () => root.render(
      <Viewer
        controller={controller}
        nodes={[output]}
        revision={1}
        source={{ kind: 'ordinary', width: 1, height: 1, revision: 1, metadata: null }}
      />,
    ));

    const image = host.querySelector<HTMLImageElement>('.viewer-pane__image');
    expect(image?.getAttribute('draggable')).toBe('false');
    const dragStart = new Event('dragstart', { bubbles: true, cancelable: true });
    expect(image?.dispatchEvent(dragStart)).toBe(false);

    await act(async () => root.unmount());
    host.remove();
  });

  it('offers ordinary and spatial graph values as preview targets', () => {
    const targets = targetsFor([
      node('segmentation', [
        { id: 'label_map', name: 'Label Map', dataType: 'core.LabelMap' },
        { id: 'confidence', name: 'Confidence', dataType: 'core.ConfidenceMap' },
        { id: 'regions', name: 'Regions', dataType: 'core.RegionSet' },
        { id: 'masks', name: 'Masks', dataType: 'core.MaskSet' },
        { id: 'depth', name: 'Depth', dataType: 'core.DepthMap' },
      ]),
    ]);

    expect(targets.map((target) => target.dataType)).toEqual([
      'core.LabelMap',
      'core.ConfidenceMap',
      'core.RegionSet',
      'core.MaskSet',
      'core.DepthMap',
    ]);
  });

  it('draws an enabled clipping canvas over the displayed image bounds', async () => {
    const context = {
      createImageData: vi.fn((width: number, height: number) => ({
        data: new Uint8ClampedArray(width * height * 4),
        width,
        height,
      })),
      putImageData: vi.fn(),
    };
    const getContext = vi.spyOn(HTMLCanvasElement.prototype, 'getContext')
      .mockReturnValue(context as unknown as CanvasRenderingContext2D);
    const stage = document.createElement('div');
    const image = document.createElement('img');
    document.body.append(stage);
    stage.append(image);
    vi.spyOn(stage, 'getBoundingClientRect').mockReturnValue({
      x: 10, y: 20, top: 20, right: 210, bottom: 120, left: 10, width: 200, height: 100,
      toJSON: () => undefined,
    });
    vi.spyOn(image, 'getBoundingClientRect').mockReturnValue({
      x: 30, y: 35, top: 35, right: 190, bottom: 95, left: 30, width: 160, height: 60,
      toJSON: () => undefined,
    });
    const analysis = analyzeImageData({
      data: new Uint8ClampedArray([255, 255, 255, 255, 0, 0, 0, 255]),
      width: 2,
      height: 1,
    } as ImageData);
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => root.render(
      <ClippingOverlay
        analysis={analysis}
        enabled
        imageRef={{ current: image }}
        stageRef={{ current: stage }}
        viewer="A"
      />,
    ));

    const canvas = host.querySelector<HTMLCanvasElement>('canvas');
    expect(canvas).not.toBeNull();
    expect(canvas?.width).toBe(2);
    expect(canvas?.height).toBe(1);
    expect(canvas?.style.left).toBe('20px');
    expect(canvas?.style.top).toBe('15px');
    expect(canvas?.style.width).toBe('160px');
    expect(canvas?.style.height).toBe('60px');
    expect(context.putImageData).toHaveBeenCalled();

    await act(async () => root.unmount());
    host.remove();
    stage.remove();
    getContext.mockRestore();
  });

  it('does not request a render when a node is moved or the viewer is navigated', async () => {
    const requests: string[] = [];
    const controller = new ViewerController({
      requestPreview: (request) => {
        requests.push(`${request.nodeId}:${request.outputPort}`);
        return new Promise(() => undefined);
      },
      cancelPreview: async () => undefined,
      releasePreview: async () => undefined,
    });
    const host = document.createElement('div');
    document.body.append(host);
    const root = createRoot(host);
    const input = node('input', [{ id: 'image', name: 'Image', dataType: 'core.Image' }]);
    const output = node('output', [{ id: 'image', name: 'Image', dataType: 'core.Image' }]);
    output.typeId = 'core.output';
    const source = { kind: 'ordinary' as const, width: 640, height: 480, revision: 1, metadata: null };

    await act(async () => root.render(
      <Viewer controller={controller} nodes={[input, output]} revision={1} source={source} />,
    ));
    expect(requests).toEqual(['output:image']);

    const movedInput = { ...input, position: { x: 120, y: 60 } };
    await act(async () => root.render(
      <Viewer controller={controller} nodes={[movedInput, output]} revision={1} source={source} />,
    ));
    await act(async () => {
      controller.setZoom('A', 2);
      controller.updatePan('A', { x: 30, y: 20 });
    });

    expect(requests).toEqual(['output:image']);

    await act(async () => root.unmount());
    host.remove();
  });
});
