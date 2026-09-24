import { useEffect, useRef } from 'react';

export interface ContextMenuItem {
  id: string;
  label: string;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
}

export interface ContextMenuState {
  x: number;
  y: number;
  title: string;
  items: ContextMenuItem[];
}

interface CanvasContextMenuProps {
  menu: ContextMenuState | null;
  onClose: () => void;
}

const MENU_WIDTH = 226;

/**
 * Right-click menu for the canvas. Rendered as a fixed-position element so it
 * is not clipped by the canvas overflow, and closed on outside pointerdown or
 * Escape.
 */
export function CanvasContextMenu({ menu, onClose }: CanvasContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu) return;
    const onPointerDown = (event: PointerEvent) => {
      if (ref.current?.contains(event.target as Node)) return;
      onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('keydown', onKeyDown);
    };
  }, [menu, onClose]);

  if (!menu) return null;

  const left = Math.max(8, Math.min(menu.x, window.innerWidth - MENU_WIDTH - 8));
  const estimatedHeight = 34 + menu.items.length * 30;
  const top = Math.max(8, Math.min(menu.y, window.innerHeight - estimatedHeight - 8));

  return (
    <div
      className="context-menu"
      ref={ref}
      role="menu"
      style={{ left, top, width: MENU_WIDTH }}
    >
      <span className="context-menu__title">{menu.title}</span>
      {menu.items.map((item) => (
        <button
          className={`context-menu__item${item.danger ? ' is-danger' : ''}`}
          disabled={item.disabled}
          key={item.id}
          onClick={() => {
            item.onSelect();
            onClose();
          }}
          role="menuitem"
          type="button"
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
