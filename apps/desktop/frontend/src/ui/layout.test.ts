import { describe, expect, it } from 'vitest';
import {
  DEFAULT_DOCK_LAYOUT,
  DOCK_LIMITS,
  normalizeDockLayout,
  parseDockLayout,
  resizeDock,
  serializeDockLayout,
} from './layout';

describe('dock layout', () => {
  it('round-trips a layout through storage', () => {
    const layout = { ...DEFAULT_DOCK_LAYOUT, libraryWidth: 300, rightWidth: 480, previewCollapsed: true };
    expect(parseDockLayout(serializeDockLayout(layout))).toEqual(layout);
  });

  it('falls back to defaults for missing or malformed storage', () => {
    expect(parseDockLayout(null)).toEqual(DEFAULT_DOCK_LAYOUT);
    expect(parseDockLayout('{ not json')).toEqual(DEFAULT_DOCK_LAYOUT);
  });

  it('clamps out-of-range sizes and ignores junk values', () => {
    const layout = normalizeDockLayout({
      libraryWidth: 99999,
      rightWidth: -20,
      previewSize: 'wide',
      sourceSize: 100,
      previewCollapsed: 'yes',
    });
    expect(layout.libraryWidth).toBe(DOCK_LIMITS.libraryWidth.max);
    expect(layout.rightWidth).toBe(DOCK_LIMITS.rightWidth.min);
    expect(layout.previewSize).toBe(DEFAULT_DOCK_LAYOUT.previewSize);
    expect(layout.sourceSize).toBe(100);
    expect(layout.previewCollapsed).toBe(false);
  });

  it('adds a portrait height to older storage without changing landscape width', () => {
    const layout = parseDockLayout('{"rightWidth":480}');
    expect(layout.rightWidth).toBe(480);
    expect(layout.rightHeight).toBe(DEFAULT_DOCK_LAYOUT.rightHeight);
    const resized = resizeDock(layout, 'rightHeight', 40);
    expect(resized.rightHeight).toBe(layout.rightHeight + 40);
    expect(resized.rightWidth).toBe(480);
    expect(resizeDock(layout, 'rightHeight', -10000).rightHeight).toBe(DOCK_LIMITS.rightHeight.min);
  });

  it('resizes a track within its limits', () => {
    const grown = resizeDock(DEFAULT_DOCK_LAYOUT, 'rightWidth', 80);
    expect(grown.rightWidth).toBe(DEFAULT_DOCK_LAYOUT.rightWidth + 80);

    const clamped = resizeDock(DEFAULT_DOCK_LAYOUT, 'libraryWidth', -10000);
    expect(clamped.libraryWidth).toBe(DOCK_LIMITS.libraryWidth.min);
  });
});
