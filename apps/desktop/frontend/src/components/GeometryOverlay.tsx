import { useRef, useState, type RefObject } from 'react';
import type { ParameterValue } from '../editor/types';
import type { ImageDimensions } from '../viewer/types';
import { drawnParameters } from '../viewer/region';

type Point = { x: number; y: number };
export function GeometryOverlay({ typeId, parameters = {}, imageRef, size, origin, onChange, onCancel }: {
  typeId: string;
  parameters?: Record<string, ParameterValue>;
  imageRef: RefObject<HTMLImageElement | null>;
  size: ImageDimensions;
  origin: Point;
  onChange: (values: Record<string, ParameterValue>) => void;
  onCancel: () => void;
}) {
  const start = useRef<Point | null>(null);
  const [drawing, setDrawing] = useState<{ a: Point; b: Point } | null>(null);
  const point = (event: { clientX: number; clientY: number }) => {
    const rect = imageRef.current?.getBoundingClientRect();
    if (!rect || rect.width <= 0 || rect.height <= 0) return null;
    return {
      x: Math.max(0, Math.min(size.width, (event.clientX - rect.left) / rect.width * size.width)),
      y: Math.max(0, Math.min(size.height, (event.clientY - rect.top) / rect.height * size.height)),
    };
  };
  const number = (id: string, fallback = 0) => Number(parameters[id] ?? fallback);
  const existing = typeId === 'core.crop' ? {
    a: { x: number('x'), y: number('y') },
    b: { x: number('x') + number('width', 1), y: number('y') + number('height', 1) },
  } : typeId === 'core.mask-linear-gradient' ? {
    a: { x: number('start_x') - origin.x, y: number('start_y') - origin.y },
    b: { x: number('end_x', 1) - origin.x, y: number('end_y') - origin.y },
  } : {
    a: { x: number('center_x') - origin.x, y: number('center_y') - origin.y },
    b: { x: number('center_x') - origin.x + number('radius', 1), y: number('center_y') - origin.y },
  };
  const guide = drawing ?? existing;
  const rect = imageRef.current?.getBoundingClientRect();
  const stage = imageRef.current?.parentElement?.getBoundingClientRect();
  const hint = typeId === 'core.crop' ? 'Drag a crop rectangle' : typeId === 'core.mask-linear-gradient' ? 'Drag from dark to light' : 'Drag from the center to the radius';
  return <>
    <div
      aria-label="Draw image region"
      className="geometry-overlay"
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        const bounds = imageRef.current?.getBoundingClientRect();
        if (!bounds || event.clientX < bounds.left || event.clientX > bounds.left + bounds.width || event.clientY < bounds.top || event.clientY > bounds.top + bounds.height) return;
        event.preventDefault(); event.stopPropagation();
        start.current = point(event);
        if (start.current) setDrawing({ a: start.current, b: start.current });
        try {
          event.currentTarget.setPointerCapture?.(event.pointerId);
        } catch {
          // The gesture still works inside the surface when capture is unavailable.
        }
      }}
      onPointerMove={(event) => {
        if (!start.current) return;
        event.stopPropagation();
        const b = point(event);
        if (b) setDrawing({ a: start.current, b });
      }}
      onPointerUp={(event) => {
        if (!start.current) return;
        event.stopPropagation();
        const b = point(event);
        const values = b && drawnParameters(typeId, start.current, b, size, origin);
        start.current = null; setDrawing(null);
        if (values) onChange(values);
      }}
      onPointerCancel={(event) => { event.stopPropagation(); start.current = null; setDrawing(null); }}
    >
      {rect && stage && <svg className="geometry-overlay__guide" style={{ left: rect.left - stage.left, top: rect.top - stage.top, width: rect.width, height: rect.height }} viewBox={`0 0 ${size.width} ${size.height}`}>
        {typeId === 'core.crop' ? <rect x={Math.min(guide.a.x, guide.b.x)} y={Math.min(guide.a.y, guide.b.y)} width={Math.abs(guide.b.x - guide.a.x)} height={Math.abs(guide.b.y - guide.a.y)} />
          : typeId === 'core.mask-radial-gradient' ? <circle cx={guide.a.x} cy={guide.a.y} r={Math.hypot(guide.b.x - guide.a.x, guide.b.y - guide.a.y)} />
          : <line x1={guide.a.x} y1={guide.a.y} x2={guide.b.x} y2={guide.b.y} />}
      </svg>}
    </div>
    <div className="geometry-overlay__controls"><span>{hint} · {size.width} × {size.height} px{typeId === 'core.crop' && ` · Crop ${Math.round(Math.abs(guide.b.x - guide.a.x))} × ${Math.round(Math.abs(guide.b.y - guide.a.y))} px`}</span><button type="button" onClick={onCancel}>Done drawing</button></div>
  </>;
}
