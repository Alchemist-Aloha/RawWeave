export type ViewerId = 'A' | 'B';
export type ViewerLayout = 'split' | 'side-by-side';
export type ViewerZoomMode = 'fit' | '100%' | 'custom';
export type PreviewQuality = 'draft' | 'preview' | 'final';

export interface PreviewTarget {
  nodeId: string;
  nodeName: string;
  outputPort: string;
  outputName: string;
}

export interface PreviewRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface PreviewTile {
  x: number;
  y: number;
}

export interface ImageDimensions {
  width: number;
  height: number;
}

export interface PreviewRequest {
  requestId: string;
  revision: number;
  nodeId: string;
  outputPort: string;
  quality: PreviewQuality;
  region: PreviewRegion;
  tile: PreviewTile;
  mip: number;
}

export interface PreviewResult {
  requestId: string;
  revision: number;
  url: string;
  width: number;
  height: number;
  fullWidth: number;
  fullHeight: number;
  mimeType: string;
}

export interface ViewerPaneState {
  target: PreviewTarget | null;
  imageUrl: string | null;
  width: number | null;
  height: number | null;
  status: 'idle' | 'loading' | 'ready' | 'error' | 'cancelled';
  progress: number;
  error: string | null;
  requestId: string | null;
  zoom: number;
  displayScale: number;
  zoomMode: ViewerZoomMode;
  pan: { x: number; y: number };
}

export interface ViewerState {
  currentRevision: number;
  layout: ViewerLayout;
  panes: Record<ViewerId, ViewerPaneState>;
}
