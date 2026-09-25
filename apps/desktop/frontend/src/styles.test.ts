import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
/**
 * The Legibility Floor (see DESIGN.md, Typography): mono chrome may go to 9px,
 * everything a user reads or acts on sits at 11px. Sub-9px type is the drift
 * this guards against, so the test fails the moment a declaration drops below it.
 */
const FLOOR_PX = 9;
const fileName = ['src/styles.css', 'apps/desktop/frontend/src/styles.css'].find((candidate) =>
  existsSync(resolve(process.cwd(), candidate)),
);
if (!fileName) throw new Error('styles.css not found; run this suite from the frontend package');
const css = readFileSync(resolve(process.cwd(), fileName), 'utf8');

describe('styles.css legibility floor', () => {
  it('never renders type below the mono-chrome floor', () => {
    const offenders = [...css.matchAll(/font-size:\s*([\d.]+)px/g)]
      .map((match) => ({
        size: Number(match[1]),
        line: css.slice(0, match.index).split('\n').length,
      }))
      .filter((entry) => entry.size < FLOOR_PX)
      .map((entry) => `line ${entry.line}: ${entry.size}px`);

    expect(offenders).toEqual([]);
  });
});

/**
 * The secondary-text tier must clear AA on every surface it can sit on, including
 * hovered rows and tiles. This list is the calibration knob: when a new surface
 * lighter than these is introduced, add it here and re-check the tier.
 */
const SURFACES = ['#19232d', '#17242a', '#151b24', '#141b24', '#141a23', '#111821', '#111720', '#10141b', '#0e151c', '#0d1117', '#0b0d12'];

/** Declarations that are not secondary text colour usage. */
const CONTRAST_EXEMPT: Record<string, string> = {
  '#07100e': 'text set ON a mint fill, not on the dark ground (.button--primary is 12.8:1 against its own background)',
  '#536478': 'large 22px placeholder glyph: WCAG 1.4.3 exempts large text at 3:1, measured 3.1:1',
  '#b9757e': 'dismiss glyph on a control, not text: WCAG 1.4.11 requires 3:1, measured 4.9:1',
};

const srgb = (channel: number) => {
  const c = channel / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
};

const luminance = (hex: string) => {
  const [r, g, b] = [1, 3, 5].map((i) => srgb(Number.parseInt(hex.slice(i, i + 2), 16)));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};

const ratio = (a: string, b: string) => {
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (high + 0.05) / (low + 0.05);
};

describe('styles.css contrast', () => {
  it('keeps every text colour at AA or better on its worst plausible surface', () => {
    const offenders: string[] = [];
    for (const line of css.split('\n')) {
      for (const [, hex] of line.matchAll(/(?:^|[{;\s])(?:color|fill):\s*(#[0-9a-fA-F]{6})/g)) {
        const colour = hex.toLowerCase();
        if (colour in CONTRAST_EXEMPT) continue;
        const worst = SURFACES.reduce((a, b) => (ratio(colour, a) <= ratio(colour, b) ? a : b));
        const measured = ratio(colour, worst);
        if (measured < 4.5) offenders.push(`${colour} at ${measured.toFixed(2)}:1 on ${worst}`);
      }
    }

    expect([...new Set(offenders)]).toEqual([]);
  });
});
