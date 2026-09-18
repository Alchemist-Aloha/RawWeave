import type { PreviewRequest, PreviewTarget, ViewerId, ViewerPaneState, ViewerState } from './types';
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
    zoomMode: 'fit',
    pan: { x: 0, y: 0 },
  };
}

function clampZoom(value: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, value));
}

export class ViewerController {
  public state: ViewerState = {
    currentRevision: 0,
    layout: 'side-by-side',
    panes: { A: paneState(), B: paneState() },
  };

  private readonly listeners = new Set<(state: ViewerState) => void>();
  private readonly active = new Map<ViewerId, ActiveRequest>();
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

  public setRevision(revision: number): void {
    if (revision === this.state.currentRevision) return;
    this.state = { ...this.state, currentRevision: revision };
    for (const [viewer, active] of this.active) {
      this.active.delete(viewer);
      void this.transport.cancelPreview(active.request.requestId);
    }
    for (const viewer of ['A', 'B'] as ViewerId[]) {
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

  public setTarget(viewer: ViewerId, target: PreviewTarget | null): void {
    const current = this.active.get(viewer);
    if (current) {
      this.active.delete(viewer);
      void this.transport.cancelPreview(current.request.requestId);
    }
    this.setPane(viewer, {
      target,
      imageUrl: null,
      width: null,
      height: null,
      status: target ? 'loading' : 'idle',
      progress: 0,
      error: null,
      requestId: null,
    });
    if (target) this.startRequest(viewer, target);
  }

  private startRequest(viewer: ViewerId, target: PreviewTarget): void {
    const request: PreviewRequest = {
      requestId: this.requestId(viewer),
      revision: this.state.currentRevision,
      nodeId: target.nodeId,
      outputPort: target.outputPort,
      quality: 'preview',
    };
    this.active.set(viewer, { request });
    this.setPane(viewer, { status: 'loading', requestId: request.requestId, progress: 0, error: null });
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
        this.setPane(viewer, {
          imageUrl: result.url,
          width: result.width,
          height: result.height,
          status: 'ready',
          progress: 1,
          error: null,
          requestId: null,
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
    this.setPane(viewer, { zoom: clampZoom(zoom), zoomMode: 'custom' });
  }

  public adjustZoom(viewer: ViewerId, delta: number): void {
    this.setZoom(viewer, this.state.panes[viewer].zoom * (delta > 0 ? 1.1 : 1 / 1.1));
  }

  public fitToWindow(viewer: ViewerId): void {
    this.setPane(viewer, { zoom: 1, zoomMode: 'fit', pan: { x: 0, y: 0 } });
  }

  public viewAt100(viewer: ViewerId): void {
    this.setPane(viewer, { zoom: 1, zoomMode: '100%' });
  }

  public setPan(viewer: ViewerId, pan: { x: number; y: number }): void {
    this.setPane(viewer, { pan });
  }

  public panBy(viewer: ViewerId, delta: { x: number; y: number }): void {
    const current = this.state.panes[viewer].pan;
    this.setPan(viewer, { x: current.x + delta.x, y: current.y + delta.y });
  }
}
