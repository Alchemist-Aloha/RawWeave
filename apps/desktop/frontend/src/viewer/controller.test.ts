import { afterEach, describe, expect, it, vi } from 'vitest';
import { ViewerController } from './controller';
import type { PreviewRequest, PreviewResult, PreviewTarget, ViewerId } from './types';
import type { PreviewTransport } from './transport';

function target(nodeId: string, name = nodeId, dataType = 'core.Image'): PreviewTarget {
  return { nodeId, nodeName: name, outputPort: 'image', outputName: 'Image', dataType };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });
  return { promise, resolve, reject };
}

class FakeTransport implements PreviewTransport {
  requests: PreviewRequest[] = [];
  cancellations: string[] = [];
  releases: string[] = [];
  pending = new Map<string, ReturnType<typeof deferred<PreviewResult>>>();

  requestPreview(request: PreviewRequest, onProgress: (progress: number) => void): Promise<PreviewResult> {
    this.requests.push(request);
    onProgress(0.25);
    const result = deferred<PreviewResult>();
    this.pending.set(request.requestId, result);
    return result.promise;
  }

  async cancelPreview(requestId: string): Promise<void> {
    this.cancellations.push(requestId);
  }

  async releasePreview(url: string): Promise<void> {
    this.releases.push(url);
  }
}

function result(
  request: PreviewRequest,
  dimensions: Partial<{ width: number; height: number; fullWidth?: number; fullHeight?: number }> = {},
  revision = request.revision,
): PreviewResult {
  const width = dimensions.width ?? 1;
  const height = dimensions.height ?? 1;
  return {
    requestId: request.requestId,
    revision,
    url: `rawweave-preview://localhost/preview/${request.requestId}.png`,
    width,
    height,
    fullWidth: dimensions.fullWidth ?? width,
    fullHeight: dimensions.fullHeight ?? height,
    mimeType: 'image/png',
  };
}

describe('viewer controller', () => {
  afterEach(() => {
    vi.useRealTimers();
  });
  it('cancels a superseded request and rejects its stale result', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setRevision(7);

    viewer.setTarget('A', target('exposure'));
    const first = transport.requests[0];
    viewer.setTarget('A', target('invert'));
    const second = transport.requests[1];

    expect(transport.cancellations).toEqual([first.requestId]);
    const staleResult = result(first);
    transport.pending.get(first.requestId)!.resolve(staleResult);
    await Promise.resolve();
    expect(viewer.state.panes.A.imageUrl).toBeNull();
    expect(transport.releases).toEqual([staleResult.url]);

    transport.pending.get(second.requestId)!.resolve(result(second));
    await Promise.resolve();
    expect(viewer.state.panes.A.imageUrl).toContain(second.requestId);
  });

  it('ignores a completed render from an old graph revision', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setRevision(10);
    viewer.setTarget('B', target('output'));
    const request = transport.requests[0];

    viewer.setRevision(11);
    const staleResult = result(request, {}, 10);
    transport.pending.get(request.requestId)!.resolve(staleResult);
    await Promise.resolve();

    expect(viewer.state.panes.B.imageUrl).toBeNull();
    expect(transport.releases).toEqual([staleResult.url]);
    expect(viewer.state.panes.B.status).toBe('loading');
    expect(transport.requests).toHaveLength(2);
    expect(transport.cancellations).toContain(request.requestId);
  });

  it('only publishes session changes for target and layout changes', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    let sessionChanges = 0;
    viewer.subscribeSession(() => {
      sessionChanges += 1;
    });

    viewer.setTarget('A', target('output'));
    expect(sessionChanges).toBe(1);

    viewer.updatePan('A', { x: 20, y: 10 });
    expect(sessionChanges).toBe(1);

    viewer.setLayout('split');
    expect(sessionChanges).toBe(2);
  });

  it('keeps A/B targets and zoom controls independent', () => {
    const viewer = new ViewerController(new FakeTransport());
    viewer.setTarget('A', target('resize', 'Resize'));
    viewer.setTarget('B', target('blur', 'Blur'));
    viewer.setZoom('A', 2);
    viewer.setPan('B', { x: 18, y: -6 });
    viewer.fitToWindow('A');
    viewer.viewAt100('B');

    expect(viewer.state.panes.A.target).toEqual(target('resize', 'Resize'));
    expect(viewer.state.panes.B.target).toEqual(target('blur', 'Blur'));
    expect(viewer.state.panes.A.zoomMode).toBe('fit');
    expect(viewer.state.panes.B.zoomMode).toBe('100%');
    expect(viewer.state.panes.A.pan).toEqual({ x: 0, y: 0 });
    expect(viewer.state.panes.B.pan).toEqual({ x: 18, y: -6 });
  });

  it('stores comparison and clipping display modes independently from render targets', () => {
    const viewer = new ViewerController(new FakeTransport());
    viewer.setTarget('A', target('before', 'Before'));
    viewer.setTarget('B', target('after', 'After'));

    viewer.setComparison('difference');
    viewer.setClippingOverlay(true);

    expect(viewer.state.comparison).toBe('difference');
    expect(viewer.state.clippingOverlay).toBe(true);
    expect(viewer.state.panes.A.target).toEqual(target('before', 'Before'));
    expect(viewer.state.panes.B.target).toEqual(target('after', 'After'));
  });

  it('reports progress while a target is rendering', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setTarget('A', target('output'));

    expect(viewer.state.panes.A.status).toBe('loading');
    expect(viewer.state.panes.A.progress).toBe(0.25);
  });

  it('requests the visible region and a lower mip while navigating a large image', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.viewAt100('A');
    viewer.setTarget('A', target('output'));

    expect(transport.requests[0]).toMatchObject({
      region: { x: 150, y: 110, width: 100, height: 80 },
      mip: 0,
      tile: { x: 4, y: 3 },
      quality: 'preview',
    });

    viewer.setZoom('A', 0.5);
    const navigationRequest = transport.requests.at(-1)!;
    expect(navigationRequest).toMatchObject({
      region: { x: 100, y: 70, width: 200, height: 160 },
      mip: 1,
      tile: { x: 3, y: 2 },
      quality: 'draft',
    });
  });

  it('coalesces pointer pan updates into one preview request when the gesture ends', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.viewAt100('A');
    viewer.setTarget('A', target('output'));

    viewer.beginPan('A');
    viewer.updatePan('A', { x: 10, y: 4 });
    viewer.updatePan('A', { x: 24, y: -8 });
    viewer.updatePan('A', { x: 31, y: -12 });

    expect(viewer.state.panes.A.pan).toEqual({ x: 31, y: -12 });
    expect(transport.requests).toHaveLength(1);

    viewer.endPan('A');

    expect(transport.requests).toHaveLength(2);
    expect(transport.requests.at(-1)).toMatchObject({
      region: { x: 119, y: 122, width: 100, height: 80 },
    });
  });

  it('renders a settled pan after the debounce when no pointer-up arrives', () => {
    vi.useFakeTimers();
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.viewAt100('A');
    viewer.setTarget('A', target('output'));

    viewer.beginPan('A');
    viewer.updatePan('A', { x: 20, y: 10 });
    viewer.updatePan('A', { x: 25, y: 15 });
    expect(transport.requests).toHaveLength(1);

    vi.advanceTimersByTime(100);

    expect(transport.requests).toHaveLength(2);
  });

  it('fits the complete image by requesting the full frame at viewport resolution', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.setTarget('A', target('output'));

    expect(transport.requests[0]).toMatchObject({
      region: { x: 0, y: 0, width: 400, height: 300 },
      mip: 2,
      quality: 'draft',
    });
    expect(viewer.state.panes.A.zoom).toBe(0.25);
    expect(viewer.state.panes.A.displayScale).toBe(1);
  });

  it('compensates a zoomed-out mip in the image display scale', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.viewAt100('A');
    viewer.setTarget('A', target('output'));
    viewer.setZoom('A', 0.5);

    const request = transport.requests.at(-1)!;
    expect(request).toMatchObject({
      region: { x: 100, y: 70, width: 200, height: 160 },
      mip: 1,
    });
    transport.pending.get(request.requestId)!.resolve(result(request, { width: 100, height: 80, fullWidth: 400, fullHeight: 300 }));
    await Promise.resolve();

    expect(viewer.state.panes.A.width).toBe(100);
    expect(viewer.state.panes.A.height).toBe(80);
    expect(viewer.state.panes.A.displayScale).toBe(1);
  });

  it('re-fits and keeps true output dimensions for an intermediate resize', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.setTarget('A', target('resize'));

    const first = transport.requests[0];
    transport.pending.get(first.requestId)!.resolve(result(first, { width: 100, height: 75, fullWidth: 800, fullHeight: 600 }));
    await Promise.resolve();

    expect(transport.requests.at(-1)).toMatchObject({
      region: { x: 0, y: 0, width: 800, height: 600 },
      mip: 3,
    });
    expect(viewer.state.panes.A.zoom).toBe(0.125);
    expect(viewer.state.panes.A.displayScale).toBe(1);
  });

  it('keeps the current preview visible while a replacement renders', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.setTarget('A', target('output'));
    const first = transport.requests[0];
    transport.pending.get(first.requestId)!.resolve(result(first, { fullWidth: 400, fullHeight: 300 }));
    await Promise.resolve();
    const firstUrl = viewer.state.panes.A.imageUrl;
    expect(firstUrl).toBeTruthy();

    viewer.setZoom('A', 0.5);

    // The old frame stays on screen and stays alive until the new one lands.
    expect(viewer.state.panes.A.imageUrl).toBe(firstUrl);
    expect(transport.releases).toEqual([]);

    const second = transport.requests.at(-1)!;
    transport.pending.get(second.requestId)!.resolve(result(second));
    await Promise.resolve();

    expect(viewer.state.panes.A.imageUrl).not.toBe(firstUrl);
    expect(transport.releases).toEqual([firstUrl]);
  });

  it('keeps the current preview visible across a revision change', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setRevision(1);
    viewer.setTarget('A', target('output'));
    const first = transport.requests[0];
    transport.pending.get(first.requestId)!.resolve(result(first));
    await Promise.resolve();
    const firstUrl = viewer.state.panes.A.imageUrl;

    viewer.setRevision(2);

    expect(viewer.state.panes.A.imageUrl).toBe(firstUrl);
    expect(viewer.state.panes.A.status).toBe('loading');

    const second = transport.requests.at(-1)!;
    transport.pending.get(second.requestId)!.resolve(result(second));
    await Promise.resolve();

    expect(transport.releases).toEqual([firstUrl]);
  });

  it('releases an old protocol URL when a pane replaces its preview', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setTarget('A', target('output'));
    const request = transport.requests[0];
    transport.pending.get(request.requestId)!.resolve(result(request));
    await Promise.resolve();

    const oldUrl = viewer.state.panes.A.imageUrl!;
    viewer.setTarget('A', target('invert'));
    expect(transport.releases).toEqual([oldUrl]);
  });

  it.each(['A', 'B'] as ViewerId[])('can cancel %s explicitly', async (pane) => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setTarget(pane, target('output'));
    const requestId = transport.requests[0].requestId;

    await viewer.cancel(pane);

    expect(transport.cancellations).toEqual([requestId]);
    expect(viewer.state.panes[pane].status).toBe('cancelled');
  });

  it('clears both preview targets when the source workflow changes', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setRevision(4);
    viewer.setTarget('A', target('raw-display'));
    viewer.setTarget('B', target('raw-display'));

    viewer.clearTargets();

    expect(viewer.state.panes.A.target).toBeNull();
    expect(viewer.state.panes.B.target).toBeNull();
    expect(transport.cancellations).toHaveLength(2);
  });

  it('requests a mask target in grayscale and can toggle its colored overlay', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);

    viewer.setTarget('A', target('mask', 'Painted Mask', 'core.Mask'));
    expect(transport.requests[0].maskDisplay).toBe('grayscale');
    expect(viewer.state.panes.A.maskDisplay).toBe('grayscale');

    viewer.setMaskDisplay('A', 'overlay');

    expect(transport.requests.at(-1)?.maskDisplay).toBe('overlay');
    expect(viewer.state.panes.A.maskDisplay).toBe('overlay');
  });
});
