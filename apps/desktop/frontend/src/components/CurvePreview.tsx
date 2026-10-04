import type { ReactNode, SVGProps } from 'react';
import type { ParameterValue } from '../editor/types';

export type Point = [number, number];
export interface CurveDomain { xMin: number; xMax: number; yMin: number; yMax: number }
export function curveDomain(points: Point[]): CurveDomain {
  return { xMin: Math.min(0, ...points.map(([x]) => x)), xMax: Math.max(1, ...points.map(([x]) => x)),
    yMin: Math.min(0, ...points.map(([, y]) => y)), yMax: Math.max(1, ...points.map(([, y]) => y)) };
}

/** Presentation only: Rust remains authoritative for evaluating image/graph values. */
export function curvePlot(typeId: string, value: ParameterValue): Point[] | null {
  if (typeId === 'core.curves') {
    if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) return null;
    return Array.from({ length: 65 }, (_, i) => [i / 64, (i / 64) ** (1 / value)]);
  }
  if (typeof value !== 'string' || value.length > 65536) return null;
  const scalar = typeId === 'core.curve';
  const pairs = value.split(';').filter((pair) => scalar || pair.trim() !== '');
  if (pairs.length < 2 || pairs.length > 4096) return null;
  const points: Point[] = [];
  for (const pair of pairs) {
    const fields = pair.split(',');
    if (fields.length !== 2 || fields.some((field) => !/^[+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$/i.test(field.trim()))) return null;
    const [x, y] = fields.map((field) => Math.fround(Number(field)));
    if (!Number.isFinite(x) || !Number.isFinite(y) || (!scalar && (x < 0 || x > 1))) return null;
    points.push([x, y]);
  }
  points.sort((a, b) => a[0] - b[0]);
  return points.some((point, i) => i > 0 && point[0] === points[i - 1][0]) ? null : points;
}

const format = (value: number) => Number(value.toPrecision(4)).toString().replace('-', '−');

export function CurvePreview({ typeId, value }: { typeId: string; value: ParameterValue }) {
  const points = curvePlot(typeId, value);
  if (!points) return <span className="curve-preview__invalid" role="status">{typeId === 'core.curves'
    ? 'Enter a finite gamma greater than zero to display the curve.'
    : `Enter at least two finite x,y pairs with distinct input values (x,y;x,y). ${typeId === 'core.curve' ? '' : 'Input values must be between 0 and 1. '}Display limit: 4096 points / 64 KiB.`}</span>;
  return <TransferPlot points={points} markers={typeId !== 'core.curves'}>
    {typeId.startsWith('pro.') && <small>Point mapping only; exposure and effect strength apply separately.</small>}
  </TransferPlot>;
}

export function TransferPlot({ points, markers = false, inputDomain, domain, svgProps, renderPoints, label = 'Transfer curve', children }: {
  points: Point[];
  markers?: boolean;
  inputDomain?: [number, number];
  domain?: CurveDomain;
  svgProps?: SVGProps<SVGSVGElement>;
  renderPoints?: (x: (n: number) => number, y: (n: number) => number) => ReactNode;
  label?: string;
  children?: ReactNode;
}) {
  const bounds = domain ?? curveDomain(points);
  const xMin = inputDomain?.[0] ?? bounds.xMin;
  const xMax = inputDomain?.[1] ?? bounds.xMax;
  const { yMin, yMax } = bounds;
  const x = (n: number) => 8 + (n - xMin) / (xMax - xMin) * 184;
  const y = (n: number) => 112 - (n - yMin) / (yMax - yMin) * 104;
  // Point curves hold their endpoint values outside the authored input domain.
  const trace: Point[] = [[xMin, points[0][1]], ...points, [xMax, points[points.length - 1][1]]];
  return <figure className="curve-preview">
    <svg viewBox="0 0 200 120" role="img" aria-label={`${label}: input on horizontal axis, output on vertical axis`} {...svgProps}>
      <path className="curve-preview__grid" d="M8 8H192V112H8Z M54 8V112 M100 8V112 M146 8V112 M8 34H192 M8 60H192 M8 86H192" />
      {Math.max(xMin, yMin) < Math.min(xMax, yMax) && <line className="curve-preview__neutral" x1={x(Math.max(xMin, yMin))} y1={y(Math.max(xMin, yMin))} x2={x(Math.min(xMax, yMax))} y2={y(Math.min(xMax, yMax))} />}
      <polyline points={trace.map(([a, b]) => `${x(a)},${y(b)}`).join(' ')} />
      {renderPoints ? renderPoints(x, y) : markers && points.map(([a, b], i) => <circle key={i} cx={x(a)} cy={y(b)} r="2.5" />)}
    </svg>
    <figcaption><span>Input {format(xMin)} to {format(xMax)}</span><span>Output {format(yMin)} to {format(yMax)}</span></figcaption>
    {children}
  </figure>;
}
