import type { PreviewRequest, PreviewResult } from './types';

export interface PreviewTransport {
  requestPreview(request: PreviewRequest, onProgress: (progress: number) => void): Promise<PreviewResult>;
  cancelPreview(requestId: string): Promise<void>;
  releasePreview(url: string): Promise<void>;
}
