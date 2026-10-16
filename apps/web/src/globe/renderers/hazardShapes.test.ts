import { describe, expect, it } from 'vitest';
import { DEFAULT_PALETTE } from '../tokens';
import {
  cycloneGlyphId,
  cycloneGlyphPx,
  fireCellGlyphId,
  hazardInk,
  quakeRingGlyphId,
  quakeRingGlyphPx,
} from './hazardGlyphs';
import {
  CYCLONE_STROKE_PX,
  cycloneSpiral,
  cycloneSpiralPx,
  HAZARD_EDGE_PX,
  hemisphereOf,
  quakeRingPx,
} from './hazardShapes';
import { QUAKE_MAX_PX, QUAKE_MIN_PX } from './symbology';
import { toMapColors } from './types';

/** WCAG 2.x relative luminance and contrast ratio. */
function luminance(hex: string): number {
  const n = Number.parseInt(hex.slice(1, 7), 16);
  const channel = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(n >> 16) + 0.7152 * channel((n >> 8) & 255) + 0.0722 * channel(n & 255);
}
const contrast = (a: number, b: number) => (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);

describe('hazard edge contrast', () => {
  it('reaches 3:1 against any imagery colour, black to white, for every ramp step', () => {
    const keyline = luminance(DEFAULT_PALETTE.keyline);
    const outline = luminance(DEFAULT_PALETTE.outline);
    // The worst background sits where the keyline and outline contrast cross.
    const worst = Math.sqrt((keyline + 0.05) / (outline + 0.05));
    expect(worst).toBeGreaterThanOrEqual(3);
    for (let step = 0; step <= 1000; step += 1) {
      const background = step / 1000;
      const best = Math.max(contrast(keyline, background), contrast(outline, background));
      expect(best, `background luminance ${background}`).toBeGreaterThanOrEqual(3);
    }
  });

  it('fixes what the ground outline alone could not: the darkest step on a black sea', () => {
    // VIIRS Black Marble 2016 open sea and Blue Marble deep ocean measure L ≈ 0.0017.
    const sea = 0.0017;
    const darkest = luminance(DEFAULT_PALETTE.hazardRamp[0]);
    expect(contrast(darkest, sea)).toBeLessThan(3);
    expect(contrast(luminance(DEFAULT_PALETTE.outline), sea)).toBeLessThan(3);
    expect(contrast(luminance(DEFAULT_PALETTE.keyline), sea)).toBeGreaterThan(9);
  });
});

describe('hazard shapes', () => {
  it('sizes quake rings by magnitude in whole pixels', () => {
    expect(quakeRingPx(2)).toBe(9);
    expect(quakeRingPx(4.2)).toBe(15);
    expect(quakeRingPx(-1)).toBe(QUAKE_MIN_PX);
    expect(quakeRingPx(9.5)).toBe(QUAKE_MAX_PX);
    expect(quakeRingGlyphPx(9)).toBe(9 + 2 * HAZARD_EDGE_PX);
    expect(quakeRingGlyphId(9.4, 1)).toBe('lv-quake-ring-9-1');
  });

  it('grows the cyclone spiral with wind and keeps unknown wind distinct', () => {
    expect([null, 0, 1, 2, 3, 4].map((step) => cycloneSpiralPx(step as never))).toEqual([
      18, 18, 20, 22, 24, 26,
    ]);
    expect(cycloneGlyphPx(22)).toBe(22 + CYCLONE_STROKE_PX + 2 * HAZARD_EDGE_PX);
    expect(cycloneGlyphId(18, 'unknown', 'north')).toBe('lv-cyclone-18-unknown-north');
    expect(cycloneGlyphId(22, 2, 'south')).toBe('lv-cyclone-22-2-south');
    expect(fireCellGlyphId(12, 4)).not.toBe(cycloneGlyphId(12, 4, 'north'));
  });

  it('draws the spiral within its reach, mirrored south of the equator', () => {
    const north = cycloneSpiral('north');
    const south = cycloneSpiral('south');
    expect(north.arms).toHaveLength(2);
    for (const arm of north.arms) {
      const radii = arm.map(([x, y]) => Math.hypot(x, y));
      expect(radii[0]).toBeCloseTo(north.eyeRadius, 9);
      expect(radii.at(-1)).toBeCloseTo(0.5, 9);
      // Every point lies further out than the one before: an outward spiral.
      for (let i = 1; i < radii.length; i += 1) {
        expect(radii[i]).toBeGreaterThan(radii[i - 1] ?? 0);
      }
    }
    // The southern glyph is the northern one reflected about the vertical axis.
    const flat = (shape: typeof north) => shape.arms.flat();
    const southPoints = flat(south);
    for (const [i, [x, y]] of flat(north).entries()) {
      expect(southPoints[i]?.[0]).toBeCloseTo(-x, 9);
      expect(southPoints[i]?.[1]).toBeCloseTo(y, 9);
    }
    // Arms trail the rotation: traced outwards they turn clockwise on screen north of the
    // equator (storms there turn counter-clockwise), counter-clockwise south of it.
    const outwardTurn = (shape: typeof north) =>
      shape.arms.map((arm) =>
        Math.sign(
          arm.slice(1).reduce((sum, [x, y], i) => {
            const [px = 0, py = 0] = arm[i] ?? [];
            // Cross product with y flipped up: positive is counter-clockwise on screen.
            return sum + (px * -y - -py * x);
          }, 0),
        ),
      );
    expect(outwardTurn(north)).toEqual([-1, -1]);
    expect(outwardTurn(south)).toEqual([1, 1]);
    expect(hemisphereOf(20)).toBe('north');
    expect(hemisphereOf(0)).toBe('north');
    expect(hemisphereOf(-0.1)).toBe('south');
  });

  it('inks glyphs with the ramp colour, the light keyline and the ground outline', () => {
    const colors = toMapColors(DEFAULT_PALETTE);
    expect(hazardInk(colors, colors.hazardRamp[0])).toEqual({
      fill: DEFAULT_PALETTE.hazardRamp[0],
      keyline: DEFAULT_PALETTE.keyline,
      outline: DEFAULT_PALETTE.outline,
    });
  });
});
