import type { EditorNode } from '../editor/types';
import { TransferPlot, type Point } from './CurvePreview';

interface Transfer {
  points?: Point[];
  inputDomain?: [number, number];
  description?: string;
  error?: string;
}

const format = (value: number) => Number(value.toPrecision(4)).toString().replace('-', '−');

/** Authored-parameter diagram only, never an evaluated graph or image result. */
export function parameterTransferPlot(node: EditorNode): Transfer | null {
  if (!['core.levels', 'core.map-range', 'core.clamp'].includes(node.typeId)) return null;
  const read = (id: string, fallback: number | boolean) => node.parameters[id]
    ?? node.descriptor.parameters.find((parameter) => parameter.id === id)?.default ?? fallback;
  const number = (id: string, fallback: number) => {
    const value = read(id, fallback);
    return typeof value === 'number' ? Math.fround(value) : NaN;
  };
  let points: Point[];
  let description: string;
  if (node.typeId === 'core.levels') {
    const black = number('black_point', 0);
    const white = number('white_point', 1);
    const gamma = number('gamma', 1);
    if (![black, white, gamma].every(Number.isFinite)) return { error: 'Levels requires finite numeric black, white and gamma values.' };
    if (white <= black) return { error: 'White Point must be greater than Black Point.' };
    if (gamma <= 0) return { error: 'Midtone Gamma must be greater than zero.' };
    const domain: [number, number] = [Math.min(0, black), Math.max(1, white)];
    const inputs = [...new Set([domain[0], ...Array.from({ length: 65 }, (_, i) => black + (white - black) * i / 64), domain[1]])];
    points = inputs.map((x) => [x, Math.min(1, Math.max(0, (x - black) / (white - black))) ** (1 / gamma)]);
    description = `Black ${format(black)} maps to 0; white ${format(white)} maps to 1. Values outside these endpoints clip.`;
  } else {
    const mapping = node.typeId === 'core.map-range';
    const start = number(mapping ? 'in_min' : 'min', 0);
    const end = number(mapping ? 'in_max' : 'max', 1);
    const outStart = mapping ? number('out_min', 0) : start;
    const outEnd = mapping ? number('out_max', 1) : end;
    const clamp = mapping ? read('clamp', false) : true;
    if (![start, end, outStart, outEnd].every(Number.isFinite) || typeof clamp !== 'boolean') return { error: 'Range endpoints must be finite numbers and Clamp must be a boolean.' };
    const span = end - start;
    if (mapping && Math.abs(Math.fround(span)) <= 2 ** -23) return { error: 'Input endpoints must differ by more than float epsilon (1.192093e−7).' };
    if (!mapping && start > end) return { error: 'Minimum must not exceed Maximum.' };
    const low = Math.min(start, end);
    const high = Math.max(start, end);
    const padding = (high - low || Math.max(1, Math.abs(low))) / 4;
    const inputs = [...new Set([low - padding, low, high, high + padding])];
    points = inputs.map((x) => {
      const mapped = mapping ? outStart + (x - start) / span * (outEnd - outStart) : x;
      return [x, clamp ? Math.min(Math.max(outStart, outEnd), Math.max(Math.min(outStart, outEnd), mapped)) : mapped];
    });
    description = mapping
      ? `Input ${format(start)} maps to ${format(outStart)}; input ${format(end)} maps to ${format(outEnd)}. ${clamp ? 'Clamped' : 'Extrapolates'} outside the input interval.`
      : `Values below ${format(start)} hold at ${format(start)}; values above ${format(end)} hold at ${format(end)}.`;
  }
  if (points.some((point) => point.some((value) => !Number.isFinite(value)))) return { error: 'This parameter range cannot be displayed with finite coordinates.' };
  return { points, inputDomain: [points[0][0], points[points.length - 1][0]], description };
}

export function ParameterTransferPreview({ node }: { node: EditorNode }) {
  const plot = parameterTransferPlot(node);
  if (!plot) return null;
  return <section className="parameter-transfer-preview" aria-label={`${node.descriptor.name} parameter mapping`}>
    {plot.points ? <TransferPlot points={plot.points} inputDomain={plot.inputDomain} label={`${node.descriptor.name} transfer curve`}>
      <small>{plot.description}</small>
    </TransferPlot> : <span className="curve-preview__invalid" role="status">{plot.error}</span>}
    <small>Applied parameter values; edits update this plot on commit. Exposed inputs may override them.</small>
  </section>;
}
