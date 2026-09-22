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
});
