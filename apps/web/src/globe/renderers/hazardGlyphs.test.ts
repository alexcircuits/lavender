/**
 * What the hazard glyphs actually draw, read back pixel by pixel from a rasterising test
 * canvas: every symbol keeps the two-tone hazard edge (ramp colour, then the light keyline,
 * then the `--map-outline`, outwards), which is what keeps every ramp step at 3:1 or more over
 * any imagery (hazardShapes.test.ts), and open parts stay fully transparent.
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { installRasterCanvas } from '../../test/rasterCanvas';
import { DEFAULT_PALETTE } from '../tokens';
import { GLYPH_SUPERSAMPLE } from './glyphs';
import {
  drawCycloneSpiral,
  drawFireCell,
  drawPickWash,
  drawQuakeRing,
  type HazardInk,
  hazardInk,
  PICK_ALPHA,
} from './hazardGlyphs';
import {
  cycloneSpiralPx,
  KEYLINE_PX,
  OUTLINE_PX,
  QUAKE_RING_STROKE_PX,
  type RampStep,
} from './hazardShapes';
import { toMapColors } from './types';

const colors = toMapColors(DEFAULT_PALETTE);
const S = GLYPH_SUPERSAMPLE;
const STEPS: readonly RampStep[] = [0, 1, 2, 3, 4];

type Tone = 'fill' | 'keyline' | 'outline' | 'clear' | 'other';

let raster: ReturnType<typeof installRasterCanvas>;
beforeEach(() => {
  raster = installRasterCanvas();
});
afterEach(() => raster.restore());

const hex = ([r, g, b]: readonly number[]) =>
  `#${[r, g, b]
    .map((c) =>
      Math.round(c ?? 0)
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')}`;

/** Each pixel of a glyph as the ink it carries. */
function tones(canvas: HTMLCanvasElement, ink: HazardInk): Tone[][] {
  const context = raster.contextOf(canvas);
  return Array.from({ length: canvas.height }, (_, y) =>
    Array.from({ length: canvas.width }, (_, x): Tone => {
      const [r, g, b, a] = context.pixel(x, y);
      if (a === 0) return 'clear';
      if (a !== 1) return 'other';
      const colour = hex([r, g, b]);
      if (colour === ink.fill) return 'fill';
      if (colour === ink.keyline) return 'keyline';
      if (colour === ink.outline) return 'outline';
      return 'other';
    }),
  );
}

/** Clear pixels connected to the glyph's border: the imagery around the symbol. */
function outsideOf(grid: Tone[][]): boolean[][] {
  const outside = grid.map((row) => row.map(() => false));
  const queue: [number, number][] = [];
  const height = grid.length;
  const width = grid[0]?.length ?? 0;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      if (x === 0 || y === 0 || x === width - 1 || y === height - 1) queue.push([x, y]);
    }
  }
  while (queue.length > 0) {
    const [x, y] = queue.pop() ?? [0, 0];
    const row = outside[y];
    if (!row || row[x] || grid[y]?.[x] !== 'clear') continue;
    row[x] = true;
    queue.push([x + 1, y], [x - 1, y], [x, y + 1], [x, y - 1]);
  }
  return outside;
}

/**
 * Asserts the two-tone hazard edge everywhere: the ramp colour only meets itself, the keyline
 * or an open centre; the keyline only meets the colour, itself or the outline; only the outline
 * meets the imagery. Returns how many enclosed (open-centre) pixels the glyph has.
 */
function expectTwoToneEdge(grid: Tone[][], label: string): number {
  const outside = outsideOf(grid);
  const allowed: Partial<Record<Tone, ReadonlySet<string>>> = {
    fill: new Set(['fill', 'keyline', 'hole']),
    keyline: new Set(['fill', 'keyline', 'outline']),
  };
  const seen = new Set<Tone>();
  let holes = 0;
  const kindAt = (x: number, y: number) => {
    const tone = grid[y]?.[x];
    if (tone === undefined) return 'outside';
    if (tone === 'clear') return outside[y]?.[x] ? 'outside' : 'hole';
    return tone;
  };
  grid.forEach((row, y) => {
    row.forEach((tone, x) => {
      seen.add(tone);
      if (kindAt(x, y) === 'hole') holes += 1;
      const ok = allowed[tone];
      if (!ok) return;
      for (const [nx, ny] of [
        [x + 1, y],
        [x - 1, y],
        [x, y + 1],
        [x, y - 1],
      ] as const) {
        const neighbour = kindAt(nx, ny);
        if (!ok.has(neighbour)) {
          throw new Error(`${label}: ${tone} at (${x}, ${y}) meets ${neighbour}`);
        }
      }
    });
  });
  expect(
    [...seen].filter((tone) => tone === 'other'),
    label,
  ).toEqual([]);
  for (const tone of ['fill', 'keyline', 'outline'] as const) {
    expect(seen.has(tone), `${label} draws ${tone}`).toBe(true);
  }
  return holes;
}

/** Runs of equal tones along row `y`, from column `from` to the right edge. */
function runs(grid: Tone[][], y: number, from = 0): [Tone, number][] {
  const out: [Tone, number][] = [];
  for (const tone of (grid[y] ?? []).slice(from)) {
    const last = out.at(-1);
    if (last && last[0] === tone) last[1] += 1;
    else out.push([tone, 1]);
  }
  return out;
}

const rampInk = (step: RampStep) => hazardInk(colors, colors.hazardRamp[step]);

describe('quake ring glyph', () => {
  it('draws a ramp ring inside the keyline and outline with a fully open centre', () => {
    for (const ringPx of [6, 9, 14, 24]) {
      for (const step of STEPS) {
        const ink = rampInk(step);
        const grid = tones(drawQuakeRing(ringPx, ink), ink);
        const holes = expectTwoToneEdge(grid, `ring ${ringPx} px, step ${step}`);
        expect(holes, `ring ${ringPx} px is open`).toBeGreaterThan(0);
      }
    }
  });

  it('layers 2 px of ramp colour, 1 px keyline and 1 px outline outwards from the open centre', () => {
    const ink = rampInk(0);
    const grid = tones(drawQuakeRing(14, ink), ink);
    const centre = grid.length / 2;
    expect(runs(grid, centre, centre)).toEqual([
      ['clear', (14 / 2 - QUAKE_RING_STROKE_PX) * S],
      ['fill', QUAKE_RING_STROKE_PX * S],
      ['keyline', KEYLINE_PX * S],
      ['outline', OUTLINE_PX * S],
    ]);
  });
});

describe('fire cell glyph', () => {
  it('draws a filled ramp square inside the keyline and outline', () => {
    for (const sidePx of [4, 9, 16]) {
      for (const step of STEPS) {
        const ink = rampInk(step);
        const grid = tones(drawFireCell(sidePx, ink), ink);
        expect(expectTwoToneEdge(grid, `cell ${sidePx} px, step ${step}`)).toBe(0);
      }
    }
    const ink = rampInk(4);
    const grid = tones(drawFireCell(9, ink), ink);
    expect(runs(grid, grid.length / 2)).toEqual([
      ['outline', OUTLINE_PX * S],
      ['keyline', KEYLINE_PX * S],
      ['fill', 9 * S],
      ['keyline', KEYLINE_PX * S],
      ['outline', OUTLINE_PX * S],
    ]);
  });
});

describe('cyclone spiral glyph', () => {
  it('edges the eye and both arms, for every wind step, unknown wind and both hemispheres', () => {
    const variants = [
      ...STEPS.map((step) => ({ step, ink: rampInk(step) })),
      { step: null, ink: hazardInk(colors, colors.space) },
    ];
    for (const { step, ink } of variants) {
      for (const hemisphere of ['north', 'south'] as const) {
        const canvas = drawCycloneSpiral(cycloneSpiralPx(step), ink, hemisphere);
        const grid = tones(canvas, ink);
        expectTwoToneEdge(grid, `spiral ${step ?? 'unknown'} ${hemisphere}`);
        const centre = canvas.width / 2;
        expect(grid[centre]?.[centre], 'the eye is filled').toBe('fill');
      }
    }
  });
});

describe('pick wash glyph', () => {
  it('is a near-invisible disc over the whole glyph, in the given colour', () => {
    const canvas = drawPickWash(18, DEFAULT_PALETTE.outline);
    const context = raster.contextOf(canvas);
    expect(canvas.width).toBe(18 * S);
    const [r, g, b, a] = context.pixel(18, 18);
    expect(hex([r, g, b])).toBe(DEFAULT_PALETTE.outline.toLowerCase());
    expect(a).toBeCloseTo(PICK_ALPHA, 9);
    // Fully transparent outside the disc (corners), so nothing outside the footprint picks.
    expect(context.pixel(0, 0)[3]).toBe(0);
    // Pickable even when stale (Cesium discards only alpha 0): 5/255, 3/255 at 60 %.
    expect(Math.round(PICK_ALPHA * 255 * 0.6)).toBeGreaterThanOrEqual(1);
  });
});
