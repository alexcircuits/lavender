/**
 * WebAssembly Vector Geometry & Boundary Micro-Kernel (ADR 0009).
 *
 * Provides ultra-fast SIMD-accelerated delta decoding for packed geographic vectors,
 * Ramer-Douglas-Peucker (RDP) polyline simplification, and point-in-polygon ray-casting
 * using the freestanding Zig WebAssembly kernel.
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
 */
const GEOMETRY_SCRATCH_OFFSET = 9 * 1024 * 1024;
const MAX_SCRATCH_VERTICES = 64_000;

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

/**
 * Decodes delta-encoded coordinate integers into geographic [lon, lat] pairs.
 * Uses Zig WebAssembly memory for SIMD cumulative sum and scale division.
 *
 * @param deltas Array of delta integers: [x0, y0, dx1, dy1, dx2, dy2, ...]
 * @param scale Divisor scale factor (default: 1000 for Natural Earth 3-decimal precision)
 * @returns Array of geographic [lon, lat] coordinate pairs
 */
export function deltaDecodeCoordinatesWasm(
  deltas: readonly number[] | Int32Array,
  scale = 1000,
): Array<[number, number]> {
  if (deltas.length < 2) return [];

  const ctx = getWasmContext();
  const numPairs = Math.min(Math.floor(deltas.length / 2), MAX_SCRATCH_VERTICES);
  const totalDeltas = numPairs * 2;

  const basePtr = ctx.heapBase + GEOMETRY_SCRATCH_OFFSET;
  const inputPtr = basePtr;
  const outputPtr = inputPtr + totalDeltas * 4;
  const requiredBytes = outputPtr + totalDeltas * 8;

  ensureMemory(ctx, requiredBytes);

  const int32View = new Int32Array(ctx.memory.buffer, inputPtr, totalDeltas);
  if (deltas instanceof Int32Array) {
    int32View.set(deltas.subarray(0, totalDeltas));
  } else {
    for (let i = 0; i < totalDeltas; i++) {
      int32View[i] = deltas[i] ?? 0;
    }
  }

  const decodedCount = ctx.exports.lavender_delta_decode_coords(
    inputPtr,
    totalDeltas,
    scale,
    outputPtr,
    totalDeltas,
  );

  const float64View = new Float64Array(ctx.memory.buffer, outputPtr, decodedCount);
  const outPoints: Array<[number, number]> = new Array(decodedCount / 2);

  for (let i = 0; i < decodedCount / 2; i++) {
    outPoints[i] = [float64View[i * 2] ?? 0, float64View[i * 2 + 1] ?? 0];
  }

  return outPoints;
}

/**
 * Simplifies a polyline using the Ramer-Douglas-Peucker algorithm in WebAssembly.
 *
 * @param coords Array of [lon, lat] coordinate pairs
 * @param epsilon Tolerance distance threshold in degrees
 * @returns Simplified array of [lon, lat] coordinate pairs
 */
export function simplifyRdpWasm(
  coords: ReadonlyArray<readonly [number, number]>,
  epsilon: number,
): Array<[number, number]> {
  if (coords.length <= 2) {
    return coords.map(([lon, lat]) => [lon, lat]);
  }

  const ctx = getWasmContext();
  const numPoints = Math.min(coords.length, 4096);
  const totalCoords = numPoints * 2;

  const basePtr = ctx.heapBase + GEOMETRY_SCRATCH_OFFSET;
  const inputPtr = basePtr;
  const outputPtr = inputPtr + totalCoords * 8;
  const requiredBytes = outputPtr + totalCoords * 8;

  ensureMemory(ctx, requiredBytes);

  const inputView = new Float64Array(ctx.memory.buffer, inputPtr, totalCoords);
  for (let i = 0; i < numPoints; i++) {
    const pt = coords[i];
    inputView[i * 2] = pt ? pt[0] : 0;
    inputView[i * 2 + 1] = pt ? pt[1] : 0;
  }

  const outPointsCount = ctx.exports.lavender_simplify_rdp(
    inputPtr,
    numPoints,
    epsilon,
    outputPtr,
    numPoints,
  );

  const outputView = new Float64Array(ctx.memory.buffer, outputPtr, outPointsCount * 2);
  const simplified: Array<[number, number]> = new Array(outPointsCount);

  for (let i = 0; i < outPointsCount; i++) {
    simplified[i] = [outputView[i * 2] ?? 0, outputView[i * 2 + 1] ?? 0];
  }

  return simplified;
}

/**
 * Tests whether a point [lon, lat] is inside a polygon ring using ray-casting in WebAssembly.
 *
 * @param lon Test longitude
 * @param lat Test latitude
 * @param polygon Array of [lon, lat] coordinate pairs forming the polygon ring
 * @returns true if the point is strictly inside the polygon
 */
export function pointInPolygonWasm(
  lon: number,
  lat: number,
  polygon: ReadonlyArray<readonly [number, number]>,
): boolean {
  if (polygon.length < 3) return false;

  const ctx = getWasmContext();
  const numPoints = Math.min(polygon.length, 8192);
  const totalCoords = numPoints * 2;

  const basePtr = ctx.heapBase + GEOMETRY_SCRATCH_OFFSET;
  const inputPtr = basePtr;
  const requiredBytes = inputPtr + totalCoords * 8;

  ensureMemory(ctx, requiredBytes);

  const inputView = new Float64Array(ctx.memory.buffer, inputPtr, totalCoords);

  for (let i = 0; i < numPoints; i++) {
    const pt = polygon[i];
    inputView[i * 2] = pt ? pt[0] : 0;
    inputView[i * 2 + 1] = pt ? pt[1] : 0;
  }

  return Boolean(ctx.exports.lavender_point_in_polygon(lon, lat, inputPtr, numPoints));
}

export { loadLavenderWasm };
