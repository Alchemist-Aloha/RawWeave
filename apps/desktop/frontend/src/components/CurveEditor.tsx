import { useEffect, useRef, useState } from 'react';
import type { ParameterValue } from '../editor/types';
import { curveAxes, type CurveAxes } from '../editor/curve-axes';
import { Icon } from '../ui/Icon';
import { CurvePreview, TransferPlot, curveDomain, curvePlot, type CurveDomain, type Point } from './CurvePreview';

// Short decimal strings that round-trip the backend's f32 values (e.g. 0.7).
export function serializeCurve(points: Point[]): string {
  const decimal = (n: number) => {
    for (let digits = 1; digits < 9; digits++) {
      const short = Number(n.toPrecision(digits));
      if (Math.fround(short) === n) return String(short);
    }
    return String(Number(n.toPrecision(9)));
  };
  return points.map((point) => point.map(decimal).join(',')).join(';');
}

export function moveCurvePoint(points: Point[], index: number, next: Point, domain: CurveDomain): Point[] {
  const result = points.map((point) => [...point] as Point);
  const current = result[index];
  if (!current || !next.every(Number.isFinite)) return result;
  let x = current[0];
  if (index > 0 && index < points.length - 1 && Math.fround(next[0]) !== current[0]) {
    const left = points[index - 1][0];
    const right = points[index + 1][0];
    const gap = Math.max((domain.xMax - domain.xMin) * 1e-6, Math.abs(left) * 2 ** -23, Math.abs(right) * 2 ** -23);
    const low = Math.fround(left + gap);
    const high = Math.fround(right - gap);
    if (low > left && high < right && low <= high) x = Math.fround(Math.max(low, Math.min(high, next[0])));
  }
  result[index] = [x, Math.fround(Math.max(domain.yMin, Math.min(domain.yMax, next[1])))];
  return result;
}

function coordinatePoints(typeId: string, points: Point[], index: number, axis: 0 | 1, text: string): Point[] | null {
  const n = Math.fround(Number(text));
  if (!text.trim() || !Number.isFinite(n) || !points[index]) return null;
  if (axis === 0 && (index === 0 || index === points.length - 1 || n <= points[index - 1][0] || n >= points[index + 1][0])) return null;
  const next = points.map((point, i) => i === index ? [axis === 0 ? n : point[0], axis === 1 ? n : point[1]] as Point : point);
  return curvePlot(typeId, serializeCurve(next)) ? next : null;
}

interface Gesture { points: Point[]; initial: string; original: string; domain: CurveDomain; index: number; pointerId?: number; anchor?: { position: Point; point: Point; client: Point }; dragged?: boolean }

export function CurveEditor({ typeId, value, committedValue, axes = curveAxes({ id: '', typeId }), onBegin, onDraft, onCommit, onCancel }: {
  typeId: string; value: ParameterValue; committedValue: ParameterValue; axes?: CurveAxes;
  onBegin: () => void; onDraft: (text: string) => void; onCommit: () => void; onCancel: () => void;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const gesture = useRef<Gesture | null>(null);
  const [selected, select] = useState(0);
  const [frozen, freeze] = useState<CurveDomain | undefined>();
  type Coordinate = { index: number; axis: 0 | 1; text: string };
  const [coordinate, setCoordinate] = useState<Coordinate | null>(null);
  const coordinateRef = useRef<Coordinate | null>(null);
  const draftCoordinate = (next: Coordinate | null) => { coordinateRef.current = next; setCoordinate(next); };
  const points = curvePlot(typeId, value);
  const activeIndex = Math.min(selected, (points?.length ?? 1) - 1);
  const release = (session: Gesture) => {
    if (session.pointerId !== undefined && svg.current?.hasPointerCapture(session.pointerId)) svg.current.releasePointerCapture(session.pointerId);
  };
  const cancel = () => {
    const session = gesture.current;
    if (!session) return;
    gesture.current = null;
    draftCoordinate(null);
    freeze(undefined);
    onCancel();
    release(session);
  };
  const finish = () => {
    const session = gesture.current;
    if (!session) return;
    gesture.current = null;
    draftCoordinate(null);
    freeze(undefined);
    const edited = serializeCurve(session.points) !== session.original;
    if (edited || session.initial !== String(committedValue)) {
      if (!edited) onDraft(session.initial);
      onCommit();
    } else onCancel();
    release(session);
  };
  // Undo/reset or an external graph edit must never be overwritten by a stale drag.
  useEffect(() => {
    if (gesture.current && String(committedValue) !== gesture.current.initial) cancel();
  }, [committedValue]);
  const begin = (index: number, pointerId?: number) => {
    if (!points || gesture.current) return null;
    const session: Gesture = { points, initial: String(value), original: serializeCurve(points), domain: curveDomain(points), index, pointerId };
    gesture.current = session;
    select(index);
    freeze(session.domain);
    onBegin();
    return session;
  };
  const update = (session: Gesture, next: Point[]) => {
    const text = serializeCurve(next);
    if (!curvePlot(typeId, text)) return false;
    session.points = next;
    onDraft(text);
    return true;
  };
  const position = (clientX: number, clientY: number, domain: CurveDomain): Point | null => {
    const matrix = svg.current?.getScreenCTM()?.inverse();
    if (!matrix) return null;
    const x = matrix.a * clientX + matrix.c * clientY + matrix.e;
    const y = matrix.b * clientX + matrix.d * clientY + matrix.f;
    if (![x, y].every(Number.isFinite)) return null;
    return [domain.xMin + (x - 8) / 184 * (domain.xMax - domain.xMin), domain.yMin + (112 - y) / 104 * (domain.yMax - domain.yMin)];
  };
  const drag = (session: Gesture, clientX: number, clientY: number) => {
    const next = position(clientX, clientY, session.domain);
    if (!next || !session.anchor) return;
    const { position: start, point, client } = session.anchor;
    // A selection click must not become an edit when focus/layout shifts the SVG.
    if (Math.hypot(clientX - client[0], clientY - client[1]) < 2) {
      if (session.dragged) update(session, session.points.map((p, i) => i === session.index ? point : p));
      return;
    }
    session.dragged = true;
    update(session, moveCurvePoint(session.points, session.index, [point[0] + (next[0] - start[0]), point[1] + (next[1] - start[1])], session.domain));
  };
  const add = () => {
    if (!points || points.length >= 4096 || gesture.current) return;
    let index = 0;
    for (let i = 1; i < points.length - 1; i++) if (points[i + 1][0] - points[i][0] > points[index + 1][0] - points[index][0]) index = i;
    const a = points[index];
    const b = points[index + 1];
    const point: Point = [Math.fround((a[0] + b[0]) / 2), Math.fround((a[1] + b[1]) / 2)];
    if (point[0] <= a[0] || point[0] >= b[0]) return;
    const session = begin(index + 1);
    if (!session) return;
    update(session, [...points.slice(0, index + 1), point, ...points.slice(index + 1)]);
    finish();
    svg.current?.focus();
  };
  const remove = (index: number) => {
    if (!points || index <= 0 || index >= points.length - 1) return;
    const session = gesture.current ?? begin(index);
    if (session) { update(session, session.points.filter((_, i) => i !== index)); select(index - 1); finish(); }
  };
  const finishCoordinate = () => {
    const draft = coordinateRef.current;
    if (!draft || !gesture.current) return true;
    if (!coordinatePoints(typeId, gesture.current.points, draft.index, draft.axis, draft.text)) { cancel(); return false; }
    finish();
    return true;
  };
  if (!points) return <CurvePreview typeId={typeId} value={value} />;
  const chosen = points[activeIndex];
  const invalidCoordinate = coordinate !== null && !coordinatePoints(typeId, points, coordinate.index, coordinate.axis, coordinate.text);
  return <div className="curve-editor nodrag nopan nowheel">
    <TransferPlot points={points} domain={frozen} axes={axes} svgProps={{
      ref: svg, role: 'group', tabIndex: 0, 'aria-label': `Curve editor: ${axes.x} horizontally; ${axes.y} vertically`,
      'aria-description': 'Click or drag to add between endpoints. Drag handles to move; Alt-click deletes an interior point. Arrow keys nudge; Shift is coarse, Alt is fine. Delete removes an interior point. Escape cancels.',
      onPointerDown: (event) => {
        if (gesture.current?.pointerId === undefined) {
          if (!finishCoordinate()) return;
          finish();
        }
        if (event.button !== 0 || gesture.current) return;
        const domain = curveDomain(points);
        const next = position(event.clientX, event.clientY, domain);
        const matrix = svg.current?.getScreenCTM();
        if (!next || !matrix) return;
        // Hit-test in screen pixels, independently of React Flow zoom/letterboxing.
        let index = -1;
        let nearest = event.pointerType === 'touch' ? 22 : 12;
        points.forEach(([a, b], i) => {
          const x = 8 + (a - domain.xMin) / (domain.xMax - domain.xMin) * 184;
          const y = 112 - (b - domain.yMin) / (domain.yMax - domain.yMin) * 104;
          const distance = Math.hypot(matrix.a * x + matrix.c * y + matrix.e - event.clientX, matrix.b * x + matrix.d * y + matrix.f - event.clientY);
          if (distance <= nearest) { nearest = distance; index = i; }
        });
        if (event.altKey) {
          event.preventDefault(); event.stopPropagation();
          if (index >= 0) remove(index);
          return;
        }
        const point: Point = next.map(Math.fround) as Point;
        const inserting = index < 0;
        if (inserting) {
          if (points.length >= 4096 || point[0] <= points[0][0] || point[0] >= points[points.length - 1][0] || point[1] < domain.yMin || point[1] > domain.yMax) return;
          index = points.findIndex(([x]) => x === point[0]);
          if (index < 0) index = points.findIndex(([x]) => x > point[0]);
        }
        event.preventDefault();
        event.stopPropagation();
        const session = begin(index, event.pointerId);
        if (!session) return;
        try { event.currentTarget.setPointerCapture(event.pointerId); }
        catch { cancel(); return; }
        event.currentTarget.focus();
        if (inserting && !points.some(([x]) => x === point[0]) && !update(session, [...points.slice(0, index), point, ...points.slice(index)])) { cancel(); return; }
        session.anchor = { position: next, point: session.points[index], client: [event.clientX, event.clientY] };
      },
      onPointerMove: (event) => {
        const session = gesture.current;
        if (!session || session.pointerId !== event.pointerId) return;
        drag(session, event.clientX, event.clientY);
      },
      onPointerUp: (event) => {
        const session = gesture.current;
        if (session?.pointerId === event.pointerId) { drag(session, event.clientX, event.clientY); finish(); }
      },
      onPointerCancel: (event) => { if (gesture.current?.pointerId === event.pointerId) cancel(); },
      onLostPointerCapture: (event) => { if (gesture.current?.pointerId === event.pointerId) cancel(); },
      onKeyDown: (event) => {
        if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); cancel(); return; }
        if (gesture.current?.pointerId !== undefined) return;
        const targetIndex = (event.target as Element).getAttribute('data-point-index');
        const index = targetIndex === null ? activeIndex : Number(targetIndex);
        if (!Number.isInteger(index) || index < 0 || index >= points.length) return;
        if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); event.stopPropagation(); select(index); return; }
        if (['Delete', 'Backspace'].includes(event.key)) {
          event.preventDefault(); event.stopPropagation();
          select(index);
          if (event.repeat || index === 0 || index === points.length - 1) return;
          remove(index);
          return;
        }
        if (!['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(event.key)) return;
        event.preventDefault(); event.stopPropagation();
        const session = gesture.current ?? begin(index);
        if (!session) return;
        const [a, b] = session.points[session.index];
        const factor = event.altKey ? 0.001 : event.shiftKey ? 0.1 : 0.01;
        const dx = (session.domain.xMax - session.domain.xMin) * factor;
        const dy = (session.domain.yMax - session.domain.yMin) * factor;
        update(session, moveCurvePoint(session.points, session.index,
          [a + (event.key === 'ArrowLeft' ? -dx : event.key === 'ArrowRight' ? dx : 0), b + (event.key === 'ArrowDown' ? -dy : event.key === 'ArrowUp' ? dy : 0)], session.domain));
      },
      onKeyUp: (event) => { if (event.key.startsWith('Arrow') && gesture.current?.pointerId === undefined) finish(); },
      onBlur: (event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) { if (gesture.current?.pointerId === undefined) finish(); else cancel(); } },
    }} renderPoints={(x, y) => points.map(([a, b], i) => <circle key={i}
      className={`curve-editor__point${i === activeIndex ? ' is-selected' : ''}`} cx={x(a)} cy={y(b)} r="3.5"
      data-point-index={i} tabIndex={0} role="button" aria-pressed={i === activeIndex}
      aria-label={`Curve point ${i + 1}: input ${a}, output ${b}${i === 0 || i === points.length - 1 ? ', endpoint input locked' : ''}`}
      onFocus={() => select(i)} />)}>
      <span className="curve-editor__readout">Point {activeIndex + 1}: input {Number(chosen[0].toPrecision(6))}, output {Number(chosen[1].toPrecision(6))}</span>
      {typeId.startsWith('pro.') && <small>Point mapping only; exposure and effect strength apply separately.</small>}
    </TransferPlot>
    <div className="curve-editor__coordinates">
      <label>Point<span className="curve-editor__picker"><select aria-label="Selected curve point" value={activeIndex} onChange={(event) => select(Number(event.target.value))}>
        {points.map((point, i) => <option key={i} value={i}>{i + 1}: {serializeCurve([point])}{i === 0 || i === points.length - 1 ? ' (endpoint)' : ''}</option>)}
      </select><Icon name="chevronDown" /></span></label>
      {(['Input', 'Output'] as const).map((name, i) => {
        const axis = i as 0 | 1;
        return <label key={name}>{name}<input type="number" step="0.01" aria-label={`Point ${name.toLowerCase()}`}
          disabled={axis === 0 && (activeIndex === 0 || activeIndex === points.length - 1)}
          aria-invalid={coordinate?.axis === axis && Boolean(invalidCoordinate)}
          value={coordinate?.index === activeIndex && coordinate.axis === axis ? coordinate.text : serializeCurve([chosen]).split(',')[axis]}
          onFocus={() => { if (begin(activeIndex)) freeze(undefined); }}
          onChange={(event) => {
            const session = gesture.current ?? begin(activeIndex);
            if (!session) return;
            freeze(undefined);
            const draft = { index: session.index, axis, text: event.target.value };
            draftCoordinate(draft);
            const next = coordinatePoints(typeId, session.points, draft.index, axis, draft.text);
            if (next) update(session, next);
          }}
          onKeyDown={(event) => {
            if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); cancel(); }
            else if (event.key === 'Enter') { event.preventDefault(); event.stopPropagation(); finishCoordinate(); }
          }}
          onKeyUp={(event) => { if (event.key.startsWith('Arrow')) finishCoordinate(); }}
          onPointerUp={finishCoordinate} onMouseUp={finishCoordinate} onPointerCancel={cancel} onBlur={() => { if (gesture.current?.pointerId !== undefined) return; if (coordinateRef.current) finishCoordinate(); else finish(); }}
        /></label>;
      })}
    </div>
    {invalidCoordinate && <small className="curve-preview__invalid" role="status">Enter a finite value within the supported number range. {coordinate?.axis === 0 && <>Input must stay strictly between neighboring points{typeId.startsWith('pro.') ? ' and within 0–1' : ''}. </>}Enter/blur discards invalid edits; Escape cancels.</small>}
    <div className="curve-editor__actions">
      <button type="button" aria-label="Add curve point" disabled={points.length >= 4096} onMouseDown={(event) => event.preventDefault()} onClick={() => { if (finishCoordinate()) { finish(); add(); } }}>Add point</button>
      <button type="button" aria-label="Delete curve point" disabled={activeIndex === 0 || activeIndex === points.length - 1} onMouseDown={(event) => event.preventDefault()} onClick={() => { if (finishCoordinate()) { finish(); remove(activeIndex); } }}>Delete point</button>
    </div>
    <small>Click-drag to add; drag handles to move. Alt-click or Delete removes interior points. Arrows nudge (Shift coarse, Alt fine). Release applies; Escape cancels. Coordinates apply on Enter/blur and can extend output beyond the plot. Endpoint inputs are locked.</small>
  </div>;
}
