import type { BatchSessionReference } from '../batch/types';
import type { ParameterValue, WorkflowSummary } from '../editor/types';
import type { ImageSetCollection } from '../imageset/model';

export type BrowserEntryKind = 'file' | 'directory';
export type BrowserFlag = 'none' | 'pick' | 'reject';
export type BrowserSortBy = 'name' | 'modified' | 'size' | 'rating';
export type BrowserSortDirection = 'asc' | 'desc';
export type BrowserRatingFilter = 'any' | 'rated' | 'unrated';
export type BrowserFlagFilter = 'any' | BrowserFlag;
export type BrowserLayout = 'grid' | 'list';

export interface BrowserSort {
  by: BrowserSortBy;
  direction: BrowserSortDirection;
}

export interface BrowserFilter {
  query: string;
  rating: BrowserRatingFilter;
  flag: BrowserFlagFilter;
}

export interface ImageMetadata {
  width: number;
  height: number;
  camera: string | null;
  lens: string | null;
  iso: number | null;
  aperture: number | null;
  shutter: number | null;
  focalLength: number | null;
  captureTime: string | null;
  orientation: string | null;
  exif: Record<string, string>;
}

export interface BrowserEntry {
  path: string;
  name: string;
  kind: BrowserEntryKind;
  extension: string;
  size: number;
  modifiedTime: string | null;
  rating: number | null;
  flag: BrowserFlag;
  metadata: ImageMetadata | null;
  thumbnail: string | null;
}

export interface DirectoryPage {
  path: string;
  entries: BrowserEntry[];
  offset: number;
  nextOffset: number | null;
  hasMore: boolean;
}

export interface DirectoryBreadcrumb {
  name: string;
  path: string;
}

export interface FileOperationResult {
  path: string;
  previousPath?: string;
}

export interface WorkflowBinding {
  id: string;
  version: string;
  hash: string;
}

export type ProcessingStatus = 'pending' | 'processing' | 'complete' | 'failed';
export type OutputStatus = 'not-started' | 'writing' | 'written' | 'failed';

export interface QueueItem {
  id: string;
  path: string;
  name: string;
  source: BrowserEntry;
  rating: number | null;
  flag: BrowserFlag;
  order: number;
  workflowBinding: WorkflowBinding | null;
  overrides: Record<string, ParameterValue>;
  processingStatus: ProcessingStatus;
  outputStatus: OutputStatus;
  errors: string[];
  warnings: string[];
  testSet: boolean;
}

export interface QueueStatusPatch {
  processingStatus?: ProcessingStatus;
  outputStatus?: OutputStatus;
  errors?: string[];
  warnings?: string[];
}

export interface BrowserViewSettings {
  sort: BrowserSort;
  filter: BrowserFilter;
  thumbnailSize: 'small' | 'medium' | 'large';
  layout?: BrowserLayout;
}

export interface WorkingQueueSession {
  items: QueueItem[];
  currentPath: string | null;
  selectedPaths: string[];
}

export interface BrowserSession {
  version: 1;
  browser: {
    currentFolder: string;
    view: BrowserViewSettings;
    selectedPaths: string[];
  };
  queue: WorkingQueueSession;
  testSet: {
    currentPath: string | null;
  };
  workflow: {
    selected: WorkflowBinding | null;
    unsavedWorkingCopy: string | null;
  };
  viewer: {
    targets: Record<'A' | 'B', { nodeId: string; outputPort: string } | null>;
  };
  batch: BatchSessionReference;
  imageSets: ImageSetCollection[];
  activeImageSetId: string | null;
  panelLayout: string;
}
