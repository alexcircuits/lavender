/**
 * WebAssembly Sky Photometry & Kinematics Micro-Kernel Bridges (ADR 0009).
 *
 * Provides accelerated Starfield Photometry (Yale Bright Star Catalogue vertex generation),
 * Cockpit Navigation/Heading Slew, Camera Trajectory Sampling, and WMO Weather Visual Effects
 * via the freestanding Zig WASM micro-kernel with pure-TypeScript reference fallbacks.
 */

import { getActiveLavenderWasm, type LavenderWasmContext, loadLavenderWasm } from './wasmPropagation';

export { loadLavenderWasm };

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

export const STAR_LIMIT_MAGNITUDE = 6.5;
export const STAR_MIN_SIGMA_PX = 0.55;
export const STAR_MAX_SIGMA_PX = 1.35;
export const STAR_MIN_ALPHA = 0.22;
export const STAR_DEFAULT_BV = 0.6;
export const STAR_VERTEX_FLOATS = 8;
const DEG = Math.PI / 180;
const COLOR_SATURATION = 0.55;

export type CameraPose6 = {
  lat: number;
  lon: number;
  alt: number;
  heading: number;
  pitch: number;
  roll: number;
};

export interface WeatherEffectInput {
  weatherCode?: number | string | null | undefined;
  cloudCoverPct?: number | null | undefined;
  precipitationMm?: number | null | undefined;
  windKph?: number | null | undefined;
  visibilityM?: number | null | undefined;
  windDirectionDeg?: number | null | undefined;
}

export interface WeatherEffectProfile {
  available: boolean;
  cloud: number;
  rain: number;
  snow: number;
  fog: number;
  haze: number;
  droplets: number;
  storm: number;
  wind: number;
  windDirectionDeg: number;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function clamp01(value: number): number {
  return clamp(value, 0, 1);
}

function range01(value: number, min: number, max: number): number {
  if (!Number.isFinite(value) || max <= min) return 0;
  return clamp01((value - min) / (max - min));
}

/** ICRF unit vector for J2000 right ascension and declination in degrees. */
export function raDecToUnitVector(raDeg: number, decDeg: number): [number, number, number] {
  const ra = raDeg * DEG;
  const dec = decDeg * DEG;
  const cosDec = Math.cos(dec);
  return [cosDec * Math.cos(ra), cosDec * Math.sin(ra), Math.sin(dec)];
}

/** Perceived star brightness relative to limit magnitude (6.5). */
export function starBrightness(vmag: number): number {
  const magnitude = Number.isFinite(vmag) ? vmag : STAR_LIMIT_MAGNITUDE;
  return 10 ** (-0.2 * (Math.min(magnitude, STAR_LIMIT_MAGNITUDE) - STAR_LIMIT_MAGNITUDE));
}

/** Sprite parameters for a visual magnitude. */
export function starAppearance(vmag: number): { sigmaPx: number; alpha: number } {
  const brightness = starBrightness(vmag);
  return {
    sigmaPx: clamp(STAR_MIN_SIGMA_PX * brightness ** 0.3, STAR_MIN_SIGMA_PX, STAR_MAX_SIGMA_PX),
    alpha: clamp(STAR_MIN_ALPHA * brightness, STAR_MIN_ALPHA, 1),
  };
}

/** Effective star temperature from B−V color index (Ballesteros 2012). */
export function bvToTemperature(bv: number): number {
  const index = clamp(Number.isFinite(bv) ? bv : STAR_DEFAULT_BV, -0.4, 2.0);
  return 4600 * (1 / (0.92 * index + 1.7) + 1 / (0.92 * index + 0.62));
}

/** Black-body sRGB colour fit for a temperature in Kelvin (Helland). */
function temperatureToRgb(kelvin: number): [number, number, number] {
  const t = clamp(kelvin, 1000, 40000) / 100;
  const red = t <= 66 ? 255 : 329.698727446 * (t - 60) ** -0.1332047592;
  const green =
    t <= 66
      ? 99.4708025861 * Math.log(t) - 161.1195681661
      : 288.1221695283 * (t - 60) ** -0.0755148492;
  const blue = t >= 66 ? 255 : t <= 19 ? 0 : 138.5177312231 * Math.log(t - 10) - 305.0447927307;
  return [clamp(red, 0, 255) / 255, clamp(green, 0, 255) / 255, clamp(blue, 0, 255) / 255];
}

/** Display colour for a B−V index desaturated and peak-normalized. */
export function bvToRgb(bv: number): [number, number, number] {
  const [rRaw, gRaw, bRaw] = temperatureToRgb(bvToTemperature(bv));
  const r = 1 - COLOR_SATURATION + COLOR_SATURATION * rRaw;
  const g = 1 - COLOR_SATURATION + COLOR_SATURATION * gRaw;
  const b = 1 - COLOR_SATURATION + COLOR_SATURATION * bRaw;
  const peak = Math.max(r, g, b, 1e-6);
  return [r / peak, g / peak, b / peak];
}

/** Pure TypeScript fallback for star vertex decoding. */
export function buildStarVerticesTs(stars: Float32Array | readonly number[]): {
  count: number;
  vertices: Float32Array;
} {
  const count = Math.floor(stars.length / 4);
  const vertices = new Float32Array(count * STAR_VERTEX_FLOATS);
  for (let i = 0; i < count; i++) {
    const raDeg = stars[i * 4] ?? 0;
    const decDeg = stars[i * 4 + 1] ?? 0;
    const vmag = stars[i * 4 + 2] ?? 0;
    const bv = stars[i * 4 + 3] ?? 0;
    const [x, y, z] = raDecToUnitVector(raDeg, decDeg);
    const { sigmaPx, alpha } = starAppearance(vmag);
    const [r, g, b] = bvToRgb(bv);
    const offset = i * STAR_VERTEX_FLOATS;
    vertices[offset] = x;
    vertices[offset + 1] = y;
    vertices[offset + 2] = z;
    vertices[offset + 3] = sigmaPx;
    vertices[offset + 4] = r;
    vertices[offset + 5] = g;
    vertices[offset + 6] = b;
    vertices[offset + 7] = alpha;
  }
  return { count, vertices };
}

/** Builds star vertices using the Zig WASM micro-kernel when loaded, falling back to TypeScript. */
export function buildStarVertices(stars: Float32Array | readonly number[]): {
  count: number;
  vertices: Float32Array;
} {
  const count = Math.floor(stars.length / 4);
  if (count === 0) return { count: 0, vertices: new Float32Array(0) };

  const wasm = getActiveLavenderWasm();
  interface SkyExports {
    lavender_sky_build_star_vertices?: (
      stars_ptr: number,
      star_count: number,
      out_vertices: number,
    ) => number;
  }
  const fn = (wasm?.exports as unknown as SkyExports | undefined)?.lavender_sky_build_star_vertices;
  if (!wasm || !fn) {
    return buildStarVerticesTs(stars);
  }

  const inputBytes = count * 4 * 4;
  const outputBytes = count * STAR_VERTEX_FLOATS * 4;
  const inPtr = align8(wasm.heapBase + 65536);
  const outPtr = align8(inPtr + inputBytes);
  ensureMemory(wasm, outPtr + outputBytes);

  const inView = new Float32Array(wasm.memory.buffer, inPtr, count * 4);
  inView.set(
    stars instanceof Float32Array ? stars.subarray(0, count * 4) : stars.slice(0, count * 4),
  );

  const written = fn(inPtr, count, outPtr);
  const outView = new Float32Array(wasm.memory.buffer, outPtr, written * STAR_VERTEX_FLOATS);
  return { count: written, vertices: new Float32Array(outView) };
}

/** Normalizes a heading into [0, 360) range. */
export function normalizeHeading(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return ((value % 360) + 360) % 360;
}

/** Advances heading toward target along shortest arc without exceeding maxStepDeg. */
export function slewHeading(current: number, target: number, maxStepDeg: number): number {
  const wasm = getActiveLavenderWasm();
  interface KinematicsExports {
    lavender_kinematics_slew_heading?: (
      current: number,
      target: number,
      max_step: number,
    ) => number;
  }
  const fn = (wasm?.exports as unknown as KinematicsExports | undefined)
    ?.lavender_kinematics_slew_heading;
  if (fn) {
    return fn(current, target, maxStepDeg);
  }
  const from = normalizeHeading(current);
  const to = normalizeHeading(target);
  if (!Number.isFinite(maxStepDeg) || maxStepDeg <= 0) return from;
  const delta = ((to - from + 540) % 360) - 180;
  const step = Math.max(-maxStepDeg, Math.min(maxStepDeg, delta));
  return normalizeHeading(from + step);
}

/** Bounded cockpit anchor correction for a single render frame. */
export function cockpitAnchorCorrectionStep(
  distanceM: number,
  speedMps: number,
  dtSec: number,
): number {
  const wasm = getActiveLavenderWasm();
  interface AnchorExports {
    lavender_kinematics_anchor_correction?: (dist: number, speed: number, dt: number) => number;
  }
  const fn = (wasm?.exports as unknown as AnchorExports | undefined)
    ?.lavender_kinematics_anchor_correction;
  if (fn) {
    return fn(distanceM, speedMps, dtSec);
  }
  if (!Number.isFinite(distanceM) || distanceM <= 0 || !Number.isFinite(dtSec) || dtSec <= 0)
    return 0;
  const dt = Math.min(0.1, dtSec);
  const speed = Number.isFinite(speedMps) ? Math.max(0, speedMps) : 0;
  const eased = distanceM * (1 - Math.exp(-1.25 * dt));
  const rate = Math.max(0.75, speed * 0.22);
  return Math.min(distanceM, eased, rate * dt);
}

/** Wraps longitude into [-180, 180) WGS84 range. */
function wrapLongitude(lon: number): number {
  return ((((lon + 180) % 360) + 360) % 360) - 180;
}

/** Samples authored 6-DOF camera motion along a shortest-arc trajectory. */
export function sampleCameraMove(
  move: { from: CameraPose6; to: CameraPose6; easing?: 'linear' | 'ease-in-out' | undefined },
  progress: number,
): CameraPose6 {
  const t = Math.max(0, Math.min(1, Number(progress) || 0));
  if (t === 0) return { ...move.from };
  if (t === 1) return { ...move.to };

  const wasm = getActiveLavenderWasm();
  interface DirectorExports {
    lavender_director_sample_camera_move?: (
      from_ptr: number,
      to_ptr: number,
      linear: boolean,
      progress: number,
      out_ptr: number,
    ) => void;
  }
  const fn = (wasm?.exports as unknown as DirectorExports | undefined)
    ?.lavender_director_sample_camera_move;
  const isLinear = move.easing === 'linear';

  if (wasm && fn) {
    const fromPtr = align8(wasm.heapBase + 131072);
    const toPtr = align8(fromPtr + 48);
    const outPtr = align8(toPtr + 48);
    ensureMemory(wasm, outPtr + 48);
    const buf = new Float64Array(wasm.memory.buffer, fromPtr, 6);
    buf[0] = move.from.lat;
    buf[1] = move.from.lon;
    buf[2] = move.from.alt;
    buf[3] = move.from.heading;
    buf[4] = move.from.pitch;
    buf[5] = move.from.roll;

    const toBuf = new Float64Array(wasm.memory.buffer, toPtr, 6);
    toBuf[0] = move.to.lat;
    toBuf[1] = move.to.lon;
    toBuf[2] = move.to.alt;
    toBuf[3] = move.to.heading;
    toBuf[4] = move.to.pitch;
    toBuf[5] = move.to.roll;

    fn(fromPtr, toPtr, isLinear, t, outPtr);
    const res = new Float64Array(wasm.memory.buffer, outPtr, 6);
    return {
      lat: res[0] ?? 0,
      lon: res[1] ?? 0,
      alt: res[2] ?? 0,
      heading: res[3] ?? 0,
      pitch: res[4] ?? 0,
      roll: res[5] ?? 0,
    };
  }

  const eased = isLinear ? t : t < 0.5 ? 4 * t ** 3 : 1 - (-2 * t + 2) ** 3 / 2;
  const lerp = (a: number, b: number) => a + (b - a) * eased;
  const angle = (a: number, b: number) => a + (((((b - a + 540) % 360) + 360) % 360) - 180) * eased;

  return {
    lat: lerp(move.from.lat, move.to.lat),
    lon: wrapLongitude(angle(move.from.lon, move.to.lon)),
    alt: lerp(move.from.alt, move.to.alt),
    heading: angle(move.from.heading, move.to.heading),
    pitch: lerp(move.from.pitch, move.to.pitch),
    roll: angle(move.from.roll, move.to.roll),
  };
}

/** Derives weather effect strengths from WMO observations. */
export function deriveWeatherEffectProfile(
  weather: WeatherEffectInput | null | undefined,
): WeatherEffectProfile {
  const reported = weather?.weatherCode;
  if (reported == null || reported === '' || !Number.isFinite(Number(reported))) {
    return {
      available: false,
      cloud: 0,
      rain: 0,
      snow: 0,
      fog: 0,
      haze: 0,
      droplets: 0,
      storm: 0,
      wind: 0,
      windDirectionDeg: 0,
    };
  }

  const wasm = getActiveLavenderWasm();
  interface WeatherExports {
    lavender_weather_derive_effects?: (
      code: number,
      cloud: number,
      precip: number,
      wind: number,
      vis: number,
      dir: number,
      out_ptr: number,
    ) => boolean;
  }
  const fn = (wasm?.exports as unknown as WeatherExports | undefined)
    ?.lavender_weather_derive_effects;

  const code = Number(reported);
  const cover = Number(weather?.cloudCoverPct) || 0;
  const precip = Number(weather?.precipitationMm) || 0;
  const windKph = Number(weather?.windKph) || 0;
  const visM = Number(weather?.visibilityM) || 0;
  const windDir = Number(weather?.windDirectionDeg) || 0;

  if (wasm && fn) {
    const ptr = align8(wasm.heapBase + 163840);
    ensureMemory(wasm, ptr + 36);
    const ok = fn(code, cover, precip, windKph, visM, windDir, ptr);
    if (!ok) {
      return {
        available: false,
        cloud: 0,
        rain: 0,
        snow: 0,
        fog: 0,
        haze: 0,
        droplets: 0,
        storm: 0,
        wind: 0,
        windDirectionDeg: 0,
      };
    }
    const res = new Float32Array(wasm.memory.buffer, ptr, 9);
    return {
      available: true,
      cloud: res[0] ?? 0,
      rain: res[1] ?? 0,
      snow: res[2] ?? 0,
      fog: res[3] ?? 0,
      haze: res[4] ?? 0,
      droplets: res[5] ?? 0,
      storm: res[6] ?? 0,
      wind: res[7] ?? 0,
      windDirectionDeg: res[8] ?? 0,
    };
  }

  const cCover = range01(cover, 8, 100);
  const precipitation = Math.max(0, precip);
  const wind = range01(windKph, 4, 90);
  const visibilityFog = Number.isFinite(visM) ? range01(12000 - visM, 0, 11500) : 0;

  const drizzle = code >= 51 && code <= 57;
  const rainCode = (code >= 61 && code <= 67) || (code >= 80 && code <= 82) || code >= 95;
  const snowCode = (code >= 71 && code <= 77) || (code >= 85 && code <= 86);
  const fogCode = code === 45 || code === 48;
  const stormCode = code >= 95;

  const observedPrecip = range01(precipitation, 0.05, 8);
  const rainFloor = drizzle ? 0.12 : rainCode ? (stormCode ? 0.68 : 0.28) : 0;
  const snowFloor = snowCode ? 0.3 : 0;
  const rain = drizzle || rainCode ? Math.max(rainFloor, observedPrecip) : 0;
  const snow = snowCode ? Math.max(snowFloor, observedPrecip) : 0;
  const fog = Math.max(fogCode ? 0.78 : 0, visibilityFog);
  const storm = stormCode ? Math.max(0.55, observedPrecip) : 0;
  const forcedCloud = stormCode ? 0.92 : rain || snow ? 0.7 : fogCode ? 0.55 : 0;
  const cloud = Math.max(cCover, forcedCloud);

  return {
    available: true,
    cloud: clamp01(cloud),
    rain: clamp01(rain),
    snow: clamp01(snow),
    fog: clamp01(fog),
    haze: clamp01(Math.max(fog * 0.78, cloud * 0.14 + (rain + snow) * 0.2)),
    droplets: clamp01(rain * (0.58 + storm * 0.42)),
    storm: clamp01(storm),
    wind,
    windDirectionDeg: ((windDir % 360) + 360) % 360,
  };
}

/** Fade ground-weather overlays as camera climbs. */
export function weatherAltitudeFactors(altitudeM: number): {
  precipitation: number;
  cloud: number;
  haze: number;
} {
  const alt = Math.max(0, altitudeM || 0);
  return {
    precipitation: 1 - range01(alt, 6000, 14000),
    cloud: 1 - range01(alt, 14000, 24000),
    haze: 1 - range01(alt, 9000, 20000),
  };
}
