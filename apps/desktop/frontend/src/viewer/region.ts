import type { ParameterValue } from '../editor/types';
import type { ImageDimensions } from './types';

type Point = { x: number; y: number };
export const DRAWABLE_NODES = new Set(['core.crop', 'core.mask-linear-gradient', 'core.mask-radial-gradient']);

/** Image gestures use local full-resolution pixels; masks use global origins. */
export function drawnParameters(type: string, start: Point, end: Point, size: ImageDimensions, origin: Point = { x: 0, y: 0 }): Record<string, ParameterValue> | null {
  const clamp = (point: Point) => ({
    x: Math.round(Math.max(0, Math.min(size.width, point.x))),
    y: Math.round(Math.max(0, Math.min(size.height, point.y))),
  });
  const a = clamp(start);
  const b = clamp(end);
  if (a.x === b.x && a.y === b.y) return null;
  if (type === 'core.crop') {
    if (a.x === b.x || a.y === b.y) return null;
    return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), width: Math.abs(b.x - a.x), height: Math.abs(b.y - a.y) };
  }
  if (type === 'core.mask-linear-gradient') {
    return { start_x: a.x + origin.x, start_y: a.y + origin.y, end_x: b.x + origin.x, end_y: b.y + origin.y };
  }
  if (type === 'core.mask-radial-gradient') {
    return { center_x: a.x + origin.x, center_y: a.y + origin.y, radius: Math.hypot(b.x - a.x, b.y - a.y) };
  }
  return null;
}
