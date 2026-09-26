export interface DockLayout {
  libraryWidth: number;
  libraryCollapsed: boolean;
  rightWidth: number;
  rightCollapsed: boolean;
  previewSize: number;
  previewCollapsed: boolean;
  sourceSize: number;
  sourceCollapsed: boolean;
}

export const DOCK_STORAGE_KEY = 'rawweave.dock';

export const DOCK_LIMITS = {
  libraryWidth: { min: 180, max: 460 },
  rightWidth: { min: 280, max: 760 },
  previewSize: { min: 140, max: 1400 },
  sourceSize: { min: 92, max: 460 },
} as const;

type NumericKey = keyof typeof DOCK_LIMITS;

export const DEFAULT_DOCK_LAYOUT: DockLayout = {
  libraryWidth: 232,
  libraryCollapsed: false,
  rightWidth: 360,
  rightCollapsed: false,
  previewSize: 420,
  previewCollapsed: false,
  sourceSize: 168,
  sourceCollapsed: false,
};

function clampNumber(key: NumericKey, value: unknown, fallback: number): number {
  const numeric = typeof value === 'number' && Number.isFinite(value) ? value : fallback;
  const { min, max } = DOCK_LIMITS[key];
  return Math.min(max, Math.max(min, Math.round(numeric)));
}

/** Coerces anything (parsed storage, stale shapes) into a usable layout. */
export function normalizeDockLayout(value: unknown): DockLayout {
  const source = value && typeof value === 'object' ? value as Record<string, unknown> : {};
  return {
    libraryWidth: clampNumber('libraryWidth', source.libraryWidth, DEFAULT_DOCK_LAYOUT.libraryWidth),
    libraryCollapsed: source.libraryCollapsed === true,
    rightWidth: clampNumber('rightWidth', source.rightWidth, DEFAULT_DOCK_LAYOUT.rightWidth),
    rightCollapsed: source.rightCollapsed === true,
    previewSize: clampNumber('previewSize', source.previewSize, DEFAULT_DOCK_LAYOUT.previewSize),
    previewCollapsed: source.previewCollapsed === true,
    sourceSize: clampNumber('sourceSize', source.sourceSize, DEFAULT_DOCK_LAYOUT.sourceSize),
    sourceCollapsed: source.sourceCollapsed === true,
  };
}

export function parseDockLayout(raw: string | null | undefined): DockLayout {
  if (!raw) return { ...DEFAULT_DOCK_LAYOUT };
  try {
    return normalizeDockLayout(JSON.parse(raw));
  } catch {
    return { ...DEFAULT_DOCK_LAYOUT };
  }
}

export function serializeDockLayout(layout: DockLayout): string {
  return JSON.stringify(layout);
}

/** Applies a resize along one axis, clamped to that track's limits. */
export function resizeDock(layout: DockLayout, key: NumericKey, delta: number): DockLayout {
  return { ...layout, [key]: clampNumber(key, layout[key] + delta, layout[key]) };
}
