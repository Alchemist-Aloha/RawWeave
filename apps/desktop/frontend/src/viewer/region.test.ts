import { describe, expect, it } from 'vitest';
import { drawnParameters } from './region';

describe('image geometry gestures', () => {
  it('normalizes a reversed crop and clamps it to actual input dimensions', () => {
    expect(drawnParameters('core.crop', { x: 700, y: 480 }, { x: -20, y: 120 }, { width: 640, height: 480 }, { x: 50, y: 30 }))
      .toEqual({ x: 0, y: 120, width: 640, height: 360 });
  });
  it('keeps crop coordinates local but gradient coordinates global', () => {
    const size = { width: 640, height: 480 };
    const origin = { x: 50, y: 30 };
    expect(drawnParameters('core.crop', { x: 10, y: 20 }, { x: 110, y: 220 }, size, origin))
      .toEqual({ x: 10, y: 20, width: 100, height: 200 });
    expect(drawnParameters('core.mask-linear-gradient', { x: 10, y: 20 }, { x: 110, y: 220 }, size, origin))
      .toEqual({ start_x: 60, start_y: 50, end_x: 160, end_y: 250 });
  });
  it('rejects a click without a region and draws radial center/radius', () => {
    const size = { width: 100, height: 100 };
    expect(drawnParameters('core.crop', { x: 5, y: 5 }, { x: 5, y: 5 }, size)).toBeNull();
    expect(drawnParameters('core.mask-radial-gradient', { x: 10, y: 10 }, { x: 40, y: 50 }, size))
      .toEqual({ center_x: 10, center_y: 10, radius: 50 });
  });
});
