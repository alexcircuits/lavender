/**
 * WebAssembly-accelerated GRIB2 Wind Grid Resampling and Streamline Baker.
 *
 * Provides ultra-fast bilinear wind field interpolation, Runge-Kutta 2nd-order (midpoint)
 * streamline integration, and spherical Fibonacci equal-area streamline baking using
 * the freestanding Zig WebAssembly kernel (`lavender_kernels.wasm`).
 */
import type { WindGridData } from '../../platform';
import {
  getActiveLavenderWasm,
  isLavenderWasmLoaded,
  type LavenderWasmContext,
  loadLavenderWasm,
} from './wasmPropagation';

export const WIND_PATH_LIMIT = 7200;
export const WIND_PATH_STEPS = 16;
export const WIND_STEP_SECONDS = 1800;
export const GOLDEN_ANGLE = 137.50776405003785;
export const DEGREE_METERS = 111320;
export const WIND_CELL_DEGREES = 30;

/** Reserve 4 MB scratch above heapBase for satellite orbit propagation */
const SGP4_SCRATCH_RESERVE = 4 * 1024 * 1024;

export interface StreamlinePath {
  readonly coordinates: Array<[number, number]>;
  readonly speed?: number | undefined;
  readonly seed?: number | undefined;
  readonly phase?: number | undefined;
}

export interface WindCellGroup {
  readonly id: number;
  readonly paths: StreamlinePath[];
}

export interface BakeAllStreamlinesOptions {
  readonly count?: number;
  readonly steps?: number;
  readonly stepSeconds?: number;
}

interface UploadedGrid {
  grid: WindGridData;
  uPtr: number;
  vPtr: number;
  scratchPtr: number;
  ni: number;
  nj: number;
  lo1: number;
  la1: number;
  di: number;
  dj: number;
}

let cachedUploaded: UploadedGrid | null = null;

export function clearWasmWindCache(): void {
  cachedUploaded = null;
}

/**
 * Normalizes a longitude to the half-open world interval [-180, 180).
 */
export function normalizeLongitude(lon: number): number {
  const norm = ((((lon + 180) % 360) + 360) % 360) - 180;
  return norm >= 180 ? -180 : norm;
}

/**
 * Metres represented by one longitude degree at a given latitude.
 */
export function metersPerDegreeLon(lat: number): number {
  return DEGREE_METERS * Math.cos((lat * Math.PI) / 180);
}

function getWasmContext(): LavenderWasmContext {
  const wasm = getActiveLavenderWasm();
  if (!wasm) {
    throw new Error('Lavender WebAssembly module is not loaded. Call loadLavenderWasm() first.');
  }
  return wasm;
}

function uploadGrid(
  wasm: LavenderWasmContext,
  grid: WindGridData,
  scratchBytes: number,
): UploadedGrid | null {
  const ni = grid.nx;
  const nj = grid.ny;
  if (!Number.isFinite(ni) || !Number.isFinite(nj) || ni < 1 || nj < 1) {
    return null;
  }
  const points = ni * nj;
  if (!grid.u || !grid.v || grid.u.length < points || grid.v.length < points) {
    return null;
  }

  const lo1 = grid.lo1 ?? 0.0;
  const la1 = grid.la1 ?? 90.0;
  const di = grid.di ?? grid.dx ?? 360.0 / ni;
  const dj = grid.dj ?? grid.dy ?? (nj > 1 ? 180.0 / (nj - 1) : 1.0);

  if (
    !Number.isFinite(lo1) ||
    !Number.isFinite(la1) ||
    !Number.isFinite(di) ||
    !Number.isFinite(dj) ||
    di <= 0 ||
    dj <= 0
  ) {
    return null;
  }

  const windBase = wasm.heapBase + SGP4_SCRATCH_RESERVE;
  const gridBytes = points * 4;
  const uPtr = windBase;
  const vPtr = uPtr + gridBytes;
  const scratchPtr = vPtr + gridBytes;

  const totalRequired = SGP4_SCRATCH_RESERVE + gridBytes * 2 + Math.max(65536, scratchBytes);
  wasm.ensureCapacity(totalRequired);

  if (
    cachedUploaded === null ||
    cachedUploaded.grid !== grid ||
    cachedUploaded.grid.u !== grid.u ||
    cachedUploaded.grid.v !== grid.v ||
    cachedUploaded.ni !== ni ||
    cachedUploaded.nj !== nj ||
    cachedUploaded.lo1 !== lo1 ||
    cachedUploaded.la1 !== la1 ||
    cachedUploaded.di !== di ||
    cachedUploaded.dj !== dj
  ) {
    const memU = new Float32Array(wasm.memory.buffer, uPtr, points);
    memU.set(grid.u.subarray(0, points));

    const memV = new Float32Array(wasm.memory.buffer, vPtr, points);
    memV.set(grid.v.subarray(0, points));

    cachedUploaded = {
      grid,
      uPtr,
      vPtr,
      scratchPtr,
      ni,
      nj,
      lo1,
      la1,
      di,
      dj,
    };
  }

  return cachedUploaded;
}

/**
 * Samples U (eastward) and V (northward) wind vectors at (lon, lat) using WebAssembly bilinear interpolation.
 * Longitude wraps seamlessly across the seam; latitude clamps to grid pole rows.
 *
 * @param grid Wind grid data with planar U and V buffers
 * @param lon Geographic longitude in degrees
 * @param lat Geographic latitude in degrees
 * @param outUv Object where `{ u, v }` in m/s will be written
 * @returns true if sample succeeded; false if inputs were invalid or non-finite
 */
export function sampleWasmWind(
  grid: WindGridData,
  lon: number,
  lat: number,
  outUv: { u: number; v: number },
): boolean {
  if (!Number.isFinite(lon) || !Number.isFinite(lat)) {
    return false;
  }
  if (!grid?.u || !grid.v || grid.nx < 1 || grid.ny < 1) {
    return false;
  }
  const wasm = getWasmContext();
  const uploaded = uploadGrid(wasm, grid, 8);
  if (!uploaded) {
    return false;
  }

  wasm.exports.lavender_wind_sample(
    uploaded.ni,
    uploaded.nj,
    uploaded.lo1,
    uploaded.la1,
    uploaded.di,
    uploaded.dj,
    uploaded.uPtr,
    uploaded.vPtr,
    lon,
    lat,
    uploaded.scratchPtr,
  );

  const uvView = new Float32Array(wasm.memory.buffer, uploaded.scratchPtr, 2);
  const u = uvView[0] ?? 0;
  const v = uvView[1] ?? 0;

  if (!Number.isFinite(u) || !Number.isFinite(v)) {
    return false;
  }

  outUv.u = u;
  outUv.v = v;
  return true;
}

/**
 * Bakes a single streamline trajectory starting from `(seedLon, seedLat)` using 2nd-order Runge-Kutta
 * midpoint integration in the WebAssembly kernel.
 *
 * Traces both backward (against wind) and forward (with wind), ordering points from upstream to downstream.
 * Points are written interleaved `[lon0, lat0, lon1, lat1, ...]` into `outBuffer`.
 *
 * @param grid Wind grid data
 * @param seedLon Seed longitude in degrees
 * @param seedLat Seed latitude in degrees
 * @param steps Number of half-steps to trace in each direction
 * @param stepSeconds Integration step size in seconds
 * @param outBuffer Output Float32Array coordinate buffer (must hold at least 6 floats for 3 points)
 * @returns Number of points written (0 if calm, polar, invalid, or fewer than 3 points)
 */
export function bakeStreamlineWasm(
  grid: WindGridData,
  seedLon: number,
  seedLat: number,
  steps: number,
  stepSeconds: number,
  outBuffer: Float32Array,
): number {
  if (
    !Number.isFinite(seedLon) ||
    !Number.isFinite(seedLat) ||
    Math.abs(seedLat) > 88.5 ||
    steps <= 0 ||
    outBuffer.length < 6
  ) {
    return 0;
  }
  if (!grid?.u || !grid.v || grid.nx < 1 || grid.ny < 1) {
    return 0;
  }
  const wasm = getWasmContext();
  const maxPoints = Math.floor(outBuffer.length / 2);
  const scratchBytes = maxPoints * 2 * 4;
  const uploaded = uploadGrid(wasm, grid, scratchBytes);
  if (!uploaded) {
    return 0;
  }

  const pointsCount = wasm.exports.lavender_wind_bake_streamline(
    uploaded.ni,
    uploaded.nj,
    uploaded.lo1,
    uploaded.la1,
    uploaded.di,
    uploaded.dj,
    uploaded.uPtr,
    uploaded.vPtr,
    seedLon,
    seedLat,
    steps,
    stepSeconds,
    uploaded.scratchPtr,
    maxPoints,
  );

  if (pointsCount >= 3) {
    const coordsView = new Float32Array(wasm.memory.buffer, uploaded.scratchPtr, pointsCount * 2);
    outBuffer.set(coordsView);
    return pointsCount;
  }

  return 0;
}

/**
 * Bakes all global streamlines using an equal-area spherical Fibonacci golden-angle seed distribution.
 *
 * Seeds are distributed evenly across the globe avoiding polar crowding. For each seed with sustained
 * wind (>= 0.15 m/s), traces a streamline via the WebAssembly kernel.
 *
 * @param grid Wind grid data
 * @param options Baking parameters (count, steps, stepSeconds)
 * @returns Array of baked streamlines with geographic coordinates ordered upstream-to-downstream
 */
export function bakeAllStreamlinesWasm(
  grid: WindGridData,
  options?: BakeAllStreamlinesOptions,
): StreamlinePath[] {
  if (!grid?.u || !grid.v || grid.nx < 1 || grid.ny < 1) {
    return [];
  }
  // Ensure WASM is loaded before starting the loop
  getWasmContext();

  const rawCount = options?.count ?? WIND_PATH_LIMIT;
  const budget = Math.min(
    WIND_PATH_LIMIT,
    Math.max(0, Math.floor(Number.isFinite(rawCount) ? rawCount : WIND_PATH_LIMIT)),
  );
  if (budget <= 0) {
    return [];
  }

  const rawSteps = options?.steps ?? WIND_PATH_STEPS;
  const steps = Math.min(
    WIND_PATH_STEPS,
    Math.max(1, Math.floor(Number.isFinite(rawSteps) ? rawSteps : WIND_PATH_STEPS)),
  );

  const rawStepSeconds = options?.stepSeconds ?? WIND_STEP_SECONDS;
  const stepSeconds = Math.min(
    3600,
    Math.max(1, Number.isFinite(rawStepSeconds) ? rawStepSeconds : WIND_STEP_SECONDS),
  );

  const maxPoints = steps * 2 + 1;
  const scratchBuffer = new Float32Array(maxPoints * 2);
  const paths: StreamlinePath[] = [];

  const sin88 = Math.sin((88 * Math.PI) / 180);
  const scratchUv = { u: 0, v: 0 };

  for (let seed = 0; seed < budget; seed++) {
    const lon = normalizeLongitude(seed * GOLDEN_ANGLE);
    const lat = (Math.asin((((seed + 0.5) / budget) * 2 - 1) * sin88) * 180) / Math.PI;

    const pointsCount = bakeStreamlineWasm(grid, lon, lat, steps, stepSeconds, scratchBuffer);
    if (pointsCount >= 3) {
      const coordinates: Array<[number, number]> = new Array(pointsCount);
      for (let i = 0; i < pointsCount; i++) {
        const pLon = scratchBuffer[i * 2] ?? 0;
        const pLat = scratchBuffer[i * 2 + 1] ?? 0;
        coordinates[i] = [pLon, pLat];
      }

      sampleWasmWind(grid, lon, lat, scratchUv);
      const speed = Math.hypot(scratchUv.u, scratchUv.v);

      paths.push({
        coordinates,
        speed: Number.isFinite(speed) ? speed : undefined,
        seed,
        phase: ((seed + 1) * 0.6180339887498949) % 1,
      });
    }
  }

  return paths;
}

/**
 * Group baked paths by their middle coordinate into equirectangular cells.
 * Preserves path and cell order for spatial binning and frustum culling.
 */
export function groupWindPaths(
  paths: readonly StreamlinePath[],
  cellDegrees: number = WIND_CELL_DEGREES,
): WindCellGroup[] {
  if (!Number.isFinite(cellDegrees) || cellDegrees <= 0 || cellDegrees > 180) {
    throw new RangeError('Wind cell size must be between 0 and 180 degrees');
  }
  const columns = Math.ceil(360 / cellDegrees);
  const rows = Math.ceil(180 / cellDegrees);
  const cells = new Map<number, StreamlinePath[]>();

  for (const path of paths) {
    const middleIndex = Math.floor(path.coordinates.length / 2);
    const middle = path.coordinates[middleIndex];
    if (!middle) continue;
    const [lon, lat] = middle;
    const column = Math.floor((normalizeLongitude(lon) + 180) / cellDegrees);
    const row = Math.min(rows - 1, Math.max(0, Math.floor((lat + 90) / cellDegrees)));
    const id = row * columns + column;

    let cellList = cells.get(id);
    if (!cellList) {
      cellList = [];
      cells.set(id, cellList);
    }
    cellList.push(path);
  }

  const result: WindCellGroup[] = [];
  for (const [id, cellPaths] of cells.entries()) {
    result.push({ id, paths: cellPaths });
  }
  return result;
}

export { getActiveLavenderWasm, isLavenderWasmLoaded, loadLavenderWasm };
