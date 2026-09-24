import { useRef, type PointerEvent as ReactPointerEvent, type KeyboardEvent as ReactKeyboardEvent } from 'react';

export interface SplitterProps {
  /** `width` draws a vertical bar that resizes a horizontal track. */
  axis: 'width' | 'height';
  label: string;
  /** Called with the pointer delta in pixels along the axis. */
  onResize: (delta: number) => void;
}

/**
 * Keyboard- and pointer-resizable divider. Pointer capture keeps the drag alive
 * when the cursor leaves the thin bar.
 */
export function Splitter({ axis, label, onResize }: SplitterProps) {
  const last = useRef<number | null>(null);

  const position = (event: ReactPointerEvent<HTMLDivElement>): number => (
    axis === 'width' ? event.clientX : event.clientY
  );

  const step = (delta: number) => onResize(delta);

  const onKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const amount = event.shiftKey ? 40 : 12;
    const negative = axis === 'width' ? 'ArrowLeft' : 'ArrowUp';
    const positive = axis === 'width' ? 'ArrowRight' : 'ArrowDown';
    if (event.key === negative) { event.preventDefault(); step(-amount); }
    else if (event.key === positive) { event.preventDefault(); step(amount); }
  };

  return (
    <div
      aria-label={label}
      aria-orientation={axis === 'width' ? 'vertical' : 'horizontal'}
      className={`splitter splitter--${axis}`}
      onKeyDown={onKeyDown}
      onPointerCancel={() => { last.current = null; }}
      onPointerDown={(event) => {
        last.current = position(event);
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (last.current === null) return;
        const current = position(event);
        const delta = current - last.current;
        if (delta === 0) return;
        last.current = current;
        step(delta);
      }}
      onPointerUp={(event) => {
        last.current = null;
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId);
        }
      }}
      role="separator"
      tabIndex={0}
    />
  );
}
