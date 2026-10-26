/**
 * WebAssembly Optics, Sensor Metrics & Cyber Sonar Micro-Kernel Bridges (ADR 0009).
 *
 * Provides accelerated routines for:
 * - GSD (Ground Sample Distance) & NIIRS (General Image Quality Equation)
 * - Off-Nadir Angle (ONA) and Solar Position Elevation
 * - Cyber Sonar sweep gain & radial contact attenuation
 * - Scope aperture keyhole geometry and altitude-adaptive terminus alpha
 *
 * Backed by the freestanding Zig WASM micro-kernel with pure-TypeScript reference fallbacks.
 */

import {
  getActiveLavenderWasm,
  type LavenderWasmContext,
  loadLavenderWasm,
} from './wasmPropagation';

export { loadLavenderWasm };

export interface KeyholeGeometry {
  readonly cx: number;
  readonly cy: number;
  readonly radiusPx: number;
  readonly featherPx: number;
}

function align8(offset: number): number {
  return (offset + 7) & ~7;
}

function ensureMemory(wasm: LavenderWasmContext, requiredBytes: number): void {
  if (requiredBytes > wasm.memory.buffer.byteLength) {
    const bytesNeeded = requiredBytes - wasm.memory.buffer.byteLength;
    const pagesNeeded = Math.ceil(bytesNeeded / 65536);
    wasm.memory.grow(pagesNeeded);
  }
}

/** Pure TypeScript fallback for GSD. */
export function calculateGsdTs(altM: number): number {
  const alt = Number.isFinite(altM) && altM >= 0 ? altM : 0;
  return Math.max(0.01, alt * 0.000375);
}

/** Pure TypeScript fallback for NIIRS. */
export function calculateNiirsTs(gsdM: number): number {
  const gsd = Number.isFinite(gsdM) && gsdM > 0 ? gsdM : 0.01;
  const gsdInches = gsd * 39.37;
  const log10Val = Math.log10(gsdInches);
  const niirs = 10.25 - 3.32 * log10Val;
  return Math.max(0, Math.min(9, niirs));
}

/** Pure TypeScript fallback for Off-Nadir Angle (ONA). */
export function calculateOnaTs(pitchDeg: number): number {
  if (!Number.isFinite(pitchDeg)) return 0;
  return Math.max(0, 90 + pitchDeg);
}

function normalize360(deg: number): number {
  const m = deg % 360;
  return m < 0 ? m + 360 : m;
}

/** Pure TypeScript fallback for Sun elevation. */
export function estimateSunElevationTs(latDeg: number, lonDeg: number, unixSec: number): number {
  const lat = Math.max(-90, Math.min(90, latDeg)) * (Math.PI / 180);
  const lon = lonDeg;

  const jd = 2440587.5 + unixSec / 86400;
  const t = (jd - 2451545.0) / 36525.0;

  const l0 = normalize360(280.46646 + t * (36000.76983 + 0.0003032 * t));
  const m = normalize360(357.52911 + t * (35999.05029 - 0.0001537 * t)) * (Math.PI / 180);

  const c =
    (1.914602 - t * (0.004817 + 0.000014 * t)) * Math.sin(m) +
    (0.019993 - 0.000101 * t) * Math.sin(2 * m) +
    0.000289 * Math.sin(3 * m);

  const sunLon = (l0 + c) * (Math.PI / 180);
  const eps = (23.439291 - 0.0130042 * t) * (Math.PI / 180);

  const sinDec = Math.sin(eps) * Math.sin(sunLon);
  const cosDec = Math.sqrt(Math.max(0, 1 - sinDec * sinDec));

  const gmst = normalize360(280.46061837 + 360.98564736629 * (jd - 2451545.0));
  const lst = normalize360(gmst + lon) * (Math.PI / 180);

  const sunRa = Math.atan2(Math.cos(eps) * Math.sin(sunLon), Math.cos(sunLon));
  const ha = lst - sunRa;

  const sinElev = Math.sin(lat) * sinDec + Math.cos(lat) * cosDec * Math.cos(ha);
  const clampedSin = Math.max(-1, Math.min(1, sinElev));
  return Math.asin(clampedSin) * (180 / Math.PI);
}

/** Pure TypeScript fallback for Cyber Sonar sweep gain. */
export function calculateSonarSweepGainTs(
  targetBearingDeg: number,
  sweepHeadingDeg: number,
  sweepWidthDeg = 24,
  featherDeg = 10,
): number {
  const tb = normalize360(targetBearingDeg);
  const sh = normalize360(sweepHeadingDeg);
  const width = sweepWidthDeg <= 0 ? 24 : sweepWidthDeg;
  const feather = featherDeg <= 0 ? 1 : featherDeg;

  const rawDelta = ((tb - sh + 540) % 360) - 180;
  const halfWidth = width * 0.5;
  const distFromCenter = Math.abs(rawDelta - halfWidth);

  if (distFromCenter <= halfWidth) {
    return 1.0;
  }
  if (distFromCenter <= halfWidth + feather) {
    const t = (distFromCenter - halfWidth) / feather;
    return 1.0 - t * t * (3.0 - 2.0 * t);
  }
  return 0.0;
}

/** Pure TypeScript fallback for scope terminus alpha. */
export function calculateScopeTerminusAlphaTs(altM: number): number {
  const HIGH_ALT = 10_000_000;
  const LOW_ALT = 7_000_000;
  const ALPHA_HIGH = 0.94;
  const ALPHA_LOW = 1.0;

  if (altM >= HIGH_ALT) return ALPHA_HIGH;
  if (altM <= LOW_ALT) return ALPHA_LOW;

  const t = (altM - LOW_ALT) / (HIGH_ALT - LOW_ALT);
  const rawAlpha = ALPHA_LOW + (ALPHA_HIGH - ALPHA_LOW) * t;
  return Math.round(rawAlpha * 100) / 100;
}

/** Pure TypeScript fallback for keyhole geometry. */
export function calculateKeyholeGeometryTs(
  viewportW: number,
  viewportH: number,
  featherRatio = 0.11,
): KeyholeGeometry {
  const w = viewportW <= 0 ? 1920 : viewportW;
  const h = viewportH <= 0 ? 1080 : viewportH;
  const cx = w * 0.5;
  const cy = h * 0.5;
  const radiusPx = Math.min(w, h) * 0.61 * 0.5;
  const ratio = Math.max(0, Math.min(0.4, featherRatio));
  const featherPx = radiusPx * ratio;
  return { cx, cy, radiusPx, featherPx };
}

// ── Accelerated WASM Bridges ─────────────────────────────────────────────────────────

interface OpticsExports {
  lavender_sensor_calculate_gsd?: (alt_m: f64) => number;
  lavender_sensor_calculate_niirs?: (gsd_m: f64) => number;
  lavender_sensor_calculate_ona?: (pitch_deg: f64) => number;
  lavender_sensor_sun_elevation?: (lat_deg: f64, lon_deg: f64, unix_sec: number) => number;
  lavender_sonar_calculate_sweep_gain?: (
    tb: number,
    sh: number,
    width: number,
    feather: number,
  ) => number;
  lavender_scope_terminus_alpha?: (alt_m: number) => number;
  lavender_keyhole_geometry?: (w: number, h: number, ratio: number, out_ptr: number) => void;
}

type f64 = number;

/** Ground Sample Distance (GSD) in meters. */
export function calculateGsd(altM: number): number {
  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as OpticsExports | undefined)?.lavender_sensor_calculate_gsd;
  if (wasm && fn) {
    return fn(altM);
  }
  return calculateGsdTs(altM);
}

/** NIIRS rating (0.0 to 9.0). */
export function calculateNiirs(gsdM: number): number {
  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as OpticsExports | undefined)
    ?.lavender_sensor_calculate_niirs;
  if (wasm && fn) {
    return fn(gsdM);
  }
  return calculateNiirsTs(gsdM);
}

/** Off-Nadir Angle (ONA) in degrees. */
export function calculateOna(pitchDeg: number): number {
  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as OpticsExports | undefined)?.lavender_sensor_calculate_ona;
  if (wasm && fn) {
    return fn(pitchDeg);
  }
  return calculateOnaTs(pitchDeg);
}

/** Solar elevation in degrees. */
export function estimateSunElevation(latDeg: number, lonDeg: number, unixSec: number): number {
  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as OpticsExports | undefined)?.lavender_sensor_sun_elevation;
  if (wasm && fn) {
    return fn(latDeg, lonDeg, unixSec);
  }
  return estimateSunElevationTs(latDeg, lonDeg, unixSec);
}

/** Tactical Cyber Sonar sweep gain [0.0, 1.0]. */
export function calculateSonarSweepGain(
  targetBearingDeg: number,
  sweepHeadingDeg: number,
  sweepWidthDeg = 24,
  featherDeg = 10,
): number {
  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as OpticsExports | undefined)
    ?.lavender_sonar_calculate_sweep_gain;
  if (wasm && fn) {
    return fn(targetBearingDeg, sweepHeadingDeg, sweepWidthDeg, featherDeg);
  }
  return calculateSonarSweepGainTs(targetBearingDeg, sweepHeadingDeg, sweepWidthDeg, featherDeg);
}

/** Scope aperture terminus alpha. */
export function calculateScopeTerminusAlpha(altM: number): number {
  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as OpticsExports | undefined)?.lavender_scope_terminus_alpha;
  if (wasm && fn) {
    return fn(altM);
  }
  return calculateScopeTerminusAlphaTs(altM);
}

/** Keyhole circular aperture geometry. */
export function calculateKeyholeGeometry(
  viewportW: number,
  viewportH: number,
  featherRatio = 0.11,
): KeyholeGeometry {
  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as OpticsExports | undefined)?.lavender_keyhole_geometry;
  if (wasm && fn) {
    const ptr = align8(wasm.heapBase + 196608);
    ensureMemory(wasm, ptr + 32);
    fn(viewportW, viewportH, featherRatio, ptr);
    const res = new Float64Array(wasm.memory.buffer, ptr, 4);
    return {
      cx: res[0] ?? 0,
      cy: res[1] ?? 0,
      radiusPx: res[2] ?? 0,
      featherPx: res[3] ?? 0,
    };
  }
  return calculateKeyholeGeometryTs(viewportW, viewportH, featherRatio);
}
