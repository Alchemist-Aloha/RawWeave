export type ShortcutAction =
  | 'focus-node-search'
  | 'open-image'
  | 'open-workflow'
  | 'save-workflow'
  | 'undo'
  | 'redo'
  | 'delete-selection'
  | 'toggle-shortcuts';

/** Resolve the small set of global editor shortcuts shared by desktop and WebView. */
export function shortcutAction(event: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey'> & { target?: EventTarget | null }): ShortcutAction | null {
  const key = event.key.toLowerCase();
  const modifier = event.ctrlKey || event.metaKey;
  if (event.altKey) return null;

  if (modifier) {
    // Native text history must not undo unrelated graph edits behind a field.
    const target = event.target;
    if ((key === 'z' || key === 'y') && target instanceof HTMLElement
      && target.closest('input, textarea, select, [contenteditable="true"], [contenteditable=""]')) return null;
    if (key === 'k' && !event.shiftKey) return 'focus-node-search';
    if (key === 'o') return event.shiftKey ? 'open-image' : 'open-workflow';
    if (key === 's' && !event.shiftKey) return 'save-workflow';
    if (key === 'z') return event.shiftKey ? 'redo' : 'undo';
    if (key === 'y' && !event.shiftKey) return 'redo';
    return null;
  }

  if (key === 'backspace' || key === 'delete') return 'delete-selection';
  if (key === '?') return 'toggle-shortcuts';
  return null;
}
