export type ViewerId = 'A' | 'B';
export type ViewerLayout = 'split' | 'side-by-side';
export type ViewerComparison = 'side-by-side' | 'wipe' | 'blink' | 'difference';
export type ViewerZoomMode = 'fit' | '100%' | 'custom';
export type PreviewQuality = 'draft' | 'preview' | 'final';
export type MaskDisplay = 'grayscale' | 'overlay';

export interface PreviewTarget {
  nodeId: string;
  nodeName: string;
  outputPort: string;
  outputName: string;
  dataType?: string;
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
  maskDisplay?: MaskDisplay;
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
  originX?: number;
  originY?: number;
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
  maskDisplay: MaskDisplay;
  imageRegion: PreviewRegion | null;
  imageMip: number;
  imageOrigin: { x: number; y: number };
}

export interface ViewerState {
  currentRevision: number;
  layout: ViewerLayout;
  comparison: ViewerComparison;
  clippingOverlay: boolean;
  panes: Record<ViewerId, ViewerPaneState>;
}
