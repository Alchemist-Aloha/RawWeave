import { describe, expect, it } from 'vitest';
import { analyzeImageData, samplePixel } from './analysis';

function imageData(width: number, pixels: number[]): ImageData {
  return { data: new Uint8ClampedArray(pixels), width, height: pixels.length / (width * 4) } as ImageData;
}

describe('preview image analysis', () => {
  it('derives histogram, clipping, gamut, and zebra data from preview pixels', () => {
    const analysis = analyzeImageData(imageData(4, [
      255, 255, 255, 255,
      0, 0, 0, 255,
      255, 0, 0, 255,
      32, 32, 32, 255,
    ]));

    expect(analysis.sampleCount).toBe(4);
    expect(analysis.histogram.red[255]).toBe(2);
    expect(analysis.histogram.green[0]).toBe(2);
    expect(analysis.clipping.highlightCount).toBe(2);
    expect(analysis.clipping.shadowCount).toBe(1);
    expect(analysis.gamutWarningCount).toBeGreaterThan(0);
    expect(analysis.zebraCount).toBe(3);
  });

  it('bounds scope sampling and keeps pixel inspection available on the sampled raster', () => {
    const pixels = Array.from({ length: 16 * 16 }, (_, index) => [index, 20, 40, 255]).flat();
    const analysis = analyzeImageData(imageData(16, pixels), { maxSamples: 20, inspectorSize: 8 });

    expect(analysis.sampleCount).toBeLessThanOrEqual(20);
    expect(analysis.inspector.width).toBeLessThanOrEqual(8);
    expect(analysis.inspector.height).toBeLessThanOrEqual(8);
    expect(samplePixel(analysis, 15, 15)?.red).toBe(255);
  });
});
