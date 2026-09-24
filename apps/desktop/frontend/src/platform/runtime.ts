export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined'
    && '__TAURI_INTERNALS__' in window
    && import.meta.env.VITE_WDIO_BROWSER !== '1';
}
