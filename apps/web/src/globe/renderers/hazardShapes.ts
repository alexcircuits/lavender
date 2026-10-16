/**
 * Geometry of the three hazard symbols (docs/design/design-system.md §4): one colour family,
 * told apart by shape. Earthquake = ring, fire = filled cell, cyclone = spiral. Pure numbers
 * (no Cesium, no DOM), shared by the globe's canvas glyphs and the layer legend's SVG swatches
 * so the legend always shows the shape the globe draws. Lengths are CSS pixels.
 */
import { quakeSizePx } from './symbology';

/** A step of the five-colour hazard ramp, low → high. */
export type RampStep = 0 | 1 | 2 | 3 | 4;

/**
 * The hazard edge round every symbol: a 1 px light `--map-keyline` just inside a 1 px
 * `--map-outline`. Over dark imagery the keyline separates the symbol, over bright imagery
 * the outline does, so every ramp step reads at 3:1 or more whatever is underneath.
 */
export const KEYLINE_PX = 1;
export const OUTLINE_PX = 1;
export const HAZARD_EDGE_PX = KEYLINE_PX + OUTLINE_PX;

/** Stroke of an earthquake ring. */
export const QUAKE_RING_STROKE_PX = 2;

/** Outer diameter of an earthquake's ring: the magnitude size in whole pixels (6–24). */
export function quakeRingPx(magnitude: number): number {
  return Math.round(quakeSizePx(magnitude));
}

/** Stroke of a cyclone spiral's arms. */
export const CYCLONE_STROKE_PX = 2;

/** Reach of a cyclone spiral by wind step (18–26 px); unknown wind draws at the smallest. */
export function cycloneSpiralPx(step: RampStep | null): number {
  return 18 + 2 * (step ?? 0);
}

/** Tropical cyclones turn counter-clockwise north of the equator and clockwise south of it. */
export type Hemisphere = 'north' | 'south';

export function hemisphereOf(lat: number): Hemisphere {
  return lat < 0 ? 'south' : 'north';
}

export type Point = readonly [x: number, y: number];

export interface SpiralShape {
  /** Radius of the filled eye. */
  readonly eyeRadius: number;
  /** Two arms as polylines, from the eye outwards. */
  readonly arms: readonly (readonly Point[])[];
}

const EYE_RADIUS = 0.17;
const ARM_SWEEP_RAD = (150 * Math.PI) / 180;
const ARM_SEGMENTS = 12;

/**
 * The cyclone spiral on a unit symbol centred on the storm (reach 0.5, y down as on screen):
 * a filled eye and two arms that leave it from opposite sides and sweep 150° outwards. Like a
 * storm's rain bands they trail its rotation: air spirals inwards counter-clockwise north of
 * the equator, so going outwards the arms turn clockwise there, and the glyph is mirrored
 * south of the equator.
 */
export function cycloneSpiral(hemisphere: Hemisphere): SpiralShape {
  // Angles grow counter-clockwise on screen; outwards, northern arms turn the other way.
  const turn = hemisphere === 'north' ? -1 : 1;
  const arms = [Math.PI / 2, (3 * Math.PI) / 2].map((start) =>
    Array.from({ length: ARM_SEGMENTS + 1 }, (_, i): Point => {
      const t = i / ARM_SEGMENTS;
      const angle = start + turn * t * ARM_SWEEP_RAD;
      const radius = EYE_RADIUS + t * (0.5 - EYE_RADIUS);
      // y grows downwards on screen, so a growing angle turns counter-clockwise.
      return [radius * Math.cos(angle), -radius * Math.sin(angle)];
    }),
  );
  return { eyeRadius: EYE_RADIUS, arms };
}
