import { describe, expect, it } from 'vitest';
import { ViewerController } from './controller';
import type { PreviewRequest, PreviewResult, PreviewTarget, ViewerId } from './types';
import type { PreviewTransport } from './transport';

function target(nodeId: string, name = nodeId): PreviewTarget {
  return { nodeId, nodeName: name, outputPort: 'image', outputName: 'Image' };
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

function result(request: PreviewRequest, revision = request.revision): PreviewResult {
  return {
    requestId: request.requestId,
    revision,
    url: `rawweave-preview://localhost/preview/${request.requestId}.png`,
    width: 2,
    height: 2,
    fullWidth: 2,
    fullHeight: 2,
    mimeType: 'image/png',
  };
}

describe('viewer controller', () => {
  it('cancels a superseded request and rejects its stale result', async () => {
    const transport = new FakeTransport();
    const viewer = new ViewerController(transport);
    viewer.setRevision(7);

    viewer.setTarget('A', target('exposure'));
    const first = transport.requests[0];
    viewer.setTarget('A', target('invert'));
    const second = transport.requests[1];

    expect(transport.cancellations).toEqual([first.requestId]);
    transport.pending.get(first.requestId)!.resolve(result(first));
    await Promise.resolve();
    expect(viewer.state.panes.A.imageUrl).toBeNull();

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
    transport.pending.get(request.requestId)!.resolve(result(request, 10));
    await Promise.resolve();

    expect(viewer.state.panes.B.imageUrl).toBeNull();
    expect(viewer.state.panes.B.status).toBe('loading');
    expect(transport.requests).toHaveLength(2);
    expect(transport.cancellations).toContain(request.requestId);
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
});
