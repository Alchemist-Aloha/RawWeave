import { useEffect, useRef } from 'react';
import { samplePixel, type ImageAnalysis, type PixelSample } from '../viewer/analysis';

export interface ScopesProps {
  analysis: ImageAnalysis;
}

type DrawScope = (context: CanvasRenderingContext2D, width: number, height: number) => void;

function ScopeCanvas({ label, draw }: { label: string; draw: DrawScope }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let context: CanvasRenderingContext2D | null = null;
    try {
      context = canvas.getContext('2d');
    } catch {
      // Canvas is optional in non-browser test environments.
    }
    if (!context) return;
    context.clearRect(0, 0, canvas.width, canvas.height);
    draw(context, canvas.width, canvas.height);
  }, [draw]);

  return (
    <figure aria-label={label} className="scope-card">
      <figcaption>{label}</figcaption>
      <canvas className="scope-canvas" height={128} ref={canvasRef} width={256} />
    </figure>
  );
}

function maxValue(values: ArrayLike<number>): number {
  let maximum = 0;
  for (let index = 0; index < values.length; index += 1) maximum = Math.max(maximum, values[index] ?? 0);
  return maximum;
}

function drawHistogram(analysis: ImageAnalysis): DrawScope {
  return (context, width, height) => {
    const channels = [analysis.histogram.red, analysis.histogram.green, analysis.histogram.blue, analysis.histogram.luma];
    const colors = ['#ef737d', '#7cde9c', '#7eafff', '#dce6ed'];
    const maximum = Math.max(1, ...channels.map(maxValue));
    context.fillStyle = '#0b1016';
    context.fillRect(0, 0, width, height);
    channels.forEach((channel, channelIndex) => {
      context.beginPath();
      channel.forEach((value, index) => {
        const x = (index / (channel.length - 1)) * width;
        const y = height - (value / maximum) * (height - 8) - 4;
        if (index === 0) context.moveTo(x, y);
        else context.lineTo(x, y);
      });
      context.strokeStyle = colors[channelIndex] ?? '#dce6ed';
      context.globalAlpha = channelIndex === 3 ? 0.85 : 0.55;
      context.lineWidth = channelIndex === 3 ? 1.5 : 1;
      context.stroke();
    });
    context.globalAlpha = 1;
  };
}

function drawWaveform(analysis: ImageAnalysis, channel: 'red' | 'green' | 'blue' | 'luma', color: string): DrawScope {
  return (context, width, height) => {
    const values = analysis.waveform[channel];
    const maximum = Math.max(1, maxValue(values));
    context.fillStyle = '#0b1016';
    context.fillRect(0, 0, width, height);
    context.fillStyle = color;
    for (let y = 0; y < analysis.waveform.height; y += 1) {
      for (let x = 0; x < analysis.waveform.width; x += 1) {
        const value = values[y * analysis.waveform.width + x] ?? 0;
        if (!value) continue;
        context.globalAlpha = Math.min(1, 0.12 + value / maximum);
        context.fillRect(
          (x / analysis.waveform.width) * width,
          ((analysis.waveform.height - 1 - y) / analysis.waveform.height) * height,
          Math.max(1, width / analysis.waveform.width),
          Math.max(1, height / analysis.waveform.height),
        );
      }
    }
    context.globalAlpha = 1;
  };
}

function drawRgbParade(analysis: ImageAnalysis): DrawScope {
  return (context, width, height) => {
    const channels = [analysis.waveform.red, analysis.waveform.green, analysis.waveform.blue];
    const colors = ['#ef737d', '#7cde9c', '#7eafff'];
    const maximum = Math.max(1, ...channels.map(maxValue));
    context.fillStyle = '#0b1016';
    context.fillRect(0, 0, width, height);
    channels.forEach((channel, channelIndex) => {
      const startX = (channelIndex * width) / 3;
      const columnWidth = width / 3;
      context.fillStyle = colors[channelIndex] ?? '#dce6ed';
      for (let y = 0; y < analysis.waveform.height; y += 1) {
        for (let x = 0; x < analysis.waveform.width; x += 1) {
          const value = channel[y * analysis.waveform.width + x] ?? 0;
          if (!value) continue;
          context.globalAlpha = Math.min(1, 0.12 + value / maximum);
          context.fillRect(
            startX + (x / analysis.waveform.width) * columnWidth,
            ((analysis.waveform.height - 1 - y) / analysis.waveform.height) * height,
            Math.max(1, columnWidth / analysis.waveform.width),
            Math.max(1, height / analysis.waveform.height),
          );
        }
      }
    });
    context.globalAlpha = 1;
  };
}

function drawVectorscope(analysis: ImageAnalysis): DrawScope {
  return (context, width, height) => {
    const maximum = Math.max(1, maxValue(analysis.vectorscope));
    context.fillStyle = '#0b1016';
    context.fillRect(0, 0, width, height);
    context.strokeStyle = '#263241';
    context.beginPath();
    context.arc(width / 2, height / 2, Math.min(width, height) * 0.4, 0, Math.PI * 2);
    context.stroke();
    context.fillStyle = '#a4e8c9';
    for (let y = 0; y < 256; y += 1) {
      for (let x = 0; x < 256; x += 1) {
        const value = analysis.vectorscope[y * 256 + x] ?? 0;
        if (!value) continue;
        context.globalAlpha = Math.min(1, 0.15 + value / maximum);
        context.fillRect((x / 256) * width, (y / 256) * height, 1, 1);
      }
    }
    context.globalAlpha = 1;
  };
}

function drawRaster(raster: { width: number; height: number; data: Uint8ClampedArray }): DrawScope {
  return (context, width, height) => {
    try {
      const image = context.createImageData(raster.width, raster.height);
      image.data.set(raster.data);
      const scratch = document.createElement('canvas');
      scratch.width = raster.width;
      scratch.height = raster.height;
      const scratchContext = scratch.getContext('2d');
      if (!scratchContext) return;
      scratchContext.putImageData(image, 0, 0);
      context.imageSmoothingEnabled = false;
      context.drawImage(scratch, 0, 0, width, height);
    } catch {
      // Keep the accessible scope present when canvas APIs are unavailable.
    }
  };
}

function drawMask(analysis: ImageAnalysis, mask: Uint8Array, colors: { on: string; off: string }): DrawScope {
  return (context, width, height) => {
    context.fillStyle = colors.off;
    context.fillRect(0, 0, width, height);
    context.fillStyle = colors.on;
    for (let y = 0; y < analysis.inspector.height; y += 1) {
      for (let x = 0; x < analysis.inspector.width; x += 1) {
        if (!mask[y * analysis.inspector.width + x]) continue;
        context.fillRect(
          (x / analysis.inspector.width) * width,
          (y / analysis.inspector.height) * height,
          Math.max(1, width / analysis.inspector.width),
          Math.max(1, height / analysis.inspector.height),
        );
      }
    }
  };
}

function PixelInspector({ sample }: { sample: PixelSample | null }) {
  return (
    <div aria-label="Pixel Inspector" className="scope-card scope-card--inspector" role="img">
      <strong>Pixel Inspector</strong>
      {sample ? (
        <dl>
          <div><dt>RGB</dt><dd>{sample.red}, {sample.green}, {sample.blue}</dd></div>
          <div><dt>Hex</dt><dd>{sample.hex}</dd></div>
          <div><dt>Alpha</dt><dd>{sample.alpha}</dd></div>
          <div><dt>Position</dt><dd>{sample.x}, {sample.y}</dd></div>
        </dl>
      ) : <span>No pixel sampled</span>}
    </div>
  );
}

export function Scopes({ analysis }: ScopesProps) {
  const centerSample = samplePixel(analysis, Math.floor(analysis.width / 2), Math.floor(analysis.height / 2));
  return (
    <section aria-label="Image scopes" className="viewer-scopes">
      <ScopeCanvas draw={drawHistogram(analysis)} label="Histogram" />
      <ScopeCanvas draw={drawWaveform(analysis, 'luma', '#dce6ed')} label="Waveform" />
      <ScopeCanvas draw={drawRgbParade(analysis)} label="RGB Parade" />
      <ScopeCanvas draw={drawVectorscope(analysis)} label="Vectorscope" />
      <ScopeCanvas draw={drawRaster(analysis.falseColor)} label="False Color" />
      <ScopeCanvas draw={drawMask(analysis, analysis.gamutWarning, { on: '#ef737d', off: '#0b1016' })} label="Gamut Warning" />
      <PixelInspector sample={centerSample} />
      <ScopeCanvas draw={drawMask(analysis, analysis.zebra, { on: '#f0cf82', off: '#0b1016' })} label="Zebra" />
    </section>
  );
}
