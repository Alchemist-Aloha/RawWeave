import { useEffect, useState } from 'react';
import type { EditorNode, ParameterValue } from '../editor/types';
import type { ImageDimensions, PreviewTarget } from '../viewer/types';
import { ViewerController } from '../viewer/controller';
import { DRAWABLE_NODES } from '../viewer/region';

const CROP_PRESETS = [
  { value: 'full', label: 'Full image', ratio: null },
  { value: '1:1', label: 'Square 1:1', ratio: 1 },
  { value: '3:2', label: '3:2', ratio: 3 / 2 },
  { value: '4:3', label: '4:3', ratio: 4 / 3 },
  { value: '16:9', label: '16:9', ratio: 16 / 9 },
] as const;

function knownSize(
  size: ImageDimensions | null | undefined,
): ImageDimensions | null {
  return size &&
    Number.isSafeInteger(size.width) &&
    size.width > 0 &&
    Number.isSafeInteger(size.height) &&
    size.height > 0
    ? size
    : null;
}

function cropRegion(size: ImageDimensions | null, ratio: number | null) {
  if (!size) return null;
  const width =
    ratio === null
      ? size.width
      : Math.min(size.width, Math.floor(size.height * ratio));
  const height =
    ratio === null
      ? size.height
      : Math.min(size.height, Math.floor(size.width / ratio));
  if (width < 1 || height < 1) return null;
  return {
    x: Math.floor((size.width - width) / 2),
    y: Math.floor((size.height - height) / 2),
    width,
    height,
  };
}

export function ImageNodeControls({
  node,
  target,
  inputSize,
  controller,
  onDraw,
  onChange,
}: {
  node: EditorNode;
  target: PreviewTarget | null;
  inputSize?: ImageDimensions;
  controller: ViewerController;
  onDraw: () => void;
  onChange: (values: Record<string, ParameterValue>) => void;
}) {
  const [, render] = useState(0);
  useEffect(
    () => controller.subscribe(() => render((value) => value + 1)),
    [controller],
  );
  const drawable = DRAWABLE_NODES.has(node.typeId);
  const crop = node.typeId === 'core.crop';
  const resize = node.typeId === 'core.resize';
  if (
    !drawable &&
    !resize &&
    !node.descriptor.inputs.some((input) => input.id === 'image')
  )
    return null;
  const size =
    knownSize(target ? controller.getTargetDimensions(target) : null) ??
    knownSize(inputSize);
  const parameter = (id: string, fallback = NaN) => {
    const value =
      node.parameters[id] ??
      node.descriptor.parameters.find((parameter) => parameter.id === id)
        ?.default ??
      fallback;
    return typeof value === 'number' ? value : NaN;
  };
  const output = knownSize({
    width: parameter('width'),
    height: parameter('height'),
  });
  const x = parameter('x', 0);
  const y = parameter('y', 0);
  const cropFits =
    size &&
    output &&
    Number.isSafeInteger(x) &&
    x >= 0 &&
    Number.isSafeInteger(y) &&
    y >= 0 &&
    x <= size.width - output.width &&
    y <= size.height - output.height;
  const result = crop ? (cropFits ? output : null) : output;
  const driven = node.exposedParameters?.some((id) =>
    [
      'x',
      'y',
      'width',
      'height',
      'start_x',
      'start_y',
      'end_x',
      'end_y',
      'center_x',
      'center_y',
      'radius',
    ].includes(id),
  );
  const resizeTo = (scale: number) => {
    if (size)
      onChange({
        width: Math.max(1, Math.round(size.width * scale)),
        height: Math.max(1, Math.round(size.height * scale)),
      });
  };
  return (
    <section
      className="image-node-controls"
      aria-label={`${node.descriptor.name} image controls`}
    >
      <span>
        {size
          ? `Input resolution: ${size.width} × ${size.height} px`
          : target
            ? 'Preview input to read its resolution'
            : 'Connect an image input to use these controls'}
      </span>
      {(crop || resize) && (
        <span>
          {crop ? 'Crop result resolution' : 'Output resolution'}:{' '}
          {result ? `${result.width} × ${result.height} px` : 'unknown'}
        </span>
      )}
      <button type="button" disabled={!target} onClick={onDraw}>
        {resize
          ? 'View input size'
          : crop
            ? 'Draw crop region'
            : drawable
              ? 'Draw gradient'
              : 'Preview input'}
      </button>
      {driven && (
        <small>Exposed parameter ports may override these values.</small>
      )}
      {crop && (
        <>
          <label className="parameter">
            Crop preset
            <select
              aria-label="Crop preset"
              value=""
              disabled={!size}
              onChange={(event) => {
                const preset = CROP_PRESETS.find(
                  (preset) => preset.value === event.currentTarget.value,
                );
                const region = preset ? cropRegion(size, preset.ratio) : null;
                if (region) onChange(region);
              }}
            >
              <option value="" disabled>
                Choose preset
              </option>
              {CROP_PRESETS.map((preset) => (
                <option
                  key={preset.value}
                  value={preset.value}
                  disabled={!cropRegion(size, preset.ratio)}
                >
                  {preset.label}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            disabled={!size}
            onClick={() => {
              const region = cropRegion(size, null);
              if (region) onChange(region);
            }}
          >
            Use full image
          </button>
        </>
      )}
      {resize && (
        <>
          <div>
            <button type="button" disabled={!size} onClick={() => resizeTo(1)}>
              Original size
            </button>
            <button
              type="button"
              disabled={!size}
              onClick={() => resizeTo(0.5)}
            >
              Half size
            </button>
          </div>
          <div>
            <button
              type="button"
              disabled={!size}
              onClick={() => resizeTo(0.25)}
            >
              Quarter size
            </button>
            <button
              type="button"
              disabled={!size}
              onClick={() => resizeTo(0.75)}
            >
              Three-quarter size
            </button>
          </div>
        </>
      )}
    </section>
  );
}
