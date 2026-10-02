import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * The world has two casts and one measuring station, and the cast a colour is
 * declared in decides what it has to survive:
 *
 *   room   warm near-black   handling chrome
 *   bench  lit acrylic       the one plane frames are laid out on
 *   plane  the bench with the lamp switched off
 *   judge  neutral grey      the measuring station
 *
 * This test used to check every text colour against a single flat list of dark
 * surfaces. That stops meaning anything the moment a plane is light: the same
 * colour cannot be measured against a warm ground and a lit sheet at once. So the
 * surfaces are now derived from the token table itself, per cast, and the rules
 * are stricter than before:
 *
 *   1. No type below the 9px floor.
 *   2. No loose colour: every hex in the stylesheet must be a declared token
 *      value, so a colour cannot be introduced without landing in a cast.
 *   3. Every ink token clears AA against the worst surface of every cast it is
 *      declared for — the worst being the surface closest to it in luminance,
 *      which is what a single flat list used to approximate by luck.
 */
const FLOOR_PX = 9;
const fileName = ['src/styles.css', 'apps/desktop/frontend/src/styles.css'].find((candidate) =>
  existsSync(resolve(process.cwd(), candidate)),
);
if (!fileName) throw new Error('styles.css not found; run this suite from the frontend package');
const css = readFileSync(resolve(process.cwd(), fileName), 'utf8');
/** Comments carry prose about the palette and the font import carries an `@`, so
 *  neither is parsed as a declaration. */
const sheet = css.replace(/\/\*[\s\S]*?\*\//g, '').replace(/@import[^;]*;/g, '');

describe('image geometry overlays', () => {
  it('keeps the crop guide transparent without removing its outline', () => {
    const rule = sheet.match(/\.geometry-overlay__guide rect[^{}]*\{([^}]+)\}/)?.[1];
    expect(rule).toBeDefined();
    expect(rule).toMatch(/fill:\s*none\s*;/);
    expect(rule).toMatch(/stroke:\s*var\(--wax-white\)/);
  });
});

describe('styles.css legibility floor', () => {
  it('never renders type below the mono-chrome floor', () => {
    const offenders = [...sheet.matchAll(/font-size:\s*([\d.]+)px/g)]
      .map((match) => ({
        size: Number(match[1]),
        line: sheet.slice(0, match.index).split('\n').length,
      }))
      .filter((entry) => entry.size < FLOOR_PX)
      .map((entry) => `line ${entry.line}: ${entry.size}px`);

    expect(offenders).toEqual([]);
  });
});

type Cast = 'room' | 'bench' | 'plane' | 'judge';

/**
 * Which cast a declaration belongs to. The one block that needs splitting is the
 * root: it carries the room's palette, the bench's default lamp-on values and the
 * judge station's neutrals, so its tokens are attributed by name there.
 */
const CASTS: Array<{ selector: RegExp; cast: Cast }> = [
  { selector: /^\.canvas-panel,\s*\.browser-panel$/, cast: 'bench' },
  { selector: /^\.app-shell\[data-lamp='off'\] \.canvas-panel/, cast: 'plane' },
  { selector: /^\.browser-panel$/, cast: 'bench' },
  { selector: /^\.viewer-section,/, cast: 'judge' },
];

function castFor(selector: string, token: string): Cast | null {
  if (selector.trim() === ':root') {
    if (token.startsWith('--room-')) return 'room';
    if (token.startsWith('--bench-')) return 'bench';
    if (token.startsWith('--judge-')) return 'judge';
    if (token.startsWith('--wax-')) return 'room';
    return null;
  }
  for (const entry of CASTS) if (entry.selector.test(selector.trim())) return entry.cast;
  return null;
}

interface Declaration {
  token: string;
  hex: string;
  cast: Cast;
}

const declarations: Declaration[] = [];
for (const block of sheet.matchAll(/([^{}@]+)\{([^{}]*)\}/g)) {
  const selector = block[1];
  for (const declaration of block[2].matchAll(/(--[a-z0-9-]+):\s*(#[0-9a-fA-F]{6});/g)) {
    const cast = castFor(selector, declaration[1]);
    if (cast) declarations.push({ token: declaration[1], hex: declaration[2].toLowerCase(), cast });
  }
}

/** Surfaces are what text can sit on: grounds, panels, raises, tints. A line is
 *  never a surface, and neither is the light box's frame — which is why border
 *  and mount colours are not measured as text. */
const SURFACE = /-(ground|sunk|strip|panel|raise|hover|select|tint|spill)$/;
/** Inks are the tokens that carry type, plus the three wax marks the app also
 *  prints with. Red is absent on purpose: its text role is `--wax-red-ink`,
 *  because a wax dark enough to be a mark is too dark to be legible type. */
const INK = /-ink(-body|-dim|-faint|-soft)?$/;
const PRINTING_WAX = ['--wax-white', '--wax-amber', '--wax-blue'];

/** Ink set on its own solid wax fill, a large placeholder glyph, and a dismiss
 *  glyph on a control. */
const CONTRAST_EXEMPT: Record<string, string> = {
  '--wax-white-ink': 'text set ON a solid wax fill, not on a cast surface',
  '--room-ink-faint': 'large 22px placeholder glyph: WCAG 1.4.3 exempts large text at 3:1',
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

const surfacesByCast = new Map<Cast, string[]>();
for (const declaration of declarations) {
  if (!SURFACE.test(declaration.token)) continue;
  const list = surfacesByCast.get(declaration.cast) ?? [];
  list.push(declaration.hex);
  surfacesByCast.set(declaration.cast, list);
}

describe('styles.css colour discipline', () => {
  it('declares every colour it uses, so nothing can escape its cast', () => {
    const declared = new Set(declarations.map((declaration) => declaration.hex));
    const loose = [...new Set([...sheet.matchAll(/#[0-9a-fA-F]{6}/g)].map((match) => match[0].toLowerCase()))]
      .filter((hex) => !declared.has(hex));
    expect(loose).toEqual([]);
  });

  it('covers every cast with surfaces to measure against', () => {
    expect([...surfacesByCast.keys()].sort()).toEqual(['bench', 'judge', 'plane', 'room']);
  });

  it('keeps every ink at AA on the worst surface of its own cast', () => {
    const offenders: string[] = [];
    for (const declaration of declarations) {
      const { token, hex, cast } = declaration;
      if (token in CONTRAST_EXEMPT) continue;
      if (!INK.test(token) && !PRINTING_WAX.includes(token)) continue;
      const surfaces = surfacesByCast.get(cast) ?? [];
      expect(surfaces.length).toBeGreaterThan(0);
      const worst = surfaces.reduce((a, b) => (ratio(hex, a) <= ratio(hex, b) ? a : b));
      const measured = ratio(hex, worst);
      if (measured < 4.5) offenders.push(`${token} ${hex} at ${measured.toFixed(2)}:1 on ${worst} (${cast})`);
    }
    expect([...new Set(offenders)]).toEqual([]);
  });
});
