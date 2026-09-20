export const SCOPE_RESOLUTION = 256;
export const CLIPPING_HIGHLIGHT = 1;
export const CLIPPING_SHADOW = 2;

const HIGHLIGHT_THRESHOLD = 0.98;
const SHADOW_THRESHOLD = 0.02;
const MAX_ANALYSIS_SAMPLES = 100_000;
const DEFAULT_INSPECTOR_SIZE = 256;

type Channel = Uint32Array;

export interface HistogramData {
  red: Channel;
  green: Channel;
  blue: Channel;
  luma: Channel;
}

export interface WaveformData {
  width: number;
  height: number;
  red: Channel;
  green: Channel;
  blue: Channel;
  luma: Channel;
}

export interface ClippingData {
  mask: Uint8Array;
  highlightCount: number;
  shadowCount: number;
}

export interface InspectorRaster {
  width: number;
  height: number;
  data: Uint8ClampedArray;
}

export interface PixelSample {
  x: number;
  y: number;
  red: number;
  green: number;
  blue: number;
  alpha: number;
  luma: number;
  hex: string;
}

export interface ImageAnalysis {
  width: number;
  height: number;
  sampleCount: number;
  histogram: HistogramData;
  waveform: WaveformData;
  vectorscope: Channel;
  clipping: ClippingData;
  inspector: InspectorRaster;
  falseColor: InspectorRaster;
  gamutWarning: Uint8Array;
  gamutWarningCount: number;
  zebra: Uint8Array;
  zebraCount: number;
}

export interface ImageAnalysisOptions {
  maxSamples?: number;
  inspectorSize?: number;
}

function clampByte(value: number): number {
  return Math.min(255, Math.max(0, Math.round(value)));
}

function clampUnit(value: number): number {
  return Math.min(1, Math.max(0, value));
}

function luma(red: number, green: number, blue: number): number {
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function level(value: number): number {
  return Math.min(255, Math.max(0, Math.round(value)));
}

function channelIndex(column: number, value: number): number {
  return column * SCOPE_RESOLUTION + level(value);
}

function sampledStep(width: number, height: number, requested: number): number {
  const maxSamples = Math.max(1, Math.floor(requested));
  let step = Math.max(1, Math.ceil(Math.sqrt((width * height) / maxSamples)));
  while (Math.ceil(width / step) * Math.ceil(height / step) > maxSamples) step += 1;
  return step;
}

function rasterSize(width: number, height: number, requested: number): { width: number; height: number } {
  const size = Math.max(1, Math.floor(requested));
  if (width <= size && height <= size) return { width, height };
  const scale = Math.min(size / width, size / height);
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  };
}

function falseColor(red: number, green: number, blue: number): [number, number, number] {
  const value = luma(red, green, blue) / 255;
  const stops: Array<[number, [number, number, number]]> = [
    [0, [26, 12, 72]],
    [0.2, [25, 104, 220]],
    [0.4, [20, 190, 200]],
    [0.6, [72, 210, 74]],
    [0.8, [245, 211, 42]],
    [0.95, [226, 65, 42]],
    [1, [255, 255, 255]],
  ];
  for (let index = 1; index < stops.length; index += 1) {
    const [end, endColor] = stops[index];
    const [start, startColor] = stops[index - 1];
    if (value > end) continue;
    const amount = clampUnit((value - start) / (end - start));
    return [
      startColor[0] + (endColor[0] - startColor[0]) * amount,
      startColor[1] + (endColor[1] - startColor[1]) * amount,
      startColor[2] + (endColor[2] - startColor[2]) * amount,
    ].map(clampByte) as [number, number, number];
  }
  return stops.at(-1)![1];
}

function pixelAt(data: Uint8ClampedArray, width: number, x: number, y: number): [number, number, number, number] {
  const index = (y * width + x) * 4;
  return [data[index] ?? 0, data[index + 1] ?? 0, data[index + 2] ?? 0, data[index + 3] ?? 255];
}

export function analyzeImageData(imageData: ImageData, options: ImageAnalysisOptions = {}): ImageAnalysis {
  const width = Math.max(1, Math.floor(imageData.width));
  const height = Math.max(1, Math.floor(imageData.height));
  const data = imageData.data;
  const histogram: HistogramData = {
    red: new Uint32Array(SCOPE_RESOLUTION),
    green: new Uint32Array(SCOPE_RESOLUTION),
    blue: new Uint32Array(SCOPE_RESOLUTION),
    luma: new Uint32Array(SCOPE_RESOLUTION),
  };
  const waveform: WaveformData = {
    width: SCOPE_RESOLUTION,
    height: SCOPE_RESOLUTION,
    red: new Uint32Array(SCOPE_RESOLUTION * SCOPE_RESOLUTION),
    green: new Uint32Array(SCOPE_RESOLUTION * SCOPE_RESOLUTION),
    blue: new Uint32Array(SCOPE_RESOLUTION * SCOPE_RESOLUTION),
    luma: new Uint32Array(SCOPE_RESOLUTION * SCOPE_RESOLUTION),
  };
  const vectorscope = new Uint32Array(SCOPE_RESOLUTION * SCOPE_RESOLUTION);
  const clippingMask = new Uint8Array(width * height);
  const maxSamples = Math.max(1, options.maxSamples ?? MAX_ANALYSIS_SAMPLES);
  const step = sampledStep(width, height, maxSamples);
  let sampleCount = 0;

  for (let y = 0; y < height; y += step) {
    for (let x = 0; x < width; x += step) {
      const [red, green, blue] = pixelAt(data, width, x, y);
      const gray = luma(red, green, blue);
      const column = Math.min(SCOPE_RESOLUTION - 1, Math.floor((x / width) * SCOPE_RESOLUTION));
      const redLevel = level(red);
      const greenLevel = level(green);
      const blueLevel = level(blue);
      const lumaLevel = level(gray);
      histogram.red[redLevel] += 1;
      histogram.green[greenLevel] += 1;
      histogram.blue[blueLevel] += 1;
      histogram.luma[lumaLevel] += 1;
      waveform.red[channelIndex(column, red)] += 1;
      waveform.green[channelIndex(column, green)] += 1;
      waveform.blue[channelIndex(column, blue)] += 1;
      waveform.luma[channelIndex(column, gray)] += 1;

      const chromaX = clampByte(((blue - gray) / 255 + 0.5) * (SCOPE_RESOLUTION - 1));
      const chromaY = clampByte((0.5 - (red - gray) / 255) * (SCOPE_RESOLUTION - 1));
      vectorscope[chromaY * SCOPE_RESOLUTION + chromaX] += 1;
      sampleCount += 1;
    }
  }

  let highlightCount = 0;
  let shadowCount = 0;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const [red, green, blue] = pixelAt(data, width, x, y);
      const maximum = Math.max(red, green, blue) / 255;
      const minimum = Math.min(red, green, blue) / 255;
      const flags = (maximum >= HIGHLIGHT_THRESHOLD ? CLIPPING_HIGHLIGHT : 0)
        | (maximum <= SHADOW_THRESHOLD ? CLIPPING_SHADOW : 0);
      clippingMask[y * width + x] = flags;
      if (flags & CLIPPING_HIGHLIGHT) highlightCount += 1;
      if (flags & CLIPPING_SHADOW) shadowCount += 1;
    }
  }

  const inspector = rasterSize(width, height, options.inspectorSize ?? DEFAULT_INSPECTOR_SIZE);
  const inspectorData = new Uint8ClampedArray(inspector.width * inspector.height * 4);
  const falseColorData = new Uint8ClampedArray(inspectorData.length);
  const gamutWarning = new Uint8Array(inspector.width * inspector.height);
  const zebra = new Uint8Array(inspector.width * inspector.height);
  let gamutWarningCount = 0;
  let zebraCount = 0;
  for (let y = 0; y < inspector.height; y += 1) {
    const sourceY = Math.min(height - 1, Math.round((y * (height - 1)) / Math.max(1, inspector.height - 1)));
    for (let x = 0; x < inspector.width; x += 1) {
      const sourceX = Math.min(width - 1, Math.round((x * (width - 1)) / Math.max(1, inspector.width - 1)));
      const [red, green, blue, alpha] = pixelAt(data, width, sourceX, sourceY);
      const sourceIndex = (sourceY * width + sourceX) * 4;
      const index = (y * inspector.width + x) * 4;
      inspectorData[index] = red;
      inspectorData[index + 1] = green;
      inspectorData[index + 2] = blue;
      inspectorData[index + 3] = alpha;
      const [falseRed, falseGreen, falseBlue] = falseColor(red, green, blue);
      falseColorData[index] = falseRed;
      falseColorData[index + 1] = falseGreen;
      falseColorData[index + 2] = falseBlue;
      falseColorData[index + 3] = 255;
      const saturated = Math.max(red, green, blue) >= 250 && Math.min(red, green, blue) < 250;
      gamutWarning[y * inspector.width + x] = saturated ? 1 : 0;
      if (saturated) gamutWarningCount += 1;
      const zebraFlags = clippingMask[sourceY * width + sourceX] ?? 0;
      zebra[y * inspector.width + x] = zebraFlags;
      if (zebraFlags !== 0) zebraCount += 1;
      void sourceIndex;
    }
  }

  return {
    width,
    height,
    sampleCount,
    histogram,
    waveform,
    vectorscope,
    clipping: { mask: clippingMask, highlightCount, shadowCount },
    inspector: { ...inspector, data: inspectorData },
    falseColor: { ...inspector, data: falseColorData },
    gamutWarning,
    gamutWarningCount,
    zebra,
    zebraCount,
  };
}

export function samplePixel(analysis: ImageAnalysis, x: number, y: number): PixelSample | null {
  if (analysis.inspector.width < 1 || analysis.inspector.height < 1) return null;
  const sourceX = Math.min(analysis.width - 1, Math.max(0, Math.floor(x)));
  const sourceY = Math.min(analysis.height - 1, Math.max(0, Math.floor(y)));
  const inspectorX = Math.min(
    analysis.inspector.width - 1,
    Math.max(0, Math.round((sourceX * (analysis.inspector.width - 1)) / Math.max(1, analysis.width - 1))),
  );
  const inspectorY = Math.min(
    analysis.inspector.height - 1,
    Math.max(0, Math.floor((sourceY * analysis.inspector.height) / analysis.height)),
  );
  const [red, green, blue, alpha] = pixelAt(analysis.inspector.data, analysis.inspector.width, inspectorX, inspectorY);
  const toHex = (value: number) => value.toString(16).padStart(2, '0');
  return {
    x: sourceX,
    y: sourceY,
    red,
    green,
    blue,
    alpha,
    luma: luma(red, green, blue),
    hex: `#${toHex(red)}${toHex(green)}${toHex(blue)}`,
  };
}

export function analyzeImageElement(image: HTMLImageElement, options: ImageAnalysisOptions = {}): ImageAnalysis | null {
  const width = image.naturalWidth || image.width;
  const height = image.naturalHeight || image.height;
  if (!width || !height || typeof document === 'undefined') return null;
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (!context) return null;
  try {
    context.drawImage(image, 0, 0, width, height);
    return analyzeImageData(context.getImageData(0, 0, width, height), options);
  } catch {
    return null;
  }
}

export function drawClippingOverlay(analysis: ImageAnalysis, canvas: HTMLCanvasElement): void {
  canvas.width = analysis.width;
  canvas.height = analysis.height;
  const context = canvas.getContext('2d');
  if (!context) return;
  const pixels = new Uint8ClampedArray(analysis.width * analysis.height * 4);
  for (let index = 0; index < analysis.clipping.mask.length; index += 1) {
    const flags = analysis.clipping.mask[index] ?? 0;
    if (!flags) continue;
    const pixelIndex = index * 4;
    pixels[pixelIndex] = flags === CLIPPING_SHADOW ? 34 : 255;
    pixels[pixelIndex + 1] = flags === CLIPPING_HIGHLIGHT ? 52 : 124;
    pixels[pixelIndex + 2] = flags === CLIPPING_HIGHLIGHT ? 52 : 255;
    pixels[pixelIndex + 3] = 170;
  }
  const overlay = context.createImageData(analysis.width, analysis.height);
  overlay.data.set(pixels);
  context.putImageData(overlay, 0, 0);
}
