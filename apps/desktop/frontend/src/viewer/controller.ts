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
export const PAN_RENDER_DEBOUNCE_MS = 80;

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

function clampInteger(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, Math.floor(value)));
}

function visibleRegion(
  dimensions: ImageDimensions,
  viewport: ImageDimensions,
  zoom: number,
  pan: { x: number; y: number },
): PreviewRegion {
  const width = clampInteger(Math.ceil(viewport.width / zoom), 1, dimensions.width);
  const height = clampInteger(Math.ceil(viewport.height / zoom), 1, dimensions.height);
  const centerX = dimensions.width / 2 - pan.x / zoom;
  const centerY = dimensions.height / 2 - pan.y / zoom;
  const x = clampInteger(Math.floor(centerX - width / 2), 0, dimensions.width - width);
  const y = clampInteger(Math.floor(centerY - height / 2), 0, dimensions.height - height);
  return { x, y, width, height };
}

function fitZoom(dimensions: ImageDimensions, viewport: ImageDimensions): number {
  return clampZoom(Math.min(viewport.width / dimensions.width, viewport.height / dimensions.height));
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
  private readonly active = new Map<ViewerId, ActiveRequest>();
  private readonly viewports = new Map<ViewerId, ImageDimensions>();
  private readonly imageDimensions = new Map<ViewerId, ImageDimensions>();
  private readonly panTimers = new Map<ViewerId, ReturnType<typeof setTimeout>>();
  private readonly dirtyPans = new Set<ViewerId>();
  private sourceDimensions: ImageDimensions = DEFAULT_DIMENSIONS;
  private sequence = 0;

  public constructor(private readonly transport: PreviewTransport) {}

  public subscribe(listener: (state: ViewerState) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private publish(): void {
    for (const listener of this.listeners) listener(this.state);
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
    if (url) void this.transport.releasePreview(url).catch(() => undefined);
  }

  private restartRequest(viewer: ViewerId): void {
    this.cancelPanRender(viewer);
    const target = this.state.panes[viewer].target;
    if (!target) return;
    const current = this.active.get(viewer);
    if (current) {
      this.active.delete(viewer);
      void this.transport.cancelPreview(current.request.requestId);
    }
    this.releasePanePreview(viewer);
    this.setPane(viewer, {
      imageUrl: null,
      width: null,
      height: null,
    });
    this.startRequest(viewer, target);
  }

  private requestPlan(viewer: ViewerId): RequestPlan {
    const pane = this.state.panes[viewer];
    const dimensions = this.imageDimensions.get(viewer) ?? this.sourceDimensions;
    const viewport = this.viewports.get(viewer) ?? DEFAULT_VIEWPORT;
    const zoom = pane.zoomMode === 'fit' ? fitZoom(dimensions, viewport) : pane.zoom;
    const mip = mipForZoom(zoom);
    const region =
      pane.zoomMode === 'fit'
        ? { x: 0, y: 0, width: dimensions.width, height: dimensions.height }
        : visibleRegion(dimensions, viewport, zoom, pane.pan);
    return { region, mip, zoom };
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
    if (this.state.panes[viewer].target) this.restartRequest(viewer);
  }

  public setRevision(revision: number): void {
    if (revision === this.state.currentRevision) return;
    this.state = { ...this.state, currentRevision: revision };
    for (const [viewer, active] of this.active) {
      this.active.delete(viewer);
      void this.transport.cancelPreview(active.request.requestId);
    }
    for (const viewer of ['A', 'B'] as ViewerId[]) {
      this.cancelPanRender(viewer);
      this.releasePanePreview(viewer);
      this.imageDimensions.delete(viewer);
      const target = this.state.panes[viewer].target;
      this.setPane(viewer, {
        status: target ? 'loading' : 'idle',
        requestId: null,
        progress: 0,
        imageUrl: null,
        width: null,
        height: null,
        error: null,
      });
      if (target) this.startRequest(viewer, target);
    }
    this.publish();
  }

  public setLayout(layout: ViewerState['layout']): void {
    if (layout === this.state.layout) return;
    this.state = { ...this.state, layout };
    this.publish();
  }

  public setComparison(comparison: ViewerComparison): void {
    if (comparison === this.state.comparison) return;
    this.state = {
      ...this.state,
      comparison,
      layout: comparison === 'side-by-side' ? this.state.layout : 'side-by-side',
    };
    this.publish();
  }

  public setClippingOverlay(enabled: boolean): void {
    if (enabled === this.state.clippingOverlay) return;
    this.state = { ...this.state, clippingOverlay: enabled };
    this.publish();
  }

  public setTarget(viewer: ViewerId, target: PreviewTarget | null): void {
    this.cancelPanRender(viewer);
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
          void this.transport.releasePreview(result.url).catch(() => undefined);
          this.setPane(viewer, { imageUrl: null, width: null, height: null });
          this.startRequest(viewer, target);
          return;
        }
        this.setPane(viewer, {
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
    this.cancelPanRender(viewer);
    const active = this.active.get(viewer);
    if (!active) return;
    this.active.delete(viewer);
    await this.transport.cancelPreview(active.request.requestId);
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

  public setZoom(viewer: ViewerId, zoom: number): void {
    const nextZoom = clampZoom(zoom);
    if (nextZoom === this.state.panes[viewer].zoom && this.state.panes[viewer].zoomMode === 'custom') {
      return;
    }
    this.setPane(viewer, { zoom: nextZoom, zoomMode: 'custom' });
    this.restartRequest(viewer);
  }

  public adjustZoom(viewer: ViewerId, delta: number): void {
    this.setZoom(viewer, this.state.panes[viewer].zoom * (delta > 0 ? 1.1 : 1 / 1.1));
  }

  public fitToWindow(viewer: ViewerId): void {
    this.setPane(viewer, { zoom: 1, zoomMode: 'fit', pan: { x: 0, y: 0 } });
    this.restartRequest(viewer);
  }

  public viewAt100(viewer: ViewerId): void {
    this.setPane(viewer, { zoom: 1, zoomMode: '100%' });
    this.restartRequest(viewer);
  }

  private cancelPanRender(viewer: ViewerId): void {
    const timer = this.panTimers.get(viewer);
    if (timer !== undefined) {
      clearTimeout(timer);
      this.panTimers.delete(viewer);
    }
    this.dirtyPans.delete(viewer);
  }

  private schedulePanRender(viewer: ViewerId): void {
    const timer = this.panTimers.get(viewer);
    if (timer !== undefined) clearTimeout(timer);
    this.panTimers.set(viewer, setTimeout(() => {
      this.panTimers.delete(viewer);
      if (!this.dirtyPans.delete(viewer)) return;
      this.restartRequest(viewer);
    }, PAN_RENDER_DEBOUNCE_MS));
  }

  /** Start a gesture whose intermediate positions should stay client-side. */
  public beginPan(viewer: ViewerId): void {
    const timer = this.panTimers.get(viewer);
    if (timer !== undefined) {
      clearTimeout(timer);
      this.panTimers.delete(viewer);
    }
  }

  /** Update the visual pan without restarting the preview render. */
  public updatePan(viewer: ViewerId, pan: { x: number; y: number }): void {
    const current = this.state.panes[viewer].pan;
    if (current.x === pan.x && current.y === pan.y) return;
    this.setPane(viewer, { pan });
    this.dirtyPans.add(viewer);
    this.schedulePanRender(viewer);
  }

  /** Commit the final gesture position immediately. */
  public endPan(viewer: ViewerId): void {
    const timer = this.panTimers.get(viewer);
    if (timer !== undefined) {
      clearTimeout(timer);
      this.panTimers.delete(viewer);
    }
    if (!this.dirtyPans.delete(viewer)) return;
    this.restartRequest(viewer);
  }

  public setPan(viewer: ViewerId, pan: { x: number; y: number }): void {
    this.cancelPanRender(viewer);
    this.setPane(viewer, { pan });
    this.restartRequest(viewer);
  }

  public panBy(viewer: ViewerId, delta: { x: number; y: number }): void {
    const current = this.state.panes[viewer].pan;
    this.setPan(viewer, { x: current.x + delta.x, y: current.y + delta.y });
  }

  public setMaskDisplay(viewer: ViewerId, display: ViewerPaneState['maskDisplay']): void {
    if (this.state.panes[viewer].target?.dataType !== 'core.Mask') return;
    if (this.state.panes[viewer].maskDisplay === display) return;
    this.setPane(viewer, { maskDisplay: display });
    this.restartRequest(viewer);
  }
}
