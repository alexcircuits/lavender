/**
//! CCTV Camera Viewshed and Frustum WebAssembly Micro-Kernel (ADR 0009).
//!
//! Provides ultra-low-latency 3D spherical trigonometry to project camera
//! viewing pyramids, ground planes, and wireframe vertices for Cesium.
//! Backed by the freestanding Zig WebAssembly kernel.
*/
import { getActiveLavenderWasm, type LavenderWasmContext } from './wasmPropagation';

export interface CctvCameraPose {
  readonly lat: number;
  readonly lon: number;
  readonly groundAltM?: number;
  readonly mountHeightM?: number;
  readonly headingDeg?: number;
  readonly pitchDeg?: number;
  readonly fovDeg?: number;
  readonly rangeM?: number;
}

export interface CctvFrustumPoints {
  readonly mount: [number, number, number]; // [lon, lat, alt]
  readonly capCenter: [number, number, number];
  readonly tl: [number, number, number];
  readonly tr: [number, number, number];
  readonly br: [number, number, number];
  readonly bl: [number, number, number];
}

/**
 * Scratch memory offsets in WASM linear memory:
 * 12MB - 14MB: CCTV camera poses & wireframe vertex scratch
 */
const CCTV_SCRATCH_OFFSET = 12 * 1024 * 1024;
const MAX_SCRATCH_CAMERAS = 4000;

function getWasmContext(): LavenderWasmContext | null {
  return getActiveLavenderWasm();
}

function ensureMemory(wasm: LavenderWasmContext, requiredBytes: number): void {
  if (requiredBytes > wasm.memory.buffer.byteLength) {
    const bytesNeeded = requiredBytes - wasm.memory.buffer.byteLength;
    const pagesNeeded = Math.ceil(bytesNeeded / 65536);
    wasm.memory.grow(pagesNeeded);
  }
}

/**
 * Computes 3D frustum points for a single CCTV camera.
 * Uses Zig WebAssembly when available, with pure TS fallback.
 */
export function computeCctvFrustumWasm(pose: CctvCameraPose): CctvFrustumPoints {
  const ctx = getWasmContext();
  const groundAlt = pose.groundAltM ?? 0;
  const mountHeight = pose.mountHeightM ?? 24;
  const heading = pose.headingDeg ?? 0;
  const pitch = pose.pitchDeg ?? -15;
  const fov = pose.fovDeg ?? 70;
  const range = pose.rangeM ?? 600;

  if (ctx?.exports.lavender_cctv_compute_frustum) {
    const basePtr = ctx.heapBase + CCTV_SCRATCH_OFFSET;
    ensureMemory(ctx, basePtr + 18 * 8);

    const ok = ctx.exports.lavender_cctv_compute_frustum(
      pose.lat,
      pose.lon,
      groundAlt,
      mountHeight,
      heading,
      pitch,
      fov,
      range,
      basePtr,
    );

    if (ok) {
      const f64 = new Float64Array(ctx.memory.buffer, basePtr, 18);
      return {
        mount: [f64[0] ?? 0, f64[1] ?? 0, f64[2] ?? 0],
        capCenter: [f64[3] ?? 0, f64[4] ?? 0, f64[5] ?? 0],
        tl: [f64[6] ?? 0, f64[7] ?? 0, f64[8] ?? 0],
        tr: [f64[9] ?? 0, f64[10] ?? 0, f64[11] ?? 0],
        br: [f64[12] ?? 0, f64[13] ?? 0, f64[14] ?? 0],
        bl: [f64[15] ?? 0, f64[16] ?? 0, f64[17] ?? 0],
      };
    }
  }

  // Pure TS fallback calculation matching Zig kernel
  return fallbackComputeFrustum(pose);
}

/**
 * Batch computes wireframe segments for an array of CCTV cameras.
 * Each camera produces 16 vertices (48 float32s: [lon, lat, alt] * 16):
 * - 4 rays from mount to tl, tr, br, bl
 * - 4 perimeter edges tl->tr, tr->br, br->bl, bl->tl
 *
 * @returns Flat Float32Array containing Cesium-ready polyline vertices
 */
export function batchCctvWireframeWasm(cameras: readonly CctvCameraPose[]): Float32Array {
  if (cameras.length === 0) return new Float32Array(0);

  const ctx = getWasmContext();
  const count = Math.min(cameras.length, MAX_SCRATCH_CAMERAS);

  if (ctx?.exports.lavender_cctv_batch_wireframe) {
    const basePtr = ctx.heapBase + CCTV_SCRATCH_OFFSET;
    const inputBytes = count * 8 * 8; // 8 f64 per camera
    const outputBytes = count * 48 * 4; // 48 f32 per camera
    ensureMemory(ctx, basePtr + inputBytes + outputBytes);

    const inputPtr = basePtr;
    const outputPtr = basePtr + inputBytes;

    const inputView = new Float64Array(ctx.memory.buffer, inputPtr, count * 8);
    for (let i = 0; i < count; i++) {
      const c = cameras[i];
      if (!c) continue;
      const b = i * 8;
      inputView[b + 0] = c.lat;
      inputView[b + 1] = c.lon;
      inputView[b + 2] = c.groundAltM ?? 0;
      inputView[b + 3] = c.mountHeightM ?? 24;
      inputView[b + 4] = c.headingDeg ?? 0;
      inputView[b + 5] = c.pitchDeg ?? -15;
      inputView[b + 6] = c.fovDeg ?? 70;
      inputView[b + 7] = c.rangeM ?? 600;
    }

    const written = ctx.exports.lavender_cctv_batch_wireframe(inputPtr, count, outputPtr, count);

    const outView = new Float32Array(ctx.memory.buffer, outputPtr, written * 48);
    return new Float32Array(outView);
  }

  // Fallback if WASM not available
  const result = new Float32Array(count * 48);
  for (let i = 0; i < count; i++) {
    const c = cameras[i];
    if (!c) continue;
    const g = fallbackComputeFrustum(c);
    const b = i * 48;

    // Ray 1: mount -> tl
    result[b + 0] = g.mount[0];
    result[b + 1] = g.mount[1];
    result[b + 2] = g.mount[2];
    result[b + 3] = g.tl[0];
    result[b + 4] = g.tl[1];
    result[b + 5] = g.tl[2];

    // Ray 2: mount -> tr
    result[b + 6] = g.mount[0];
    result[b + 7] = g.mount[1];
    result[b + 8] = g.mount[2];
    result[b + 9] = g.tr[0];
    result[b + 10] = g.tr[1];
    result[b + 11] = g.tr[2];

    // Ray 3: mount -> br
    result[b + 12] = g.mount[0];
    result[b + 13] = g.mount[1];
    result[b + 14] = g.mount[2];
    result[b + 15] = g.br[0];
    result[b + 16] = g.br[1];
    result[b + 17] = g.br[2];

    // Ray 4: mount -> bl
    result[b + 18] = g.mount[0];
    result[b + 19] = g.mount[1];
    result[b + 20] = g.mount[2];
    result[b + 21] = g.bl[0];
    result[b + 22] = g.bl[1];
    result[b + 23] = g.bl[2];

    // Perimeter 1: tl -> tr
    result[b + 24] = g.tl[0];
    result[b + 25] = g.tl[1];
    result[b + 26] = g.tl[2];
    result[b + 27] = g.tr[0];
    result[b + 28] = g.tr[1];
    result[b + 29] = g.tr[2];

    // Perimeter 2: tr -> br
    result[b + 30] = g.tr[0];
    result[b + 31] = g.tr[1];
    result[b + 32] = g.tr[2];
    result[b + 33] = g.br[0];
    result[b + 34] = g.br[1];
    result[b + 35] = g.br[2];

    // Perimeter 3: br -> bl
    result[b + 36] = g.br[0];
    result[b + 37] = g.br[1];
    result[b + 38] = g.br[2];
    result[b + 39] = g.bl[0];
    result[b + 40] = g.bl[1];
    result[b + 41] = g.bl[2];

    // Perimeter 4: bl -> tl
    result[b + 42] = g.bl[0];
    result[b + 43] = g.bl[1];
    result[b + 44] = g.bl[2];
    result[b + 45] = g.tl[0];
    result[b + 46] = g.tl[1];
    result[b + 47] = g.tl[2];
  }

  return result;
}

// Spherical math fallback
const WGS84_A = 6378137.0;
const DEG_TO_RAD = Math.PI / 180.0;
const RAD_TO_DEG = 180.0 / Math.PI;

function projectPointFallback(
  latDeg: number,
  lonDeg: number,
  headingDeg: number,
  distanceM: number,
): [number, number] {
  if (distanceM === 0) return [lonDeg, latDeg];

  const d = distanceM / WGS84_A;
  const latRad = latDeg * DEG_TO_RAD;
  const lonRad = lonDeg * DEG_TO_RAD;
  const brgRad = headingDeg * DEG_TO_RAD;

  const sinLat = Math.sin(latRad);
  const cosLat = Math.cos(latRad);
  const sinD = Math.sin(d);
  const cosD = Math.cos(d);

  const destLatRad = Math.asin(sinLat * cosD + cosLat * sinD * Math.cos(brgRad));
  const destLonRad =
    lonRad + Math.atan2(Math.sin(brgRad) * sinD * cosLat, cosD - sinLat * Math.sin(destLatRad));

  let destLon = destLonRad * RAD_TO_DEG;
  while (destLon > 180) destLon -= 360;
  while (destLon < -180) destLon += 360;

  return [destLon, destLatRad * RAD_TO_DEG];
}

function fallbackComputeFrustum(pose: CctvCameraPose): CctvFrustumPoints {
  const pitch = Math.max(-89, Math.min(89, pose.pitchDeg ?? -15));
  const fov = Math.max(8, Math.min(160, pose.fovDeg ?? 70));
  const range = Math.max(1, pose.rangeM ?? 600);
  const groundAlt = pose.groundAltM ?? 0;
  const mountAlt = groundAlt + (pose.mountHeightM ?? 24);
  const heading = pose.headingDeg ?? 0;

  const pitchRad = pitch * DEG_TO_RAD;
  const fovRad = fov * DEG_TO_RAD;

  const horizDist = range * Math.cos(pitchRad);
  const vertDist = range * Math.sin(pitchRad);

  const capCenterCoords = projectPointFallback(pose.lat, pose.lon, heading, horizDist);
  const capCenterAlt = mountAlt + vertDist;

  const halfW = range * Math.tan(fovRad / 2.0);
  const vFovRad = 2.0 * Math.atan(Math.tan(fovRad / 2.0) / (16.0 / 9.0));
  const halfH = range * Math.tan(vFovRad / 2.0);

  const leftBrg = heading - 90;
  const rightBrg = heading + 90;

  const upVert = Math.cos(pitchRad) * halfH;
  const upHoriz = -Math.sin(pitchRad) * halfH;

  const tlCenter = projectPointFallback(capCenterCoords[1], capCenterCoords[0], heading, upHoriz);
  const tlCoords = projectPointFallback(tlCenter[1], tlCenter[0], leftBrg, halfW);
  const trCoords = projectPointFallback(tlCenter[1], tlCenter[0], rightBrg, halfW);

  const brCenter = projectPointFallback(capCenterCoords[1], capCenterCoords[0], heading, -upHoriz);
  const brCoords = projectPointFallback(brCenter[1], brCenter[0], rightBrg, halfW);
  const blCoords = projectPointFallback(brCenter[1], brCenter[0], leftBrg, halfW);

  return {
    mount: [pose.lon, pose.lat, mountAlt],
    capCenter: [capCenterCoords[0], capCenterCoords[1], capCenterAlt],
    tl: [tlCoords[0], tlCoords[1], capCenterAlt + upVert],
    tr: [trCoords[0], trCoords[1], capCenterAlt + upVert],
    br: [brCoords[0], brCoords[1], capCenterAlt - upVert],
    bl: [blCoords[0], blCoords[1], capCenterAlt - upVert],
  };
}
