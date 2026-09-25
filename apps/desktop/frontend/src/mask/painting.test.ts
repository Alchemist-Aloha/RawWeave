import { describe, expect, it } from 'vitest';
import {
  PaintedMaskController,
  PaintedMaskHistory,
  imagePointFromPointer,
  parseSerializedPoints,
  paintedMaskStateFromParameters,
  serializePaintedMaskState,
  serializePaintedMaskParameters,
  serializePoints,
} from './painting';

describe('painted mask model', () => {
  it('maps a zoomed and panned pointer to global image coordinates', () => {
    expect(imagePointFromPointer(
      { clientX: 250, clientY: 170 },
      { left: 100, top: 50, width: 400, height: 200 },
      { x: 20, y: 30, width: 800, height: 400 },
      { x: 10, y: 5 },
    )).toEqual({ x: 330, y: 275 });
  });

  it('round trips the compact point parameter format used by Painted Mask', () => {
    const points = [{ x: 1.25, y: 2.5 }, { x: 8, y: 9 }];
    expect(parseSerializedPoints(serializePoints(points))).toEqual(points);
  });

  it('supports stroke undo and redo without losing brush parameters', () => {
    const history = new PaintedMaskHistory({ size: 42, hardness: 0.6, opacity: 0.8, mode: 'subtract' });
    history.applyStroke([{ x: 1, y: 2 }, { x: 3, y: 4 }]);
    history.setBrush({ size: 18 });
    history.applyStroke([{ x: 5, y: 6 }]);

    expect(history.snapshot().strokes).toHaveLength(2);
    expect(history.undo()).toBe(true);
    expect(history.snapshot().strokes).toHaveLength(1);
    expect(history.redo()).toBe(true);
    expect(history.snapshot().strokes[1]).toMatchObject({
      points: [{ x: 5, y: 6 }],
      size: 18,
      hardness: 0.6,
      opacity: 0.8,
      mode: 'subtract',
    });
  });

  it('serializes the active stroke and brush through Painted Mask parameters', () => {
    const history = new PaintedMaskHistory({ size: 12, hardness: 0.75, opacity: 0.5, mode: 'subtract' });
    history.applyStroke([{ x: 4, y: 5 }, { x: 6, y: 7 }]);

    expect(serializePaintedMaskParameters(history.snapshot())).toEqual({
      size: 12,
      hardness: 0.75,
      opacity: 0.5,
      mode: 'subtract',
      points: '4,5;6,7',
    });
  });

  it('batches pointer updates and keeps undo and redo serialized', () => {
    const scheduled: Array<() => void> = [];
    const writes: Array<[string, string | number]> = [];
    const controller = new PaintedMaskController(
      { size: 20, hardness: 1, opacity: 1, mode: 'add' },
      (parameter, value) => writes.push([parameter, value]),
      (callback) => {
        scheduled.push(callback);
        return scheduled.length;
      },
    );

    controller.beginStroke({ x: 1, y: 2 });
    controller.appendPoint({ x: 3, y: 4 });
    controller.appendPoint({ x: 5, y: 6 });
    expect(writes).toEqual([]);
    expect(scheduled).toHaveLength(1);

    scheduled.shift()?.();
    expect(writes).toEqual([
      ['size', 20],
      ['hardness', 1],
      ['opacity', 1],
      ['mode', 'add'],
      ['points', '1,2;3,4;5,6'],
    ]);

    controller.endStroke();
    controller.undo();
    scheduled.shift()?.();
    expect(writes.at(-1)).toEqual(['points', '']);
    controller.redo();
    scheduled.shift()?.();
    expect(writes.at(-1)).toEqual(['points', '1,2;3,4;5,6']);
  });

  it('serializes the complete history for local project state', () => {
    const history = new PaintedMaskHistory({ size: 12, hardness: 1, opacity: 1, mode: 'add' });
    history.applyStroke([{ x: 4, y: 5 }]);
    const restored = JSON.parse(serializePaintedMaskState(history.snapshot())) as ReturnType<PaintedMaskHistory['snapshot']>;
    expect(restored).toEqual(history.snapshot());
  });

  it('serializes every stroke so a painted mask survives multi-stroke editing', () => {
    const history = new PaintedMaskHistory({ size: 12, hardness: 1, opacity: 1, mode: 'add' });
    history.applyStroke([{ x: 4, y: 5 }, { x: 6, y: 7 }]);
    history.applyStroke([{ x: 8, y: 9 }]);

    const parameters = serializePaintedMaskParameters(history.snapshot());
    expect(parameters.points).toBe('4,5;6,7|8,9');

    const restored = paintedMaskStateFromParameters(parameters);
    expect(restored.strokes.map((stroke) => stroke.points)).toEqual([
      [{ x: 4, y: 5 }, { x: 6, y: 7 }],
      [{ x: 8, y: 9 }],
    ]);
  });

  it('keeps committed strokes visible while a new stroke is painted', () => {
    const scheduled: Array<() => void> = [];
    const writes: Array<[string, string | number]> = [];
    const controller = new PaintedMaskController(
      { size: 20, hardness: 1, opacity: 1, mode: 'add' },
      (parameter, value) => writes.push([parameter, value]),
      (callback) => {
        scheduled.push(callback);
        return scheduled.length;
      },
    );

    controller.beginStroke({ x: 1, y: 1 });
    controller.endStroke();
    scheduled.shift()?.();
    expect(writes.at(-1)).toEqual(['points', '1,1']);

    controller.beginStroke({ x: 2, y: 2 });
    controller.appendPoint({ x: 3, y: 3 });
    scheduled.shift()?.();
    expect(writes.at(-1)).toEqual(['points', '1,1|2,2;3,3']);
  });
});

