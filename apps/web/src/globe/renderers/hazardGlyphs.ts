/**
 * Runtime-drawn hazard glyphs (no image files): the earthquake ring, the fire cell and the
 * cyclone spiral of hazardShapes.ts, each inside the two-tone hazard edge (1 px light keyline,
 * 1 px `--map-outline`). They are drawn in their final colours, one atlas entry per shape,
 * size and ramp step: a billboard tint would also tint the light keyline. Billboards only take
 * the stale alpha.
 *
 * Fully transparent texels are not pickable in Cesium, so the open parts of a symbol (inside a
 * ring, between spiral arms) would not select it. Each symbol therefore gets a separate
 * near-invisible `PICK_ALPHA` wash disc over its footprint, in its layer's pick assists
 * (types.ts `PickAssist`): a click lands on the wash only when no drawn symbol is under the
 * cursor, so a wash never hides the ring drawn through its open centre.
 */
import type { Color } from '@cesium/core';
import { GLYPH_SUPERSAMPLE } from './glyphs';
import {
  CYCLONE_STROKE_PX,
  cycloneSpiral,
  HAZARD_EDGE_PX,
  type Hemisphere,
  KEYLINE_PX,
  OUTLINE_PX,
  QUAKE_RING_STROKE_PX,
  type RampStep,
} from './hazardShapes';
import type { MapColors } from './types';

/** Alpha of the wash that keeps open parts of a symbol pickable (5/255; 3/255 when stale). */
export const PICK_ALPHA = 0.02;

/** The three colours of a hazard glyph, as CSS colour strings. */
export interface HazardInk {
  readonly fill: string;
  readonly keyline: string;
  readonly outline: string;
}

/** The ink for a symbol of `fill` colour (a ramp step, or the neutral unknown-wind colour). */
export function hazardInk(colors: MapColors, fill: Color): HazardInk {
  return {
    fill: fill.toCssHexString(),
    keyline: colors.keyline.toCssHexString(),
    outline: colors.outline.toCssHexString(),
  };
}

/** One canvas per glyph id, drawn on first use (the billboard atlas keeps one entry per id). */
export class GlyphCache {
  readonly #images = new Map<string, HTMLCanvasElement>();

  get(id: string, draw: () => HTMLCanvasElement): HTMLCanvasElement {
    let image = this.#images.get(id);
    if (!image) {
      image = draw();
      this.#images.set(id, image);
    }
    return image;
  }
}

type Context = CanvasRenderingContext2D;

function canvasOf(sidePx: number): { canvas: HTMLCanvasElement; ctx: Context | null } {
  const canvas = document.createElement('canvas');
  const side = Math.ceil(sidePx * GLYPH_SUPERSAMPLE);
  canvas.width = side;
  canvas.height = side;
  return { canvas, ctx: canvas.getContext('2d') };
}

/** A filled disc of `radiusPx` (CSS pixels) at the canvas centre. */
function disc(ctx: Context, radiusPx: number, style: string): void {
  const centre = ctx.canvas.width / 2;
  ctx.beginPath();
  ctx.arc(centre, centre, Math.max(0, radiusPx) * GLYPH_SUPERSAMPLE, 0, Math.PI * 2);
  ctx.fillStyle = style;
  ctx.fill();
}

// ── Earthquake ring ─────────────────────────────────────────────────────────────────────────

/** Side of a ring glyph: the ring plus the hazard edge outside it. */
export function quakeRingGlyphPx(ringPx: number): number {
  return Math.round(ringPx) + 2 * HAZARD_EDGE_PX;
}

export function quakeRingGlyphId(ringPx: number, step: RampStep): string {
  return `lv-quake-ring-${Math.round(ringPx)}-${step}`;
}

/**
 * An earthquake as a ring of `ringPx` outer diameter: a 2 px stroke in the ramp colour with
 * the hazard edge outside it and a fully open centre, so overlapping events stay legible (the
 * centre picks through the layer's wash, `drawPickWash`).
 */
export function drawQuakeRing(ringPx: number, ink: HazardInk): HTMLCanvasElement {
  const { canvas, ctx } = canvasOf(quakeRingGlyphPx(ringPx));
  if (!ctx) return canvas;
  const radius = Math.round(ringPx) / 2;
  const hole = radius - QUAKE_RING_STROKE_PX;
  disc(ctx, radius + HAZARD_EDGE_PX, ink.outline);
  disc(ctx, radius + KEYLINE_PX, ink.keyline);
  disc(ctx, radius, ink.fill);
  ctx.globalCompositeOperation = 'destination-out';
  disc(ctx, hole, '#000000');
  ctx.globalCompositeOperation = 'source-over';
  return canvas;
}

// ── Fire cell ───────────────────────────────────────────────────────────────────────────────

/** Side of a fire glyph: the filled square plus the hazard edge round it. */
export function fireCellGlyphPx(sidePx: number): number {
  return Math.round(sidePx) + 2 * HAZARD_EDGE_PX;
}

export function fireCellGlyphId(sidePx: number, step: RampStep): string {
  return `lv-fire-cell-${Math.round(sidePx)}-${step}`;
}

/** A fire as a filled square cell of `sidePx` (whole pixels) inside the hazard edge. */
export function drawFireCell(sidePx: number, ink: HazardInk): HTMLCanvasElement {
  const { canvas, ctx } = canvasOf(fireCellGlyphPx(sidePx));
  if (!ctx) return canvas;
  const s = GLYPH_SUPERSAMPLE;
  const square = (insetPx: number, style: string) => {
    ctx.fillStyle = style;
    ctx.fillRect(
      insetPx * s,
      insetPx * s,
      canvas.width - 2 * insetPx * s,
      canvas.height - 2 * insetPx * s,
    );
  };
  square(0, ink.outline);
  square(OUTLINE_PX, ink.keyline);
  square(HAZARD_EDGE_PX, ink.fill);
  return canvas;
}

// ── Cyclone spiral ──────────────────────────────────────────────────────────────────────────

/** Side of a spiral glyph: the spiral's reach, its stroke and the hazard edge. */
export function cycloneGlyphPx(spiralPx: number): number {
  return Math.round(spiralPx) + CYCLONE_STROKE_PX + 2 * HAZARD_EDGE_PX;
}

export function cycloneGlyphId(
  spiralPx: number,
  step: RampStep | 'unknown',
  hemisphere: Hemisphere,
): string {
  return `lv-cyclone-${Math.round(spiralPx)}-${step}-${hemisphere}`;
}

/**
 * A cyclone position as a spiral of `spiralPx` reach: a filled eye and two arms trailing the
 * storm's rotation (hazardShapes.ts `cycloneSpiral`). Drawn three times, widest first (outline, keyline, colour), so the
 * hazard edge runs round the eye and both arms as one shape.
 */
export function drawCycloneSpiral(
  spiralPx: number,
  ink: HazardInk,
  hemisphere: Hemisphere,
): HTMLCanvasElement {
  const { canvas, ctx } = canvasOf(cycloneGlyphPx(spiralPx));
  if (!ctx) return canvas;
  const reach = Math.round(spiralPx);
  const shape = cycloneSpiral(hemisphere);
  const s = GLYPH_SUPERSAMPLE;
  const centre = canvas.width / 2;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  const layer = (style: string, growPx: number) => {
    ctx.strokeStyle = style;
    ctx.lineWidth = (CYCLONE_STROKE_PX + 2 * growPx) * s;
    for (const arm of shape.arms) {
      ctx.beginPath();
      arm.forEach(([x, y], i) => {
        const px = centre + x * reach * s;
        const py = centre + y * reach * s;
        if (i === 0) ctx.moveTo(px, py);
        else ctx.lineTo(px, py);
      });
      ctx.stroke();
    }
    disc(ctx, shape.eyeRadius * reach + growPx, style);
  };
  layer(ink.outline, HAZARD_EDGE_PX);
  layer(ink.keyline, KEYLINE_PX);
  layer(ink.fill, 0);
  return canvas;
}

// ── Pick wash ───────────────────────────────────────────────────────────────────────────────

export function pickWashGlyphId(sidePx: number): string {
  return `lv-pick-wash-${Math.round(sidePx)}`;
}

/**
 * A near-invisible `PICK_ALPHA` disc of `sidePx` diameter (a symbol's whole glyph) in `style`,
 * the hit area of a symbol's open parts. It goes in a layer's pick assists, never in the
 * symbol's own glyph.
 */
export function drawPickWash(sidePx: number, style: string): HTMLCanvasElement {
  const { canvas, ctx } = canvasOf(sidePx);
  if (!ctx) return canvas;
  ctx.globalAlpha = PICK_ALPHA;
  disc(ctx, Math.round(sidePx) / 2, style);
  ctx.globalAlpha = 1;
  return canvas;
}
