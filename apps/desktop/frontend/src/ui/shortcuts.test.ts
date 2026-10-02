import { describe, expect, it } from 'vitest';
import { shortcutAction } from './shortcuts';

describe('keyboard shortcuts', () => {
  it('maps graph editing and file actions without depending on the platform modifier', () => {
    expect(shortcutAction(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true }))).toBe('focus-node-search');
    expect(shortcutAction(new KeyboardEvent('keydown', { key: 'o', ctrlKey: true }))).toBe('open-workflow');
    expect(shortcutAction(new KeyboardEvent('keydown', { key: 'o', ctrlKey: true, shiftKey: true }))).toBe('open-image');
    expect(shortcutAction(new KeyboardEvent('keydown', { key: 's', metaKey: true }))).toBe('save-workflow');
    expect(shortcutAction(new KeyboardEvent('keydown', { key: 'z', metaKey: true }))).toBe('undo');
    expect(shortcutAction(new KeyboardEvent('keydown', { key: 'z', metaKey: true, shiftKey: true }))).toBe('redo');
  });

  it('leaves undo and redo to focused native editing controls', () => {
    for (const tag of ['input', 'textarea', 'select', 'div']) {
      const field = document.createElement(tag);
      if (tag === 'div') field.contentEditable = 'true';
      const target = tag === 'div' ? field.appendChild(document.createElement('span')) : field;
      if (tag === 'div') field.setAttribute('contenteditable', 'true');
      for (const ctrlKey of [true, false]) {
        const event = { key: 'z', ctrlKey, metaKey: !ctrlKey, shiftKey: false, altKey: false, target };
        expect(shortcutAction(event)).toBeNull();
        expect(shortcutAction({ ...event, shiftKey: true })).toBeNull();
        expect(shortcutAction({ ...event, key: 'y' })).toBeNull();
        expect(shortcutAction({ ...event, key: 's' })).toBe('save-workflow');
        expect(shortcutAction({ ...event, key: 'k' })).toBe('focus-node-search');
      }
    }
  });

  it('keeps destructive and help shortcuts explicit', () => {
    expect(shortcutAction(new KeyboardEvent('keydown', { key: 'Backspace' }))).toBe('delete-selection');
    expect(shortcutAction(new KeyboardEvent('keydown', { key: '?' }))).toBe('toggle-shortcuts');
    expect(shortcutAction(new KeyboardEvent('keydown', { key: 'x', ctrlKey: true }))).toBeNull();
  });
});
