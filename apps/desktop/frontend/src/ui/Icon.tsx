/**
 * The app's control marks, drawn rather than typed.
 *
 * A unicode arrow is a glyph from whichever font resolves it, at that font's
 * weight, sitting beside 1.5px hairlines it has nothing to do with — and it
 * changes shape when the font falls back. These are drawn on one 12px grid in one
 * stroke weight, inherit the ink of the control they sit in, and never change
 * with the type.
 */
interface IconProps {
  name:
    | 'chevronDown'
    | 'chevronLeft'
    | 'chevronRight'
    | 'close'
    | 'plus'
    | 'minus'
    | 'arrowUp'
    | 'arrowDown';
  className?: string;
}

const PATHS: Record<IconProps['name'], string> = {
  chevronDown: 'M2.5 4.5L6 8L9.5 4.5',
  chevronLeft: 'M7.5 2.5L4 6L7.5 9.5',
  chevronRight: 'M4.5 2.5L8 6L4.5 9.5',
  close: 'M3 3L9 9M9 3L3 9',
  plus: 'M6 2.5V9.5M2.5 6H9.5',
  minus: 'M2.5 6H9.5',
  arrowUp: 'M6 9.5V2.5M2.5 6L6 2.5L9.5 6',
  arrowDown: 'M6 2.5V9.5M2.5 6L6 9.5L9.5 6',
};

export function Icon({ className, name }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      className={className ? `icon ${className}` : 'icon'}
      fill="none"
      focusable="false"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={1.5}
      viewBox="0 0 12 12"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
