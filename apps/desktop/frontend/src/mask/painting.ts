export type PaintMode = 'add' | 'subtract';

export interface PaintPoint {
  x: number;
  y: number;
}

export interface PaintBrush {
  size: number;
  hardness: number;
  opacity: number;
  mode: PaintMode;
}

export interface PaintStroke extends PaintBrush {
  points: PaintPoint[];
}

export interface PaintedMaskState {
  brush: PaintBrush;
  strokes: PaintStroke[];
  redo: PaintStroke[];
}

export type PaintParameterValue = number | string;
export type PaintParameterWriter = (parameterId: string, value: PaintParameterValue) => void;
export type PaintScheduler = (callback: () => void) => unknown;

export interface PointerRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface ImageRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function clampPaintBrush(brush: PaintBrush): PaintBrush {
  return {
    size: Number.isFinite(brush.size) ? Math.max(0.01, brush.size) : 1,
    hardness: Number.isFinite(brush.hardness) ? Math.min(1, Math.max(0, brush.hardness)) : 1,
    opacity: Number.isFinite(brush.opacity) ? Math.min(1, Math.max(0, brush.opacity)) : 1,
    mode: brush.mode === 'subtract' ? 'subtract' : 'add',
  };
}

export function serializePoints(points: PaintPoint[]): string {
  return points.map((point) => `${point.x},${point.y}`).join(';');
}

/**
 * Strokes are separated by `|` and points within a stroke by `;`.
 *
 * A serialized value without `|` is a single stroke, which keeps every mask
 * painted before multi-stroke support parseable.
 */
export const STROKE_SEPARATOR = '|';

export function serializeStrokes(strokes: PaintPoint[][]): string {
  return strokes
    .filter((points) => points.length > 0)
    .map((points) => serializePoints(points))
    .join(STROKE_SEPARATOR);
}

function parseStroke(serialized: string): PaintPoint[] {
  return serialized
    .split(';')
    .map((pair) => pair.trim())
    .filter(Boolean)
    .flatMap((pair) => {
      const [x, y] = pair.split(',').map((value) => Number(value.trim()));
      return Number.isFinite(x) && Number.isFinite(y) ? [{ x, y }] : [];
    });
}

export function parseSerializedStrokes(serialized: string): PaintPoint[][] {
  return serialized
    .split(STROKE_SEPARATOR)
    .map(parseStroke)
    .filter((points) => points.length > 0);
}

export function parseSerializedPoints(serialized: string): PaintPoint[] {
  return parseSerializedStrokes(serialized).flat();
}

export function serializePaintedMaskState(state: PaintedMaskState): string {
  return JSON.stringify(state);
}

export function imagePointFromPointer(
  pointer: { clientX: number; clientY: number },
  imageRect: PointerRect,
  region: ImageRegion,
  pan: { x: number; y: number } = { x: 0, y: 0 },
): PaintPoint | null {
  if (imageRect.width <= 0 || imageRect.height <= 0 || region.width <= 0 || region.height <= 0) {
    return null;
  }
  const x = ((pointer.clientX - imageRect.left) / imageRect.width) * region.width;
  const y = ((pointer.clientY - imageRect.top) / imageRect.height) * region.height;
  return {
    x: region.x + Math.min(region.width, Math.max(0, x)) + pan.x,
    y: region.y + Math.min(region.height, Math.max(0, y)) + pan.y,
  };
}

export class PaintedMaskHistory {
  private brush: PaintBrush;
  private strokes: PaintStroke[] = [];
  private redoStrokes: PaintStroke[] = [];

  public constructor(brush: PaintBrush, state?: PaintedMaskState) {
    this.brush = clampPaintBrush({ ...brush });
    if (state) {
      this.strokes = state.strokes.map((stroke) => ({
        ...stroke,
        points: stroke.points.map((point) => ({ ...point })),
      }));
      this.redoStrokes = state.redo.map((stroke) => ({
        ...stroke,
        points: stroke.points.map((point) => ({ ...point })),
      }));
    }
  }

  public setBrush(patch: Partial<PaintBrush>): void {
    this.brush = clampPaintBrush({ ...this.brush, ...patch });
  }

  public applyStroke(points: PaintPoint[]): boolean {
    if (points.length === 0) return false;
    const stroke: PaintStroke = {
      ...this.brush,
      points: points.map((point) => ({ x: point.x, y: point.y })),
    };
    this.strokes.push(stroke);
    this.redoStrokes = [];
    return true;
  }

  public undo(): boolean {
    const stroke = this.strokes.pop();
    if (!stroke) return false;
    this.redoStrokes.push(stroke);
    return true;
  }

  public redo(): boolean {
    const stroke = this.redoStrokes.pop();
    if (!stroke) return false;
    this.strokes.push(stroke);
    return true;
  }

  public snapshot(): PaintedMaskState {
    return {
      brush: { ...this.brush },
      strokes: this.strokes.map((stroke) => ({ ...stroke, points: stroke.points.map((point) => ({ ...point })) })),
      redo: this.redoStrokes.map((stroke) => ({ ...stroke, points: stroke.points.map((point) => ({ ...point })) })),
    };
  }
}

export function paintedMaskStateFromParameters(parameters: Record<string, unknown>): PaintedMaskState {
  const mode = parameters.mode === 'subtract' ? 'subtract' : 'add';
  const brush = clampPaintBrush({
    size: typeof parameters.size === 'number' ? parameters.size : 1,
    hardness: typeof parameters.hardness === 'number' ? parameters.hardness : 1,
    opacity: typeof parameters.opacity === 'number' ? parameters.opacity : 1,
    mode,
  });
  // The node carries a single brush, so every restored stroke takes the current
  // brush settings. Geometry is preserved for the whole painting history.
  const strokes = parseSerializedStrokes(typeof parameters.points === 'string' ? parameters.points : '');
  return {
    brush,
    strokes: strokes.map((points) => ({ ...brush, points })),
    redo: [],
  };
}

export function serializePaintedMaskParameters(state: PaintedMaskState): Record<string, PaintParameterValue> {
  return {
    size: state.brush.size,
    hardness: state.brush.hardness,
    opacity: state.brush.opacity,
    mode: state.brush.mode,
    points: serializeStrokes(state.strokes.map((stroke) => stroke.points)),
  };
}

function defaultPaintScheduler(callback: () => void): unknown {
  if (typeof requestAnimationFrame === 'function') return requestAnimationFrame(callback);
  return setTimeout(callback, 0);
}

export class PaintedMaskController {
  private readonly history: PaintedMaskHistory;
  private readonly write: PaintParameterWriter;
  private readonly schedule: PaintScheduler;
  private activePoints: PaintPoint[] = [];
  private pending: Record<string, PaintParameterValue> | null = null;
  private scheduled = false;

  public constructor(
    initial: PaintBrush | PaintedMaskState,
    write: PaintParameterWriter,
    schedule: PaintScheduler = defaultPaintScheduler,
  ) {
    const hasState = 'strokes' in initial;
    const state = hasState ? initial : undefined;
    const brush: PaintBrush = hasState ? initial.brush : initial;
    this.history = new PaintedMaskHistory(brush, state);
    this.write = write;
    this.schedule = schedule;
  }

  public get brush(): PaintBrush {
    return this.history.snapshot().brush;
  }

  public get canUndo(): boolean {
    return this.history.snapshot().strokes.length > 0;
  }

  public get canRedo(): boolean {
    return this.history.snapshot().redo.length > 0;
  }

  public snapshot(): PaintedMaskState {
    return this.history.snapshot();
  }

  public setBrush(patch: Partial<PaintBrush>): void {
    this.history.setBrush(patch);
    this.queueParameters();
  }

  public beginStroke(point: PaintPoint): void {
    this.activePoints = [{ ...point }];
    this.queueParameters();
  }

  public appendPoint(point: PaintPoint): void {
    const previous = this.activePoints.at(-1);
    if (previous?.x === point.x && previous.y === point.y) return;
    this.activePoints.push({ ...point });
    this.queueParameters();
  }

  public endStroke(): boolean {
    if (this.activePoints.length === 0) return false;
    this.history.applyStroke(this.activePoints);
    this.activePoints = [];
    this.queueParameters();
    return true;
  }

  public cancelStroke(): void {
    this.activePoints = [];
    this.queueParameters();
  }

  public undo(): boolean {
    this.activePoints = [];
    const changed = this.history.undo();
    if (changed) this.queueParameters();
    return changed;
  }

  public redo(): boolean {
    this.activePoints = [];
    const changed = this.history.redo();
    if (changed) this.queueParameters();
    return changed;
  }

  public flush(): void {
    if (!this.pending) return;
    const pending = this.pending;
    this.pending = null;
    this.scheduled = false;
    for (const [parameterId, value] of Object.entries(pending)) this.write(parameterId, value);
  }

  private queueParameters(): void {
    const state = this.history.snapshot();
    const parameters = serializePaintedMaskParameters(state);
    // The in-progress stroke is appended so the committed strokes stay visible
    // in the rendered preview while a new one is being painted.
    if (this.activePoints.length > 0) {
      parameters.points = serializeStrokes([
        ...state.strokes.map((stroke) => stroke.points),
        this.activePoints,
      ]);
    }
    this.pending = { ...(this.pending ?? {}), ...parameters };
    if (this.scheduled) return;
    this.scheduled = true;
    this.schedule(() => this.flush());
  }
}
