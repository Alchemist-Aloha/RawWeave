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
    fullWidth: null,
    fullHeight: null,
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
    imageOrigin: { x: 0, y: 0 },
  };
}

function clampZoom(value: number, floor = MIN_ZOOM): number {
  return Math.min(MAX_ZOOM, Math.max(floor, value));
}

const TILE_SIZE = 32;
const DEFAULT_DIMENSIONS: ImageDimensions = { width: 1, height: 1 };
interface RequestPlan {
  region: PreviewRegion;
  zoom: number;
  mip: number;
}

/**
 * The scale that fits the whole frame inside the viewport.
 *
 * Fit is deliberately not clamped to the interactive zoom range: a large image
 * in a small panel needs a scale below `MIN_ZOOM`, and clamping it there is what
 * let the "fit" image overflow the panel. Display scale stays in full-resolution
 * coordinates even when the preview bitmap uses a mip.
 */
function fitZoom(dimensions: ImageDimensions, viewport: ImageDimensions): number {
  const fit = Math.min(viewport.width / dimensions.width, viewport.height / dimensions.height);
  return Number.isFinite(fit) && fit > 0 ? fit : 1;
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
  private readonly viewports = new Map<ViewerId, ImageDimensions & { pixelRatio: number }>();
  private readonly loadedMips = new Map<ViewerId, number>();
  private readonly imageDimensions = new Map<ViewerId, ImageDimensions>();
  private sourceDimensions: ImageDimensions = DEFAULT_DIMENSIONS;
  private sequence = 0;

  public constructor(private readonly transport: PreviewTransport) {}

  public subscribe(listener: (state: ViewerState) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Current evaluated full-size metadata, independent of zoom or bitmap mip. */
  public getTargetDimensions(target: PreviewTarget): ImageDimensions | null {
    for (const viewer of ['A', 'B'] as ViewerId[]) {
      const pane = this.state.panes[viewer];
      if (pane.status !== 'ready' || pane.target?.nodeId !== target.nodeId || pane.target.outputPort !== target.outputPort) continue;
      const size = this.imageDimensions.get(viewer);
      if (size && Number.isSafeInteger(size.width) && Number.isSafeInteger(size.height) && size.width > 0 && size.height > 0) return { ...size };
    }
    return null;
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
    this.loadedMips.delete(viewer);
    this.setPane(viewer, {
      imageUrl: null,
      width: null,
      height: null,
      fullWidth: null,
      fullHeight: null,
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

  /** Reuse a loaded or in-flight frame when its detail level is already correct. */
  private updateDetail(viewer: ViewerId): void {
    if (!this.state.panes[viewer].target) return;
    const plan = this.requestPlan(viewer);
    const active = this.active.get(viewer)?.request;
    if (active ? active.mip === plan.mip && sameRegion(active.region, plan.region) : this.loadedMips.get(viewer) === plan.mip) return;
    this.restartRequest(viewer);
  }

  /** Request full-frame coordinates with enough bitmap pixels for the display. */
  private requestPlan(viewer: ViewerId): RequestPlan {
    const pane = this.state.panes[viewer];
    const dimensions = this.imageDimensions.get(viewer) ?? this.sourceDimensions;
    const viewport = this.viewports.get(viewer);
    const zoom = pane.zoomMode === 'fit' ? fitZoom(dimensions, viewport ?? dimensions) : pane.zoom;
    return {
      region: { x: 0, y: 0, width: dimensions.width, height: dimensions.height },
      zoom,
      // A missing/hidden stage must not turn the initial request into a 1x1 fit.
      mip: viewport ? Math.max(0, Math.min(6, Math.floor(Math.log2(1 / (zoom * viewport.pixelRatio))))) : 0,
    };
  }

  public setSourceDimensions(dimensions: ImageDimensions): void {
    this.sourceDimensions = {
      width: Math.max(1, Math.floor(dimensions.width)),
      height: Math.max(1, Math.floor(dimensions.height)),
    };
    this.imageDimensions.clear();
    this.loadedMips.clear();
    for (const viewer of ['A', 'B'] as ViewerId[]) {
      if (this.state.panes[viewer].target) this.restartRequest(viewer);
    }
  }

  public setViewport(viewer: ViewerId, viewport: ImageDimensions, pixelRatio = 1): void {
    if (!Number.isFinite(pixelRatio) || pixelRatio <= 0 ||
      !Number.isFinite(viewport.width) || viewport.width < 1 ||
      !Number.isFinite(viewport.height) || viewport.height < 1) return;
    const next = {
      width: Math.floor(viewport.width),
      height: Math.floor(viewport.height),
      pixelRatio,
    };
    const previous = this.viewports.get(viewer);
    if (previous?.width === next.width && previous.height === next.height && previous.pixelRatio === pixelRatio) return;
    this.viewports.set(viewer, next);
    const pane = this.state.panes[viewer];
    if (pane.zoomMode === 'fit') {
      const zoom = this.requestPlan(viewer).zoom;
      this.setPane(viewer, { zoom, displayScale: zoom });
    }
    this.updateDetail(viewer);
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
      this.loadedMips.delete(viewer);
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
          fullWidth: null,
          fullHeight: null,
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
    this.loadedMips.delete(viewer);
    this.setPane(viewer, {
      target,
      imageUrl: null,
      width: null,
      height: null,
      fullWidth: null,
      fullHeight: null,
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

  private startRequest(viewer: ViewerId, target: PreviewTarget, corrected = false): void {
    const plan = this.requestPlan(viewer);
    const request: PreviewRequest = {
      requestId: this.requestId(viewer),
      revision: this.state.currentRevision,
      nodeId: target.nodeId,
      outputPort: target.outputPort,
      quality: 'preview',
      region: plan.region,
      tile: { x: Math.floor(plan.region.x / TILE_SIZE), y: Math.floor(plan.region.y / TILE_SIZE) },
      mip: plan.mip,
      maskDisplay: this.state.panes[viewer].maskDisplay,
    };
    this.active.set(viewer, { request });
    this.setPane(viewer, {
      zoom: plan.zoom,
      displayScale: plan.zoom,
      imageRegion: request.region,
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
        if (!Number.isSafeInteger(result.fullWidth) || result.fullWidth <= 0 ||
          !Number.isSafeInteger(result.fullHeight) || result.fullHeight <= 0) {
          this.releasePreview(result.url);
          throw new Error('preview returned invalid full-size dimensions');
        }
        this.imageDimensions.set(viewer, { width: result.fullWidth, height: result.fullHeight });
        const nextPlan = this.requestPlan(viewer);
        if (!sameRegion(request.region, nextPlan.region) || request.mip !== nextPlan.mip) {
          this.releasePreview(result.url);
          // One metadata correction is enough for a stable evaluated output.
          if (corrected) throw new Error('preview output dimensions changed during rendering');
          this.active.delete(viewer);
          this.startRequest(viewer, target, true);
          return;
        }
        this.active.delete(viewer);
        this.loadedMips.set(viewer, request.mip);
        this.replacePanePreview(viewer, {
          imageUrl: result.url,
          width: result.width,
          height: result.height,
          fullWidth: result.fullWidth,
          fullHeight: result.fullHeight,
          zoom: nextPlan.zoom,
          displayScale: nextPlan.zoom,
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
    if (this.state.panes[viewer].requestId !== active.request.requestId) return;
    this.releasePanePreview(viewer);
    this.loadedMips.delete(viewer);
    this.setPane(viewer, {
      status: 'cancelled',
      requestId: null,
      progress: 0,
      imageUrl: null,
      width: null,
      height: null,
      fullWidth: null,
      fullHeight: null,
      error: null,
    });
  }

  /**
   * The smallest zoom the user may dial in.
   *
   * Fit is deliberately allowed below `MIN_ZOOM` (a large frame in a small panel
   * needs it), so an absolute floor above the current scale turned "zoom out"
   * into a zoom in - and then, because the clamped value equalled the current
   * zoom, froze the control for good. The floor follows the fit scale instead;
   * `MIN_ZOOM` stays the floor for frames that fit comfortably.
   */
  private zoomFloor(viewer: ViewerId): number {
    const dimensions = this.imageDimensions.get(viewer) ?? this.sourceDimensions;
    const viewport = this.viewports.get(viewer) ?? dimensions;
    return Math.min(MIN_ZOOM, fitZoom(dimensions, viewport) / 4);
  }

  /** Transform immediately; render only when the required mip changes. */
  public setZoom(viewer: ViewerId, zoom: number): void {
    if (!Number.isFinite(zoom)) return;
    const nextZoom = clampZoom(zoom, this.zoomFloor(viewer));
    if (nextZoom === this.state.panes[viewer].zoom && this.state.panes[viewer].zoomMode === 'custom') return;
    this.setPane(viewer, {
      zoom: nextZoom,
      zoomMode: 'custom',
      displayScale: nextZoom,
    });
    this.updateDetail(viewer);
  }

  public adjustZoom(viewer: ViewerId, delta: number): void {
    this.setZoom(viewer, this.state.panes[viewer].zoom * (delta > 0 ? 1.1 : 1 / 1.1));
  }

  public fitToWindow(viewer: ViewerId): void {
    const dimensions = this.imageDimensions.get(viewer) ?? this.sourceDimensions;
    const viewport = this.viewports.get(viewer) ?? dimensions;
    const zoom = fitZoom(dimensions, viewport);
    this.setPane(viewer, {
      zoom,
      zoomMode: 'fit',
      pan: { x: 0, y: 0 },
      displayScale: zoom,
    });
    this.updateDetail(viewer);
  }

  public viewAt100(viewer: ViewerId): void {
    this.setPane(viewer, {
      zoom: 1,
      zoomMode: '100%',
      displayScale: 1,
    });
    this.updateDetail(viewer);
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
    this.loadedMips.delete(viewer);
    this.restartRequest(viewer);
  }
}
