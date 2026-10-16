/**
 * A tiny 2D canvas for unit tests (happy-dom has no 2D context). It rasterises the few calls
 * the runtime-drawn glyphs make (full-circle arcs, rectangles, round-capped polylines,
 * `globalAlpha`, `destination-out`) into RGBA pixels, sampling each pixel at its centre
 * without anti-aliasing, so a test can read back which colour landed where.
 */
import { vi } from 'vitest';

/** 0–255 channels and a 0–1 alpha. */
export type Rgba = readonly [r: number, g: number, b: number, a: number];

type Point = readonly [x: number, y: number];
type Shape =
  | { readonly kind: 'circle'; readonly x: number; readonly y: number; readonly r: number }
  | { readonly kind: 'line'; readonly points: Point[] };

const FULL_TURN = Math.PI * 2;

function parseHex(style: string): Rgba {
  const match = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(style);
  if (!match?.[1]) throw new Error(`Unsupported colour ${style}`);
  const rgb = Number.parseInt(match[1], 16);
  const alpha = match[2] ? Number.parseInt(match[2], 16) / 255 : 1;
  return [rgb >> 16, (rgb >> 8) & 255, rgb & 255, alpha];
}

function distanceToSegment([px, py]: Point, [ax, ay]: Point, [bx, by]: Point): number {
  const dx = bx - ax;
  const dy = by - ay;
  const lengthSq = dx * dx + dy * dy;
  const t =
    lengthSq === 0 ? 0 : Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / lengthSq));
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}

export class RasterContext {
  readonly canvas: HTMLCanvasElement;
  fillStyle = '#000000';
  strokeStyle = '#000000';
  lineWidth = 1;
  lineCap = 'butt';
  lineJoin = 'miter';
  globalAlpha = 1;
  globalCompositeOperation = 'source-over';
  readonly #width: number;
  readonly #height: number;
  readonly #pixels: Float64Array;
  #path: Shape[] = [];

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    this.#width = canvas.width;
    this.#height = canvas.height;
    this.#pixels = new Float64Array(this.#width * this.#height * 4);
  }

  beginPath(): void {
    this.#path = [];
  }

  arc(x: number, y: number, r: number, start: number, end: number): void {
    if (Math.abs(end - start) < FULL_TURN - 1e-9) throw new Error('Only full circles are drawn.');
    this.#path.push({ kind: 'circle', x, y, r });
  }

  moveTo(x: number, y: number): void {
    this.#path.push({ kind: 'line', points: [[x, y]] });
  }

  lineTo(x: number, y: number): void {
    const last = this.#path.at(-1);
    if (last?.kind === 'line') last.points.push([x, y]);
    else this.moveTo(x, y);
  }

  fill(): void {
    const circles = this.#path.flatMap((shape) => (shape.kind === 'circle' ? [shape] : []));
    this.#paint(this.fillStyle, (p) =>
      circles.some((c) => Math.hypot(p[0] - c.x, p[1] - c.y) <= c.r),
    );
  }

  stroke(): void {
    if (this.lineCap !== 'round' || this.lineJoin !== 'round') {
      throw new Error('Only round caps and joins are rasterised.');
    }
    const half = this.lineWidth / 2;
    const lines = this.#path.flatMap((shape) => (shape.kind === 'line' ? [shape.points] : []));
    this.#paint(this.strokeStyle, (p) =>
      lines.some((points) =>
        points.some((a, i) => distanceToSegment(p, a, points[i + 1] ?? a) <= half),
      ),
    );
  }

  fillRect(x: number, y: number, width: number, height: number): void {
    this.#paint(
      this.fillStyle,
      ([px, py]) => px >= x && px < x + width && py >= y && py < y + height,
    );
  }

  /** The pixel at column `x`, row `y`. */
  pixel(x: number, y: number): Rgba {
    const i = (y * this.#width + x) * 4;
    const p = this.#pixels;
    return [p[i] ?? 0, p[i + 1] ?? 0, p[i + 2] ?? 0, p[i + 3] ?? 0];
  }

  #paint(style: string, covers: (centre: Point) => boolean): void {
    const [r, g, b, styleAlpha] = parseHex(style);
    const alpha = styleAlpha * this.globalAlpha;
    const erase = this.globalCompositeOperation === 'destination-out';
    if (!erase && this.globalCompositeOperation !== 'source-over') {
      throw new Error(`Unsupported compositing ${this.globalCompositeOperation}`);
    }
    const p = this.#pixels;
    for (let y = 0; y < this.#height; y += 1) {
      for (let x = 0; x < this.#width; x += 1) {
        if (!covers([x + 0.5, y + 0.5])) continue;
        const i = (y * this.#width + x) * 4;
        const below = p[i + 3] ?? 0;
        if (erase) {
          p[i + 3] = below * (1 - alpha);
          continue;
        }
        const out = alpha + below * (1 - alpha);
        const mix = (src: number, dst: number) =>
          out === 0 ? 0 : (src * alpha + dst * below * (1 - alpha)) / out;
        p[i] = mix(r, p[i] ?? 0);
        p[i + 1] = mix(g, p[i + 1] ?? 0);
        p[i + 2] = mix(b, p[i + 2] ?? 0);
        p[i + 3] = out;
      }
    }
  }
}

/**
 * Gives every canvas a RasterContext until `restore()`. `contextOf` returns the context a
 * drawing function used for the canvas it returned.
 */
export function installRasterCanvas(): {
  contextOf(canvas: HTMLCanvasElement): RasterContext;
  restore(): void;
} {
  const contexts = new WeakMap<HTMLCanvasElement, RasterContext>();
  function rasterContext(this: HTMLCanvasElement): RasterContext {
    let context = contexts.get(this);
    if (!context) {
      context = new RasterContext(this);
      contexts.set(this, context);
    }
    return context;
  }
  const spy = vi
    .spyOn(HTMLCanvasElement.prototype, 'getContext')
    .mockImplementation(rasterContext as unknown as HTMLCanvasElement['getContext']);
  return {
    contextOf(canvas) {
      const context = contexts.get(canvas);
      if (!context) throw new Error('This canvas was never drawn on.');
      return context;
    },
    restore: () => spy.mockRestore(),
  };
}
