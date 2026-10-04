import type { EditorNode } from '../editor/types';

const bounded = (value: number, min = 0, max = 1) => Number.isFinite(value) && value >= min && value <= max;
const format = (value: number) => Number(value.toPrecision(4)).toString();
const degrees = (value: number) => `${Number((value * 360).toFixed(1))}°`;

/** CSS sRGB reference only: no source/working-space transform is implied. */
export function referenceRgb(channels: number[]): string | null {
  return channels.length === 3 && channels.every((value) => bounded(value))
    ? `rgb(${channels.map((value) => `${Number((value * 100).toFixed(4))}%`).join(' ')})` : null;
}

/** Width is Rust's circular hue-distance radius, not the full selected arc. */
export function hueIntervals(hue: number, width: number): Array<[number, number]> | null {
  if (!bounded(hue) || !bounded(width, 0.001, 0.5)) return null;
  if (width === 0.5) return [[0, 1]];
  const start = hue % 1 - width;
  const end = hue % 1 + width;
  if (start < 0) return [[0, end], [1 + start, 1]];
  if (end > 1) return [[0, end - 1], [start, 1]];
  return [[start, end]];
}

function HueReference({ label, hue, width, saturation }: {
  label: string; hue: number; width?: number; saturation?: number;
}) {
  const intervals = width === undefined ? [] : hueIntervals(hue, width);
  if (!bounded(hue) || intervals === null || (saturation !== undefined && !bounded(saturation)))
    return <span className="curve-preview__invalid" role="status">{label} requires a finite hue from 0 to 360°, {width === undefined ? 'and saturation from 0 to 100%.' : 'and a radius from 0.36 to 180°.'}</span>;
  const description = `${label} ${degrees(hue)}${width === undefined ? `; saturation ${format(saturation! * 100)}%` : `; radius ${degrees(width)}`}`;
  return <figure className="color-hue" role="img" aria-label={description}>
    <figcaption>{description}</figcaption>
    <div className="color-hue__spectrum" aria-hidden="true">
      <span className="color-hue__marker" style={{ left: `${hue % 1 * 100}%` }} />
    </div>
    {width !== undefined && <div className="color-hue__range" aria-hidden="true">
      {intervals.map(([start, end], i) => <span className="color-hue__interval" key={i} style={{ left: `${start * 100}%`, width: `${(end - start) * 100}%` }} />)}
    </div>}
    <div className="color-hue__scale" aria-hidden="true"><span>0°</span><span>180°</span><span>360°</span></div>
    {saturation === 0 && <small>No tint at 0% saturation</small>}
  </figure>;
}

export function ColorParameterPreview({ node }: { node: EditorNode }) {
  if (!['core.mask-color-qualifier', 'pro.color-zones', 'pro.split-toning'].includes(node.typeId)) return null;
  const number = (id: string, fallback: number) => {
    const value = node.parameters[id] ?? node.descriptor.parameters.find((parameter) => parameter.id === id)?.default ?? fallback;
    return typeof value === 'number' ? value : NaN;
  };
  const qualifier = node.typeId === 'core.mask-color-qualifier';
  const channels = ['target_r', 'target_g', 'target_b'].map((id) => number(id, 1));
  const rgb = referenceRgb(channels);
  return <section className="color-parameter-preview" aria-label={`${node.descriptor.name} color reference`}>
    {qualifier ? <>
      {node.parameters.color !== undefined
        ? <span className="curve-preview__invalid" role="status">Legacy color parameter overrides the RGB fields; no target swatch is shown.</span>
        : rgb ? <div className="color-reference">
          <span className="color-reference__swatch" role="img" aria-label={`Target RGB ${channels.map(format).join(', ')}`} style={{ backgroundColor: rgb }} />
          <span className="color-reference__readout">RGB {channels.map(format).join(', ')}</span>
        </div> : <span className="curve-preview__invalid" role="status">Reference swatch requires finite RGB values between 0 and 1.</span>}
      <small>Reference only: input/working-space RGB shown as CSS sRGB, not color-managed.</small>
    </> : <>
      {node.typeId === 'pro.color-zones'
        ? <HueReference label="Target Hue" hue={number('hue', 0)} width={number('width', 0.2)} />
        : <>
          <HueReference label="Shadow Hue" hue={number('shadow_hue', 0.6)} saturation={number('shadow_saturation', 0)} />
          <HueReference label="Highlight Hue" hue={number('highlight_hue', 0.1)} saturation={number('highlight_saturation', 0)} />
        </>}
      <small>HSV hue reference only; spectrum is fully saturated, not a color-managed image result.</small>
    </>}
    <small>Applied values; updates on commit. Exposed inputs may override them.</small>
  </section>;
}
