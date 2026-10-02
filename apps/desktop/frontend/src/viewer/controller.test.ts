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
  it.each([[1, 3], [2, 2]])('fits 6000x4000 at DPR %i using mip %i and full coordinates', (dpr, mip) => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 6000, height: 4000 });
    viewer.setViewport('A', { width: 750, height: 500 }, dpr);
    viewer.setTarget('A', target('output'));
    expect(transport.requests[0]).toMatchObject({ mip, region: { x: 0, y: 0, width: 6000, height: 4000 } });
    expect(viewer.state.panes.A.displayScale).toBe(0.125);
  });

  it('uses mip0 until a real viewport is known and validates DPR', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 6000, height: 4000 });
    viewer.setViewport('A', { width: 0, height: 0 });
    viewer.setTarget('A', target('output'));
    expect(transport.requests[0].mip).toBe(0);
    for (const ratio of [0, -1, NaN, Infinity]) {
      viewer.setViewport('A', { width: 750, height: 500 }, ratio);
    }
    expect(transport.requests).toHaveLength(1);
    viewer.setViewport('A', { width: 750, height: 500 });
    expect(transport.requests.at(-1)?.mip).toBe(3);
    viewer.setViewport('A', { width: 750, height: 500 }, 2);
    expect(transport.requests.at(-1)?.mip).toBe(2);
  });

  it('coalesces rapid same-mip zooms and upgrades to 100% without blanking the old frame', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 6000, height: 4000 });
    viewer.setViewport('A', { width: 750, height: 500 });
    viewer.setTarget('A', target('output'));
    const first = transport.requests[0];
    viewer.setZoom('A', 0.11);
    viewer.setZoom('A', 0.12);
    viewer.updatePan('A', { x: 50, y: 30 });
    expect(transport.requests).toHaveLength(1);
    expect(transport.cancellations).toEqual([]);
    transport.pending.get(first.requestId)!.resolve(result(first, { width: 750, height: 500, fullWidth: 6000, fullHeight: 4000 }));
    await Promise.resolve();
    viewer.setZoom('A', 0.124);
    expect(transport.requests).toHaveLength(1);
    viewer.viewAt100('A');
    const upgrade = transport.requests[1];
    expect(upgrade.mip).toBe(0);
    expect(viewer.state.panes.A.imageUrl).toBe(result(first).url);
    viewer.setZoom('A', 1.1);
    viewer.setZoom('A', 1.3);
    viewer.viewAt100('A');
    expect(transport.requests).toHaveLength(2);
    expect(transport.cancellations).toEqual([]);
    transport.pending.get(upgrade.requestId)!.resolve(result(upgrade, { width: 6000, height: 4000 }));
    await Promise.resolve();
    expect(viewer.state.panes.A).toMatchObject({ fullWidth: 6000, fullHeight: 4000, displayScale: 1 });
    expect(transport.releases).toEqual([result(first).url]);
  });

  it('resizes and refits only when the required mip changes', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 6000, height: 4000 });
    viewer.setViewport('A', { width: 750, height: 500 });
    viewer.setTarget('A', target('output'));
    viewer.setViewport('A', { width: 700, height: 480 });
    expect(transport.requests).toHaveLength(1);
    viewer.setViewport('A', { width: 1500, height: 1000 });
    viewer.setViewport('A', { width: 375, height: 250 });
    viewer.viewAt100('A');
    viewer.fitToWindow('A');
    expect(transport.requests.map((request) => request.mip)).toEqual([3, 2, 4, 0, 4]);
  });

  it.each(['100%', 'custom'] as const)('corrects resized output coordinates in %s mode and releases the incomplete frame', async (mode) => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 4000, height: 3000 });
    viewer.setViewport('A', { width: 750, height: 500 });
    if (mode === '100%') viewer.viewAt100('A');
    else viewer.setZoom('A', 0.5);
    viewer.setTarget('A', target('resize'));
    const first = transport.requests[0];
    transport.pending.get(first.requestId)!.resolve(result(first, { width: 400, height: 300, fullWidth: 800, fullHeight: 600 }));
    await Promise.resolve();
    const corrected = transport.requests[1];
    expect(corrected.region).toEqual({ x: 0, y: 0, width: 800, height: 600 });
    expect(corrected.mip).toBe(mode === '100%' ? 0 : 1);
    expect(transport.releases).toEqual([result(first).url]);
    transport.pending.get(corrected.requestId)!.resolve(result(corrected, { width: 800, height: 600 }));
    await Promise.resolve();
    expect(viewer.state.panes.A).toMatchObject({ status: 'ready', fullWidth: 800, fullHeight: 600, displayScale: mode === '100%' ? 1 : 0.5 });
    expect(transport.requests).toHaveLength(2);
  });

  it('bounds dimension correction when evaluated dimensions keep changing', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.viewAt100('A');
    viewer.setTarget('A', target('resize'));
    const first = transport.requests[0];
    transport.pending.get(first.requestId)!.resolve(result(first, { width: 800, height: 600 }));
    await Promise.resolve();
    const corrected = transport.requests[1];
    transport.pending.get(corrected.requestId)!.resolve(result(corrected, { width: 1600, height: 1200 }));
    await Promise.resolve();
    await Promise.resolve();
    expect(transport.requests).toHaveLength(2);
    expect(viewer.state.panes.A.status).toBe('error');
    expect(transport.releases).toEqual([result(first).url, result(corrected).url]);
  });

  it('releases cancelled mip arrivals and mismatched request/revision results without changing metadata', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 6000, height: 4000 });
    viewer.setViewport('A', { width: 750, height: 500 });
    viewer.setTarget('A', target('output'));
    const fit = transport.requests[0];
    viewer.viewAt100('A');
    const full = transport.requests[1];
    const stale = result(fit, { width: 750, height: 500, fullWidth: 6000, fullHeight: 4000 });
    transport.pending.get(fit.requestId)!.resolve(stale);
    await Promise.resolve();
    expect(transport.cancellations).toEqual([fit.requestId]);
    expect(viewer.state.panes.A).toMatchObject({ fullWidth: null, fullHeight: null, imageUrl: null });
    const wrongId = { ...result(full), requestId: 'other' };
    transport.pending.get(full.requestId)!.resolve(wrongId);
    await Promise.resolve();
    expect(transport.releases).toEqual([stale.url, wrongId.url]);
    expect(viewer.state.panes.A.imageUrl).toBeNull();
    viewer.setRevision(1);
    const revised = transport.requests[2];
    const wrongRevision = result(revised, {}, 0);
    transport.pending.get(revised.requestId)!.resolve(wrongRevision);
    await Promise.resolve();
    expect(transport.releases).toContain(wrongRevision.url);
    expect(viewer.state.panes.A.fullWidth).toBeNull();
  });

  it('retains full metadata for same-target loading and resets it when the frame is cleared', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 800, height: 600 });
    viewer.viewAt100('A');
    viewer.setTarget('A', target('output'));
    const request = transport.requests[0];
    transport.pending.get(request.requestId)!.resolve(result(request, { width: 800, height: 600 }));
    await Promise.resolve();
    viewer.setRevision(1);
    expect(viewer.state.panes.A).toMatchObject({ fullWidth: 800, fullHeight: 600, status: 'loading' });
    viewer.setSourceDimensions({ width: 1600, height: 1200 });
    expect(viewer.state.panes.A).toMatchObject({ fullWidth: 800, fullHeight: 600, status: 'loading' });
    await viewer.cancel('A');
    expect(viewer.state.panes.A).toMatchObject({ fullWidth: null, fullHeight: null, imageUrl: null });
    viewer.setTarget('A', target('other'));
    expect(viewer.state.panes.A).toMatchObject({ fullWidth: null, fullHeight: null });
  });

  it('caps small display requests at mip6', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 1000000, height: 1000000 });
    viewer.setViewport('A', { width: 1, height: 1 });
    viewer.setTarget('A', target('output'));
    expect(transport.requests[0].mip).toBe(6);
  });

  it('reports evaluated target dimensions at 100% and rejects stale metadata', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    const upstream = target('resize');
    viewer.setSourceDimensions({ width: 4000, height: 3000 });
    viewer.viewAt100('A');
    viewer.setTarget('A', upstream);
    const request = transport.requests[0];
    expect(viewer.getTargetDimensions(upstream)).toBeNull();
    transport.pending.get(request.requestId)!.resolve(result(request, { width: 800, height: 600 }));
    await Promise.resolve();
    const corrected = transport.requests.at(-1)!;
    expect(corrected.region).toEqual({ x: 0, y: 0, width: 800, height: 600 });
    expect(viewer.getTargetDimensions(upstream)).toBeNull();
    transport.pending.get(corrected.requestId)!.resolve(result(corrected, { width: 800, height: 600 }));
    await Promise.resolve();
    expect(viewer.getTargetDimensions(upstream)).toEqual({ width: 800, height: 600 });
    expect(viewer.getTargetDimensions(target('other'))).toBeNull();
    viewer.setRevision(1);
    expect(viewer.getTargetDimensions(upstream)).toBeNull();
  });
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
    viewer.updatePan('B', { x: 18, y: -6 });
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

  it('requests original resolution at 100% and changes mip only at quality boundaries', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.viewAt100('A');
    viewer.setTarget('A', target('output'));

    expect(transport.requests).toHaveLength(1);
    expect(transport.requests[0]).toMatchObject({
      region: { x: 0, y: 0, width: 400, height: 300 },
      mip: 0,
      quality: 'preview',
    });

    viewer.setZoom('A', 0.5);
    expect(transport.requests).toHaveLength(2);
    expect(transport.requests[1].mip).toBe(1);
    expect(viewer.state.panes.A.zoom).toBe(0.5);
    expect(viewer.state.panes.A.displayScale).toBe(0.5);
  });

  it('pans without starting a render', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.viewAt100('A');
    viewer.setTarget('A', target('output'));

    viewer.updatePan('A', { x: 10, y: 4 });
    viewer.updatePan('A', { x: 24, y: -8 });
    viewer.updatePan('A', { x: 31, y: -12 });

    expect(viewer.state.panes.A.pan).toEqual({ x: 31, y: -12 });
    expect(transport.requests).toHaveLength(1);
  });

  it('refits and requests the new detail level when the viewport changes', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.setTarget('A', target('output'));

    viewer.setViewport('A', { width: 200, height: 160 });

    expect(transport.requests.map((request) => request.mip)).toEqual([2, 1]);
    expect(viewer.state.panes.A.zoom).toBe(0.5);
    expect(viewer.state.panes.A.displayScale).toBe(0.5);
  });

  it('renders for quality transitions and graph revisions, not pan or same-mip viewport changes', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setRevision(1);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.setTarget('A', target('output'));
    expect(transport.requests).toHaveLength(1);

    viewer.setZoom('A', 2);
    viewer.updatePan('A', { x: 40, y: -15 });
    viewer.setViewport('A', { width: 120, height: 90 });
    expect(transport.requests.map((request) => request.mip)).toEqual([2, 0]);
    viewer.fitToWindow('A');
    viewer.viewAt100('A');
    expect(transport.requests.map((request) => request.mip)).toEqual([2, 0, 1, 0]);

    viewer.setRevision(2);
    expect(transport.requests).toHaveLength(5);
    expect(transport.requests[4].mip).toBe(0);
  });

  it('fits a mip preview using a full-resolution region', () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.setTarget('A', target('output'));

    expect(transport.requests[0]).toMatchObject({
      region: { x: 0, y: 0, width: 400, height: 300 },
      mip: 2,
      quality: 'preview',
    });
    expect(viewer.state.panes.A.zoom).toBe(0.25);
    expect(viewer.state.panes.A.displayScale).toBe(0.25);
  });

  it('does not clamp a fit below the interactive zoom floor', () => {
    const viewer = new ViewerController(new FakeTransport());
    viewer.setSourceDimensions({ width: 6000, height: 4000 });
    viewer.setViewport('A', { width: 300, height: 150 });
    viewer.setTarget('A', target('output'));

    // 150 / 4000 = 0.0375, below the 0.1 interactive floor. Clamping it there
    // made the "fit" preview overflow a small panel.
    expect(Math.min(300 / 6000, 150 / 4000)).toBeLessThan(0.1);
    expect(viewer.state.panes.A.zoomMode).toBe('fit');
    expect(viewer.state.panes.A.zoom).toBeCloseTo(150 / 4000, 6);
  });

  it('shrinks the image when zooming out from a fit below the interactive floor', () => {
    const viewer = new ViewerController(new FakeTransport());
    viewer.setSourceDimensions({ width: 6000, height: 4000 });
    viewer.setViewport('A', { width: 300, height: 150 });
    viewer.setTarget('A', target('output'));
    const fit = viewer.state.panes.A.zoom;
    expect(fit).toBeCloseTo(150 / 4000, 6);

    viewer.adjustZoom('A', -1);
    const firstStep = viewer.state.panes.A.zoom;
    expect(firstStep).toBeLessThan(fit);

    // The clamp used to snap back to MIN_ZOOM, which equalled the current zoom,
    // so "zoom out" both enlarged the frame and left the control dead.
    viewer.adjustZoom('A', -1);
    const secondStep = viewer.state.panes.A.zoom;
    expect(secondStep).toBeLessThan(firstStep);

    // Zooming back in past the floor still works.
    viewer.adjustZoom('A', 1);
    expect(viewer.state.panes.A.zoom).toBeGreaterThan(secondStep);
    expect(viewer.state.panes.A.zoomMode).toBe('custom');
  });

  it('keeps the interactive zoom floor for frames that fit comfortably', () => {
    const viewer = new ViewerController(new FakeTransport());
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 400, height: 300 });
    viewer.setTarget('A', target('output'));

    for (let step = 0; step < 40; step += 1) viewer.adjustZoom('A', -1);
    expect(viewer.state.panes.A.zoom).toBeCloseTo(0.1, 6);
  });

  it('retains full-size metadata and the old frame while upgrading a mip preview', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.setTarget('A', target('output'));

    const request = transport.requests[0];
    expect(request).toMatchObject({
      region: { x: 0, y: 0, width: 400, height: 300 },
      mip: 2,
    });
    transport.pending.get(request.requestId)!.resolve(result(request, { width: 100, height: 75, fullWidth: 400, fullHeight: 300 }));
    await Promise.resolve();

    expect(viewer.state.panes.A.width).toBe(100);
    expect(viewer.state.panes.A.height).toBe(75);
    expect(viewer.state.panes.A).toMatchObject({ fullWidth: 400, fullHeight: 300 });
    expect(viewer.state.panes.A.displayScale).toBe(0.25);

    viewer.setZoom('A', 0.5);
    expect(transport.requests).toHaveLength(2);
    expect(transport.requests[1].mip).toBe(1);
    expect(viewer.state.panes.A.imageUrl).toBe(result(request).url);
    expect(viewer.state.panes.A).toMatchObject({ fullWidth: 400, fullHeight: 300, status: 'loading' });
    expect(transport.releases).toEqual([]);
    expect(viewer.state.panes.A.displayScale).toBe(0.5);
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
    expect(viewer.state.panes.A.displayScale).toBe(0.125);
  });

  it('keeps the loaded preview visible across navigation and replaces it on a node setting change', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setRevision(1);
    viewer.setSourceDimensions({ width: 400, height: 300 });
    viewer.setViewport('A', { width: 100, height: 80 });
    viewer.setTarget('A', target('output'));
    const first = transport.requests[0];
    transport.pending.get(first.requestId)!.resolve(result(first, { fullWidth: 400, fullHeight: 300 }));
    await Promise.resolve();
    const firstUrl = viewer.state.panes.A.imageUrl;
    expect(firstUrl).toBeTruthy();

    // Zoom inside the same quality band and pan reuse the frame.
    viewer.setZoom('A', 0.23);
    viewer.updatePan('A', { x: 40, y: -20 });
    expect(viewer.state.panes.A.imageUrl).toBe(firstUrl);
    expect(transport.releases).toEqual([]);
    expect(transport.requests).toHaveLength(1);

    // A node setting change (new graph revision) does replace the frame.
    viewer.setRevision(2);
    expect(viewer.state.panes.A.imageUrl).toBe(firstUrl);

    const second = transport.requests.at(-1)!;
    transport.pending.get(second.requestId)!.resolve(result(second, { fullWidth: 400, fullHeight: 300 }));
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

  it('does not clear a replacement frame when an older cancellation is acknowledged', async () => {
    const transport = new FakeTransport();
    const acknowledgement = deferred<void>();
    vi.spyOn(transport, 'cancelPreview').mockReturnValue(acknowledgement.promise);
    const viewer = new ViewerController(transport);
    viewer.setTarget('A', target('old'));
    const cancellation = viewer.cancel('A');
    viewer.setTarget('A', target('replacement'));
    const replacement = transport.requests.at(-1)!;
    transport.pending.get(replacement.requestId)!.resolve(result(replacement));
    await Promise.resolve();
    acknowledgement.resolve(undefined);
    await cancellation;
    expect(viewer.state.panes.A.status).toBe('ready');
    expect(viewer.state.panes.A.imageUrl).toContain(replacement.requestId);
    expect(transport.releases).not.toContain(result(replacement).url);
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
