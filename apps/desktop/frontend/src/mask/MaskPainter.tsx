import { useRef, useState, type PointerEvent, type RefObject } from 'react';
import type { EditorNode, ParameterValue } from '../editor/types';
import {
  PaintedMaskController,
  imagePointFromPointer,
  paintedMaskStateFromParameters,
  type PaintMode,
  type PaintPoint,
} from './painting';
import type { ImageRegion, PointerRect } from './painting';

interface MaskPainterProps {
  node: EditorNode;
  imageRef: RefObject<HTMLImageElement | null>;
  imageRegion: ImageRegion | null;
  imageOrigin: PaintPoint;
  onChange: (parameterId: string, value: ParameterValue) => void;
}

function inside(rect: PointerRect, event: { clientX: number; clientY: number }): boolean {
  return event.clientX >= rect.left
    && event.clientX <= rect.left + rect.width
    && event.clientY >= rect.top
    && event.clientY <= rect.top + rect.height;
}

export function MaskPainter({ node, imageRef, imageRegion, imageOrigin, onChange }: MaskPainterProps) {
  const [, render] = useState(0);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const controllerRef = useRef<PaintedMaskController | null>(null);
  const nodeIdRef = useRef<string | null>(null);
  if (!controllerRef.current || nodeIdRef.current !== node.id) {
    controllerRef.current = new PaintedMaskController(
      paintedMaskStateFromParameters(node.parameters),
      (parameterId, value) => {
        onChangeRef.current(parameterId, value);
        render((revision) => revision + 1);
      },
    );
    nodeIdRef.current = node.id;
  }
  const controller = controllerRef.current;
  const painting = useRef(false);

  const pointFromEvent = (event: { clientX: number; clientY: number }): PaintPoint | null => {
    const image = imageRef.current;
    if (!image || !imageRegion) return null;
    const rect = image.getBoundingClientRect();
    if (!inside(rect, event)) return null;
    return imagePointFromPointer(
      event,
      rect,
      { x: imageOrigin.x, y: imageOrigin.y, width: imageRegion.width, height: imageRegion.height },
    );
  };

  const updateBrush = (patch: { size?: number; hardness?: number; opacity?: number; mode?: PaintMode }) => {
    controller.setBrush(patch);
    controller.flush();
  };

  const start = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    const point = pointFromEvent(event);
    if (!point) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    painting.current = true;
    controller.beginStroke(point);
  };

  const move = (event: PointerEvent<HTMLDivElement>) => {
    if (!painting.current) return;
    event.preventDefault();
    event.stopPropagation();
    const point = pointFromEvent(event);
    if (point) controller.appendPoint(point);
  };

  const finish = (event: PointerEvent<HTMLDivElement>) => {
    if (!painting.current) return;
    event.preventDefault();
    event.stopPropagation();
    painting.current = false;
    controller.endStroke();
    controller.flush();
  };

  const cancel = (event: PointerEvent<HTMLDivElement>) => {
    if (!painting.current) return;
    event.preventDefault();
    event.stopPropagation();
    painting.current = false;
    controller.cancelStroke();
    controller.flush();
  };

  const brush = controller.brush;
  const hasImage = Boolean(imageRegion);
  return (
    <>
      <div
        aria-disabled={!hasImage}
        aria-label="Paint mask"
        className="mask-painter__surface"
        onPointerCancel={cancel}
        onPointerDown={start}
        onPointerMove={move}
        onPointerUp={finish}
        style={{ pointerEvents: hasImage ? 'auto' : 'none' }}
      />
      <div aria-label="Mask painting controls" className="mask-painter__controls">
        <label>
          <span>Size</span>
          <input
            aria-label="Brush size"
            max="1000"
            min="1"
            onChange={(event) => updateBrush({ size: Number(event.target.value) })}
            type="range"
            value={brush.size}
          />
          <output>{Math.round(brush.size)}px</output>
        </label>
        <label>
          <span>Hardness</span>
          <input
            aria-label="Brush hardness"
            max="1"
            min="0"
            onChange={(event) => updateBrush({ hardness: Number(event.target.value) })}
            step="0.01"
            type="range"
            value={brush.hardness}
          />
          <output>{Math.round(brush.hardness * 100)}%</output>
        </label>
        <label>
          <span>Opacity</span>
          <input
            aria-label="Brush opacity"
            max="1"
            min="0"
            onChange={(event) => updateBrush({ opacity: Number(event.target.value) })}
            step="0.01"
            type="range"
            value={brush.opacity}
          />
          <output>{Math.round(brush.opacity * 100)}%</output>
        </label>
        <div className="mask-painter__modes" role="group" aria-label="Paint mode">
          <button
            aria-label="Add mask"
            aria-pressed={brush.mode === 'add'}
            className={brush.mode === 'add' ? 'is-active' : ''}
            onClick={() => updateBrush({ mode: 'add' })}
            type="button"
          >
            Add
          </button>
          <button
            aria-label="Subtract mask"
            aria-pressed={brush.mode === 'subtract'}
            className={brush.mode === 'subtract' ? 'is-active' : ''}
            onClick={() => updateBrush({ mode: 'subtract' })}
            type="button"
          >
            Subtract
          </button>
        </div>
        <div className="mask-painter__history">
          <button
            aria-label="Undo mask stroke"
            disabled={!controller.canUndo}
            onClick={() => {
              controller.undo();
              controller.flush();
              render((revision) => revision + 1);
            }}
            type="button"
          >
            Undo
          </button>
          <button
            aria-label="Redo mask stroke"
            disabled={!controller.canRedo}
            onClick={() => {
              controller.redo();
              controller.flush();
              render((revision) => revision + 1);
            }}
            type="button"
          >
            Redo
          </button>
        </div>
      </div>
    </>
  );
}
