import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { PreviewRequest, PreviewResult } from '../viewer/types';
import type { PreviewTransport } from '../viewer/transport';
import { isTauriRuntime } from './runtime';

interface PreviewReadyEvent extends PreviewResult {}
interface PreviewProgressEvent {
  requestId: string;
  revision: number;
  progress: number;
}
interface PreviewErrorEvent {
  requestId: string;
  revision: number;
  message: string;
}
interface PreviewCancelledEvent {
  requestId: string;
  revision: number;
}

function errorMessage(error: unknown): Error {
  if (error instanceof Error) return error;
  return new Error(typeof error === 'string' ? error : JSON.stringify(error));
}

export function createTauriPreviewTransport(): PreviewTransport {
  return {
    requestPreview(request, onProgress) {
      return new Promise<PreviewResult>((resolve, reject) => {
        let unlisten: UnlistenFn[] = [];
        let settled = false;

        const cleanup = () => {
          for (const remove of unlisten) remove();
          unlisten = [];
        };
        const finish = (callback: () => void) => {
          if (settled) return;
          settled = true;
          cleanup();
          callback();
        };
        const matches = (candidate: { requestId: string; revision: number }) =>
          candidate.requestId === request.requestId && candidate.revision === request.revision;

        void Promise.all([
          listen<PreviewReadyEvent>('preview-ready', (event) => {
            if (matches(event.payload)) finish(() => resolve(event.payload));
          }),
          listen<PreviewProgressEvent>('preview-progress', (event) => {
            if (matches(event.payload)) onProgress(event.payload.progress);
          }),
          listen<PreviewErrorEvent>('preview-error', (event) => {
            if (matches(event.payload)) finish(() => reject(new Error(event.payload.message)));
          }),
          listen<PreviewCancelledEvent>('preview-cancelled', (event) => {
            if (matches(event.payload)) finish(() => reject(new Error('preview cancelled')));
          }),
        ])
          .then((listeners) => {
            if (settled) {
              for (const remove of listeners) remove();
              return;
            }
            unlisten = listeners;
            return invoke<PreviewResult>('request_preview', { request })
              .then((result) => finish(() => resolve(result)));
          })
          .catch((error: unknown) => finish(() => reject(errorMessage(error))));
      });
    },
    async cancelPreview(requestId) {
      await invoke('cancel_preview', { requestId });
    },
    async releasePreview(url) {
      await invoke('release_preview', { url });
    },
  };
}

class BrowserPreviewTransport implements PreviewTransport {
  requestPreview(_request: PreviewRequest, onProgress: (progress: number) => void): Promise<PreviewResult> {
    onProgress(1);
    return Promise.reject(new Error('preview source image unavailable in browser mode'));
  }

  async cancelPreview(_requestId: string): Promise<void> {
    // Browser mode has no backend render job to cancel.
  }

  async releasePreview(_url: string): Promise<void> {
    // Browser mode has no backend preview store to release.
  }
}

export function createPreviewTransport(): PreviewTransport {
  if (isTauriRuntime()) {
    return createTauriPreviewTransport();
  }
  return new BrowserPreviewTransport();
}
