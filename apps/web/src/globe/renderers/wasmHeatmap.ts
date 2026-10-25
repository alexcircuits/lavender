/**
 * WebAssembly Heatmap Binning Micro-Kernel (ADR 0009).
 *
 * Bins interleaved [lon, lat, weight] points onto an equirectangular grid in
 * the freestanding Zig WebAssembly kernel, producing two f32 planes (weighted
 * sum and count) ready for direct texture upload: row 0 is lat_max (north at
 * the top), column 0 is lon_min. Accumulation is additive, so callers can
 * batch. Points outside the bounds or non-finite are skipped by the kernel.
 */
import {
  getActiveLavenderWasm,
  type LavenderWasmContext,
  loadLavenderWasm,
} from './wasmPropagation';

/**
 * Scratch memory offsets in WASM linear memory:
 * 0 - 4MB: SGP4 orbit propagator scratch
 * 4MB - 8MB: Wind grid & streamline scratch
 * 8MB - 9MB: Label candidate scratch
 * 9MB - 12MB: Geometry vector & delta-decode scratch
 * 12MB - 16MB: Heatmap binning scratch (this module)
 */
const HEATMAP_SCRATCH_OFFSET = 12 * 1024 * 1024;
const MAX_HEATMAP_POINTS = 200_000;
const MAX_HEATMAP_CELLS = 4_000_000;

export interface HeatmapBounds {
  readonly lonMin: number;
  readonly lonMax: number;
  readonly latMin: number;
  readonly latMax: number;
}

export interface HeatmapGrid {
  readonly width: number;
  readonly height: number;
  /** Weighted sums per cell, row-major, row 0 = north (latMax). */
  readonly weightSum: Float32Array;
  /** Point counts per cell, same layout as `weightSum`. */
  readonly count: Float32Array;
  /** How many of the input points fell inside the bounds. */
  readonly binned: number;
}

function getWasmContext(): LavenderWasmContext {
  const wasm = getActiveLavenderWasm();
  if (!wasm) {
    throw new Error('Lavender WebAssembly module is not loaded. Call loadLavenderWasm() first.');
  }
  return wasm;
}

function ensureMemory(wasm: LavenderWasmContext, requiredBytes: number): void {
  if (requiredBytes > wasm.memory.buffer.byteLength) {
    const bytesNeeded = requiredBytes - wasm.memory.buffer.byteLength;
    const pagesNeeded = Math.ceil(bytesNeeded / 65536);
    wasm.memory.grow(pagesNeeded);
  }
}

export { loadLavenderWasm };

/**
 * Pure-TypeScript binning with the kernel's exact semantics: the same layout,
 * the same skip rules (out-of-bounds, non-finite). Used directly when the WASM
 * export is unavailable (older committed artifact) and as the reference the
 * kernel path must agree with.
 */
export function heatmapBinTs(
  points: readonly number[] | Float64Array,
  width: number,
  height: number,
  bounds: HeatmapBounds,
): HeatmapGrid {
  const cells = width * height;
  const weightSum = new Float32Array(cells);
  const count = new Float32Array(cells);
  const lonSpan = bounds.lonMax - bounds.lonMin;
  const latSpan = bounds.latMax - bounds.latMin;
  let binned = 0;
  if (lonSpan > 0 && latSpan > 0) {
    const numPoints = Math.floor(points.length / 3);
    for (let i = 0; i < numPoints; i++) {
      const lon = points[i * 3] ?? NaN;
      const lat = points[i * 3 + 1] ?? NaN;
      const weight = points[i * 3 + 2] ?? NaN;
      if (
        !Number.isFinite(lon) ||
        !Number.isFinite(lat) ||
        !Number.isFinite(weight) ||
        lon < bounds.lonMin ||
        lon > bounds.lonMax ||
        lat < bounds.latMin ||
        lat > bounds.latMax
      ) {
        continue;
      }
      const col = Math.min(width - 1, Math.floor(((lon - bounds.lonMin) / lonSpan) * width));
      const row = Math.min(height - 1, Math.floor(((bounds.latMax - lat) / latSpan) * height));
      const cell = row * width + col;
      weightSum[cell] = (weightSum[cell] ?? 0) + weight;
      count[cell] = (count[cell] ?? 0) + 1;
      binned++;
    }
  }
  return { width, height, weightSum, count, binned };
}

/**
 * Bins through the WASM kernel when the loaded artifact exports it, and
 * through `heatmapBinTs` otherwise, so the renderer works on every build.
 */
export function heatmapBin(
  points: readonly number[] | Float64Array,
  width: number,
  height: number,
  bounds: HeatmapBounds,
): HeatmapGrid {
  if (width <= 0 || height <= 0) throw new Error('Heatmap grid needs positive dimensions.');
  const cells = width * height;
  if (cells > MAX_HEATMAP_CELLS) {
    throw new Error(`Heatmap grid too large (max ${MAX_HEATMAP_CELLS} cells).`);
  }
  const wasm = getActiveLavenderWasm();
  if (wasm?.exports.lavender_heatmap_bin) {
    return heatmapBinWasm(points, width, height, bounds);
  }
  return heatmapBinTs(points, width, height, bounds);
}

/**
 * Bins `points` (interleaved [lon, lat, weight] triples) onto a
 * `width` × `height` grid over `bounds`.
 *
 * @throws When the module is not loaded, or the inputs exceed the kernel's caps.
 */
export function heatmapBinWasm(
  points: readonly number[] | Float64Array,
  width: number,
  height: number,
  bounds: HeatmapBounds,
): HeatmapGrid {
  if (width <= 0 || height <= 0) throw new Error('Heatmap grid needs positive dimensions.');
  const cells = width * height;
  if (cells > MAX_HEATMAP_CELLS)
    throw new Error(`Heatmap grid too large (max ${MAX_HEATMAP_CELLS} cells).`);
  const numPoints = Math.min(Math.floor(points.length / 3), MAX_HEATMAP_POINTS);
  if (numPoints === 0) {
    return {
      width,
      height,
      weightSum: new Float32Array(cells),
      count: new Float32Array(cells),
      binned: 0,
    };
  }

  const ctx = getWasmContext();
  const inputBytes = numPoints * 3 * 8;
  const outputBytes = cells * 4;
  ensureMemory(ctx, ctx.heapBase + HEATMAP_SCRATCH_OFFSET + inputBytes + 2 * outputBytes);

  const basePtr = ctx.heapBase + HEATMAP_SCRATCH_OFFSET;
  new Float64Array(ctx.memory.buffer, basePtr, numPoints * 3).set(
    points instanceof Float64Array
      ? points.subarray(0, numPoints * 3)
      : points.slice(0, numPoints * 3),
  );
  const weightPtr = basePtr + inputBytes;
  const countPtr = weightPtr + outputBytes;
  new Float32Array(ctx.memory.buffer, weightPtr, cells).fill(0);
  new Float32Array(ctx.memory.buffer, countPtr, cells).fill(0);

  const bin = ctx.exports.lavender_heatmap_bin;
  if (!bin) {
    throw new Error('lavender_heatmap_bin is not exported by the loaded kernel.');
  }
  const binned = bin(
    basePtr,
    numPoints,
    width,
    height,
    bounds.lonMin,
    bounds.lonMax,
    bounds.latMin,
    bounds.latMax,
    weightPtr,
    countPtr,
  );

  return {
    width,
    height,
    weightSum: new Float32Array(ctx.memory.buffer, weightPtr, cells).slice(),
    count: new Float32Array(ctx.memory.buffer, countPtr, cells).slice(),
    binned,
  };
}
