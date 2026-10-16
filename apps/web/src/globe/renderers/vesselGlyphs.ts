/**
 * Runtime-drawn vessel symbols (design-system §4, Maritime): a hull chevron, bow up, for a
 * ship making way and a dot for one at rest. Like the other glyphs they are drawn at
 * `GLYPH_SUPERSAMPLE`× in white and tinted by the billboard colour, with a 1 px
 * `--map-outline` border that stays dark under any tint or the stale alpha.
 *
 * The chevron is a pointed bow with a notched stern, so it reads as a direction even at
 * 16 px and never as the aircraft silhouette that shares the globe.
 */
import { GLYPH_SUPERSAMPLE } from './glyphs';
import type { VesselGlyphKind } from './vesselSymbology';

/** On-screen side of the (square) chevron canvas, in CSS pixels; the hull is 16 px long. */
export const VESSEL_CHEVRON_PX = 18;
/** On-screen diameter of the at-rest dot, in CSS pixels, outline included. */
export const VESSEL_DOT_PX = 9;

/** Hull chevron, bow up, on an 18-unit grid centred on the canvas. */
const HULL: readonly (readonly [number, number])[] = [
  [9, 1],
  [13.5, 8],
  [13.5, 16.5],
  [9, 13.5],
  [4.5, 16.5],
  [4.5, 8],
];

/** The billboard image id for a vessel glyph (one shared atlas entry per kind). */
export function vesselGlyphId(kind: VesselGlyphKind): string {
  return `lv-vessel-${kind}`;
}

/** The on-screen size of a vessel glyph, for the selection halo. */
export function vesselGlyphPx(kind: VesselGlyphKind): number {
  return kind === 'underway' ? VESSEL_CHEVRON_PX : VESSEL_DOT_PX;
}

function canvasOf(sizePx: number): {
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D | null;
} {
  const canvas = document.createElement('canvas');
  const side = Math.ceil(sizePx * GLYPH_SUPERSAMPLE);
  canvas.width = side;
  canvas.height = side;
  return { canvas, ctx: canvas.getContext('2d') };
}

export function drawVesselGlyph(kind: VesselGlyphKind, outline: string): HTMLCanvasElement {
  const s = GLYPH_SUPERSAMPLE;
  if (kind === 'at-rest') {
    const { canvas, ctx } = canvasOf(VESSEL_DOT_PX);
    if (!ctx) return canvas;
    const centre = canvas.width / 2;
    ctx.beginPath();
    ctx.arc(centre, centre, centre, 0, Math.PI * 2);
    ctx.fillStyle = outline;
    ctx.fill();
    ctx.beginPath();
    ctx.arc(centre, centre, centre - 1 * s, 0, Math.PI * 2);
    ctx.fillStyle = '#ffffff';
    ctx.fill();
    return canvas;
  }
  const { canvas, ctx } = canvasOf(VESSEL_CHEVRON_PX);
  if (!ctx) return canvas;
  ctx.beginPath();
  HULL.forEach(([x, y], i) => {
    if (i === 0) ctx.moveTo(x * s, y * s);
    else ctx.lineTo(x * s, y * s);
  });
  ctx.closePath();
  ctx.lineJoin = 'round';
  ctx.strokeStyle = outline;
  ctx.lineWidth = 2 * s; // 1 px shows outside the fill
  ctx.stroke();
  ctx.fillStyle = '#ffffff';
  ctx.fill();
  return canvas;
}
