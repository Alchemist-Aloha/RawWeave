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
