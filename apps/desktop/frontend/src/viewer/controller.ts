import type {
  ImageDimensions,
  PreviewRegion,
  PreviewRequest,
  PreviewTarget,
  ViewerId,
  ViewerPaneState,
  ViewerState,
  ViewerComparison,
} from './types';
import type { PreviewTransport } from './transport';

interface ActiveRequest {
  request: PreviewRequest;
}

const MIN_ZOOM = 0.1;
const MAX_ZOOM = 8;

function paneState(): ViewerPaneState {
  return {
    target: null,
    imageUrl: null,
    width: null,
    height: null,
    status: 'idle',
    progress: 0,
    error: null,
    requestId: null,
    zoom: 1,
    displayScale: 1,
    zoomMode: 'fit',
    pan: { x: 0, y: 0 },
    maskDisplay: 'grayscale',
    imageRegion: null,
    imageMip: 0,
    imageOrigin: { x: 0, y: 0 },
  };
}

function clampZoom(value: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, value));
}

const TILE_SIZE = 32;
const MAX_MIP = 8;
const DEFAULT_DIMENSIONS: ImageDimensions = { width: 1, height: 1 };
const DEFAULT_VIEWPORT: ImageDimensions = { width: 1, height: 1 };

interface RequestPlan {
  region: PreviewRegion;
  mip: number;
  zoom: number;
}

/**
 * The scale that fits the whole frame inside the viewport.
 *
 * Fit is deliberately not clamped to the interactive zoom range: a large image
 * in a small panel needs a scale below `MIN_ZOOM`, and clamping it there is what
 * let the "fit" image overflow the panel. `mipForZoom` still caps the render
 * resolution and `displayScale` compensates for whatever mip is loaded.
 */
function fitZoom(dimensions: ImageDimensions, viewport: ImageDimensions): number {
  const fit = Math.min(viewport.width / dimensions.width, viewport.height / dimensions.height);
  return Number.isFinite(fit) && fit > 0 ? fit : 1;
}

function mipForZoom(zoom: number): number {
  return Math.min(MAX_MIP, Math.max(0, Math.floor(Math.log2(1 / zoom))));
}

function sameRegion(first: PreviewRegion, second: PreviewRegion): boolean {
  return (
    first.x === second.x &&
    first.y === second.y &&
    first.width === second.width &&
    first.height === second.height
  );
}

export class ViewerController {
  public state: ViewerState = {
    currentRevision: 0,
    layout: 'side-by-side',
    comparison: 'side-by-side',
    clippingOverlay: false,
    panes: { A: paneState(), B: paneState() },
  };

  private readonly listeners = new Set<(state: ViewerState) => void>();
  private readonly sessionListeners = new Set<() => void>();
  private readonly active = new Map<ViewerId, ActiveRequest>();
  private readonly viewports = new Map<ViewerId, ImageDimensions>();
  private readonly imageDimensions = new Map<ViewerId, ImageDimensions>();
  private sourceDimensions: ImageDimensions = DEFAULT_DIMENSIONS;
  private sequence = 0;

  public constructor(private readonly transport: PreviewTransport) {}

  public subscribe(listener: (state: ViewerState) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  public subscribeSession(listener: () => void): () => void {
    this.sessionListeners.add(listener);
    return () => this.sessionListeners.delete(listener);
  }

  private publish(): void {
    for (const listener of this.listeners) listener(this.state);
  }

  private publishSession(): void {
    for (const listener of this.sessionListeners) listener();
  }

  /** Applies a new preview frame and releases the frame it replaces. */
  private replacePanePreview(viewer: ViewerId, patch: Partial<ViewerPaneState>): void {
    const previous = this.state.panes[viewer].imageUrl;
    this.setPane(viewer, patch);
    if (previous && previous !== patch.imageUrl) this.releasePreview(previous);
  }

  private setPane(viewer: ViewerId, patch: Partial<ViewerPaneState>): void {
    this.state = {
      ...this.state,
      panes: { ...this.state.panes, [viewer]: { ...this.state.panes[viewer], ...patch } },
    };
    this.publish();
  }

  private requestId(viewer: ViewerId): string {
    this.sequence += 1;
    return `viewer-${viewer.toLowerCase()}-${this.sequence}`;
  }

  private releasePanePreview(viewer: ViewerId): void {
    const url = this.state.panes[viewer].imageUrl;
    if (url) this.releasePreview(url);
  }

  private releasePreview(url: string): void {
    void this.transport.releasePreview(url).catch(() => undefined);
  }

  public reportImageLoadFailure(viewer: ViewerId, url: string): void {
    if (this.state.panes[viewer].imageUrl !== url) return;
    this.releasePreview(url);
    this.setPane(viewer, {
      imageUrl: null,
      width: null,
      height: null,
      status: 'error',
      error: `preview image could not be loaded from ${url}`,
    });
  }

  private restartRequest(viewer: ViewerId): void {
    const target = this.state.panes[viewer].target;
    if (!target) return;
    const current = this.active.get(viewer);
    if (current) {
      this.active.delete(viewer);
      void this.transport.cancelPreview(current.request.requestId);
    }
    // Keep the previous preview on screen while the replacement renders; the
    // old URL is released once the new one is applied. Blanking here makes the
    // pane flash on every source or revision change.
    this.startRequest(viewer, target);
  }

  /**
   * Plans the render for a target/revision change.
   *
   * The complete frame is always requested so that viewer navigation (pan,
   * zoom, viewport resize) is a client-side transform of the loaded preview and
   * never triggers another render. The mip level is chosen for the current
   * fit-to-viewport scale, which is the resolution the viewer can actually show.
   */
  private requestPlan(viewer: ViewerId): RequestPlan {
    const pane = this.state.panes[viewer];
    const dimensions = this.imageDimensions.get(viewer) ?? this.sourceDimensions;
    const viewport = this.viewports.get(viewer) ?? DEFAULT_VIEWPORT;
    const fit = fitZoom(dimensions, viewport);
    const zoom = pane.zoomMode === 'fit' ? fit : pane.zoom;
    return {
      region: { x: 0, y: 0, width: dimensions.width, height: dimensions.height },
      mip: mipForZoom(fit),
      zoom,
    };
  }

  public setSourceDimensions(dimensions: ImageDimensions): void {
    this.sourceDimensions = {
      width: Math.max(1, Math.floor(dimensions.width)),
      height: Math.max(1, Math.floor(dimensions.height)),
    };
    this.imageDimensions.clear();
    for (const viewer of ['A', 'B'] as ViewerId[]) {
      if (this.state.panes[viewer].target) this.restartRequest(viewer);
    }
  }

  public setViewport(viewer: ViewerId, viewport: ImageDimensions): void {
    const next = {
      width: Math.max(1, Math.floor(viewport.width)),
      height: Math.max(1, Math.floor(viewport.height)),
    };
    const previous = this.viewports.get(viewer);
    if (previous?.width === next.width && previous.height === next.height) return;
    this.viewports.set(viewer, next);
    // Resizing only refits the loaded preview; it must not start another render.
    if (!this.state.panes[viewer].target || this.state.panes[viewer].zoomMode !== 'fit') return;
    const dimensions = this.imageDimensions.get(viewer) ?? this.sourceDimensions;
    const zoom = fitZoom(dimensions, next);
    this.setPane(viewer, { zoom, displayScale: this.displayScaleFor(viewer, zoom) });
  }

  public setRevision(revision: number): void {
    if (revision === this.state.currentRevision) return;
    this.state = { ...this.state, currentRevision: revision };
    for (const [viewer, active] of this.active) {
      this.active.delete(viewer);
      void this.transport.cancelPreview(active.request.requestId);
    }
    for (const viewer of ['A', 'B'] as ViewerId[]) {
      this.imageDimensions.delete(viewer);
      const target = this.state.panes[viewer].target;
      if (!target) {
        this.releasePanePreview(viewer);
        this.setPane(viewer, {
          status: 'idle',
          requestId: null,
          progress: 0,
          imageUrl: null,
          width: null,
          height: null,
          error: null,
        });
        continue;
      }
      // The previous frame stays visible until the new revision arrives so the
      // pane does not flash empty on every graph change.
      this.setPane(viewer, { status: 'loading', requestId: null, progress: 0, error: null });
      this.startRequest(viewer, target);
    }
    this.publish();
  }

  public setLayout(layout: ViewerState['layout']): void {
    if (layout === this.state.layout) return;
    this.state = { ...this.state, layout };
    this.publish();
    this.publishSession();
  }

  public setComparison(comparison: ViewerComparison): void {
    if (comparison === this.state.comparison) return;
    const layout = comparison === 'side-by-side' ? this.state.layout : 'side-by-side';
    const layoutChanged = layout !== this.state.layout;
    this.state = {
      ...this.state,
      comparison,
      layout,
    };
    this.publish();
    if (layoutChanged) this.publishSession();
  }

  public setClippingOverlay(enabled: boolean): void {
    if (enabled === this.state.clippingOverlay) return;
    this.state = { ...this.state, clippingOverlay: enabled };
    this.publish();
  }

  public setTarget(viewer: ViewerId, target: PreviewTarget | null): void {
    const current = this.active.get(viewer);
    if (current) {
      this.active.delete(viewer);
      void this.transport.cancelPreview(current.request.requestId);
    }
    this.releasePanePreview(viewer);
    this.imageDimensions.delete(viewer);
    this.setPane(viewer, {
      target,
      imageUrl: null,
      width: null,
      height: null,
      status: target ? 'loading' : 'idle',
      progress: 0,
      error: null,
      requestId: null,
      maskDisplay: target?.dataType === 'core.Mask' ? this.state.panes[viewer].maskDisplay : 'grayscale',
      imageRegion: null,
      imageOrigin: { x: 0, y: 0 },
    });
    this.publishSession();
    if (target) this.startRequest(viewer, target);
  }

  public clearTargets(): void {
    for (const viewer of ['A', 'B'] as ViewerId[]) this.setTarget(viewer, null);
  }

  private startRequest(viewer: ViewerId, target: PreviewTarget): void {
    const plan = this.requestPlan(viewer);
    const request: PreviewRequest = {
      requestId: this.requestId(viewer),
      revision: this.state.currentRevision,
      nodeId: target.nodeId,
      outputPort: target.outputPort,
      quality: plan.mip > 0 ? 'draft' : 'preview',
      region: plan.region,
      tile: { x: Math.floor(plan.region.x / TILE_SIZE), y: Math.floor(plan.region.y / TILE_SIZE) },
      mip: plan.mip,
      maskDisplay: this.state.panes[viewer].maskDisplay,
    };
    this.active.set(viewer, { request });
    this.setPane(viewer, {
      zoom: plan.zoom,
      displayScale: plan.zoom * 2 ** plan.mip,
      imageRegion: request.region,
      imageMip: plan.mip,
      status: 'loading',
      requestId: request.requestId,
      progress: 0,
      error: null,
    });
    void this.transport
      .requestPreview(request, (progress) => {
        if (this.active.get(viewer)?.request.requestId !== request.requestId) return;
        this.setPane(viewer, { progress: Math.min(1, Math.max(0, progress)) });
      })
      .then((result) => {
        const current = this.active.get(viewer);
        if (
          !current ||
          current.request.requestId !== request.requestId ||
          request.revision !== this.state.currentRevision ||
          result.requestId !== request.requestId ||
          result.revision !== this.state.currentRevision
        ) {
          this.releasePreview(result.url);
          return;
        }
        this.active.delete(viewer);
        this.imageDimensions.set(viewer, {
          width: Math.max(1, result.fullWidth),
          height: Math.max(1, result.fullHeight),
        });
        const nextPlan = this.requestPlan(viewer);
        if (
          this.state.panes[viewer].zoomMode === 'fit' &&
          (!sameRegion(request.region, nextPlan.region) || request.mip !== nextPlan.mip)
        ) {
          this.releasePreview(result.url);
          this.startRequest(viewer, target);
          return;
        }
        this.replacePanePreview(viewer, {
          imageUrl: result.url,
          width: result.width,
          height: result.height,
          status: 'ready',
          progress: 1,
          error: null,
          requestId: null,
          imageOrigin: { x: result.originX ?? 0, y: result.originY ?? 0 },
        });
      })
      .catch((error: unknown) => {
        if (this.active.get(viewer)?.request.requestId !== request.requestId) return;
        this.active.delete(viewer);
        this.setPane(viewer, {
          status: 'error',
          error: error instanceof Error ? error.message : String(error),
          requestId: null,
        });
      });
  }

  public async cancel(viewer: ViewerId): Promise<void> {
    const active = this.active.get(viewer);
    if (!active) return;
    this.active.delete(viewer);
    await this.transport.cancelPreview(active.request.requestId);
    this.releasePanePreview(viewer);
    this.setPane(viewer, {
      status: 'cancelled',
      requestId: null,
      progress: 0,
      imageUrl: null,
      width: null,
      height: null,
      error: null,
    });
  }

  private displayScaleFor(viewer: ViewerId, zoom: number): number {
    return zoom * 2 ** this.state.panes[viewer].imageMip;
  }

  /** Zoom changes are applied to the loaded preview; they never start a render. */
  public setZoom(viewer: ViewerId, zoom: number): void {
    const nextZoom = clampZoom(zoom);
    if (nextZoom === this.state.panes[viewer].zoom && this.state.panes[viewer].zoomMode === 'custom') {
      return;
    }
    this.setPane(viewer, {
      zoom: nextZoom,
      zoomMode: 'custom',
      displayScale: this.displayScaleFor(viewer, nextZoom),
    });
  }

  public adjustZoom(viewer: ViewerId, delta: number): void {
    this.setZoom(viewer, this.state.panes[viewer].zoom * (delta > 0 ? 1.1 : 1 / 1.1));
  }

  public fitToWindow(viewer: ViewerId): void {
    const dimensions = this.imageDimensions.get(viewer) ?? this.sourceDimensions;
    const viewport = this.viewports.get(viewer) ?? DEFAULT_VIEWPORT;
    const zoom = fitZoom(dimensions, viewport);
    this.setPane(viewer, {
      zoom,
      zoomMode: 'fit',
      pan: { x: 0, y: 0 },
      displayScale: this.displayScaleFor(viewer, zoom),
    });
  }

  public viewAt100(viewer: ViewerId): void {
    this.setPane(viewer, {
      zoom: 1,
      zoomMode: '100%',
      displayScale: this.displayScaleFor(viewer, 1),
    });
  }

  /** Pans the loaded preview without starting a render. */
  public updatePan(viewer: ViewerId, pan: { x: number; y: number }): void {
    const current = this.state.panes[viewer].pan;
    if (current.x === pan.x && current.y === pan.y) return;
    this.setPane(viewer, { pan });
  }

  public setMaskDisplay(viewer: ViewerId, display: ViewerPaneState['maskDisplay']): void {
    if (this.state.panes[viewer].target?.dataType !== 'core.Mask') return;
    if (this.state.panes[viewer].maskDisplay === display) return;
    this.setPane(viewer, { maskDisplay: display });
    this.restartRequest(viewer);
  }
}
