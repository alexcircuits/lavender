/**
 * Runtime-drawn symbols (no image files): aircraft silhouettes and the selection halo. Glyphs
 * are drawn at 2× in white and tinted by the billboard colour, so one texture serves live and
 * stale states; the 1 px `--map-outline` stays dark under any tint (design-system §4).
 *
 * Civil aircraft are a filled airliner; military contacts a hollow delta-wing jet, so identity
 * is carried by shape and fill, never by colour alone.
 */

/** Canvas pixels per screen pixel; billboards are drawn at `1 / GLYPH_SUPERSAMPLE` scale. */
export const GLYPH_SUPERSAMPLE = 2;
/** On-screen size of an aircraft glyph, in CSS pixels. */
export const AIRCRAFT_GLYPH_PX = 22;

export type AircraftGlyphKind = 'civil' | 'military';

type Outline = readonly (readonly [number, number])[];

/** Airliner, nose up, on a 24-unit grid. */
const AIRLINER: Outline = [
  [12, 1],
  [13.4, 2.6],
  [13.6, 9],
  [23, 14.5],
  [23, 16.5],
  [13.6, 13.6],
  [13.4, 19.2],
  [16.6, 21.6],
  [16.6, 23],
  [12, 21.8],
  [7.4, 23],
  [7.4, 21.6],
  [10.6, 19.2],
  [10.4, 13.6],
  [1, 16.5],
  [1, 14.5],
  [10.4, 9],
  [10.6, 2.6],
];

/** Delta-wing military jet, nose up, on a 24-unit grid. */
const DELTA_JET: Outline = [
  [12, 1],
  [13.8, 6.5],
  [13.8, 10],
  [22, 18],
  [22, 20],
  [13.8, 17],
  [13.6, 19.5],
  [16.8, 22],
  [16.8, 23],
  [12, 21.6],
  [7.2, 23],
  [7.2, 22],
  [10.4, 19.5],
  [10.2, 17],
  [2, 20],
  [2, 18],
  [10.2, 10],
  [10.2, 6.5],
];

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

function tracePath(ctx: CanvasRenderingContext2D, outline: Outline, scale: number, offset: number) {
  ctx.beginPath();
  outline.forEach(([x, y], i) => {
    const px = offset + x * scale;
    const py = offset + y * scale;
    if (i === 0) ctx.moveTo(px, py);
    else ctx.lineTo(px, py);
  });
  ctx.closePath();
}

/** The billboard image id for a glyph (shared atlas entry per kind). */
export function aircraftGlyphId(kind: AircraftGlyphKind): string {
  return `lv-aircraft-${kind}`;
}

export function drawAircraftGlyph(kind: AircraftGlyphKind, outline: string): HTMLCanvasElement {
  const { canvas, ctx } = canvasOf(AIRCRAFT_GLYPH_PX);
  if (!ctx) return canvas;
  const unit = (AIRCRAFT_GLYPH_PX * GLYPH_SUPERSAMPLE) / 26; // 24-unit grid plus a 1-unit margin
  const offset = unit;
  ctx.lineJoin = 'round';
  if (kind === 'civil') {
    tracePath(ctx, AIRLINER, unit, offset);
    ctx.strokeStyle = outline;
    ctx.lineWidth = 2 * GLYPH_SUPERSAMPLE; // 1 px shows outside the fill
    ctx.stroke();
    ctx.fillStyle = '#ffffff';
    ctx.fill();
    return canvas;
  }
  tracePath(ctx, DELTA_JET, unit, offset);
  ctx.strokeStyle = outline;
  ctx.lineWidth = 3.5 * GLYPH_SUPERSAMPLE;
  ctx.stroke();
  ctx.fillStyle = outline;
  ctx.fill();
  ctx.strokeStyle = '#ffffff';
  ctx.lineWidth = 1.5 * GLYPH_SUPERSAMPLE;
  ctx.stroke();
  return canvas;
}

/** Halo ring geometry around a symbol of `symbolPx` diameter (CSS pixels). */
export function haloSizePx(symbolPx: number): number {
  return Math.ceil(symbolPx) + 12;
}

export function haloId(symbolPx: number): string {
  return `lv-halo-${Math.ceil(symbolPx)}`;
}

/**
 * The selection halo: a 2 px `--map-selection` ring with a 1 px `--map-outline` ring on each
 * side (separation from the symbol inside, from bright imagery outside). Drawn in its final
 * colours; the billboard is not tinted.
 */
export function drawHalo(symbolPx: number, selection: string, outline: string): HTMLCanvasElement {
  const size = haloSizePx(symbolPx);
  const { canvas, ctx } = canvasOf(size);
  if (!ctx) return canvas;
  const s = GLYPH_SUPERSAMPLE;
  const centre = (size * s) / 2;
  const inner = Math.ceil(symbolPx) / 2 + 1;
  const ring = (radius: number, width: number, colour: string) => {
    ctx.beginPath();
    ctx.arc(centre, centre, radius * s, 0, Math.PI * 2);
    ctx.strokeStyle = colour;
    ctx.lineWidth = width * s;
    ctx.stroke();
  };
  ring(inner + 0.5, 1, outline);
  ring(inner + 2, 2, selection);
  ring(inner + 3.5, 1, outline);
  return canvas;
}
