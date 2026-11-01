/**
 * Cockpit cloud deck model (LB-161): the pure, testable core of the weather-backed
 * cloud overlay ported from upstream `legacy/src/cockpitCloudEffects.js` — the capped
 * low-res framebuffer sizing, the time/movement weather refresh gate, the strength
 * mapping through the WASM weather profile, and the frame-budget fps clamp.
 *
 * Upstream rendered a raymarched WebGL cloudscape; Lavender renders a layered 2.5D
 * deck on canvas 2D instead (cheaper on integrated GPUs, and composites over Cesium
 * without a second GL context). The layer geometry this module derives — three
 * parallax cloud bands plus fog veil and storm darkness — is what the renderer draws;
 * the frame-budget hint from the kernel (`WeatherEffectProfile.frameCost`) maps onto
 * the 12–15 fps timestamp clamp so heavy weather spends fewer, not heavier, frames.
 *
 * No DOM, no Cesium: everything here runs in unit tests directly.
 */

import type { WeatherEffectProfile } from '../../globe/renderers/wasmSkyKinematics';
import { greatCircleM } from '../annotations/geometry';

/** Upstream framebuffer caps (legacy/src/cockpitCloudEffects.js). */
export const CLOUD_MAX_RENDER_WIDTH = 520;
export const CLOUD_MAX_RENDER_HEIGHT = 320;
/** Largest fraction of the viewport the low-res backing store may cover. */
export const CLOUD_RENDER_SCALE_CAP = 0.42;

/** Upstream weather refresh policy: 5 minutes stationary, 25 km of movement. */
export const WEATHER_REFRESH_MS = 5 * 60_000;
export const WEATHER_MOVE_REFRESH_M = 25_000;
/** How often the render loop re-checks the refresh gate (upstream lockstep). */
export const WEATHER_REFRESH_CHECK_MS = 1_000;

/** Frame-budget fps clamp bounds: cost 0 renders at the fastest bound. */
export const CLOUD_MIN_FPS = 12;
export const CLOUD_MAX_FPS = 15;

/** Below this eased strength the deck clears its canvas (upstream lockstep). */
export const CLOUD_VISIBLE_THRESHOLD = 0.035;
/** Per-render ease factor toward the target strength (upstream `blend`). */
export const CLOUD_STRENGTH_BLEND = 0.08;

/** A camera/anchor point for the weather gate; latitude/longitude in degrees. */
export interface CloudAnchorPoint {
  readonly latitude: number;
  readonly longitude: number;
}

/** Returns the capped framebuffer size used by the cockpit cloud pass. */
export function cockpitCloudRenderSize(
  width: number,
  height: number,
): {
  width: number;
  height: number;
} {
  const viewportWidth = Math.max(1, Number(width) || 1);
  const viewportHeight = Math.max(1, Number(height) || 1);
  const scale = Math.min(
    CLOUD_RENDER_SCALE_CAP,
    CLOUD_MAX_RENDER_WIDTH / viewportWidth,
    CLOUD_MAX_RENDER_HEIGHT / viewportHeight,
  );
  return {
    width: Math.max(1, Math.round(viewportWidth * scale)),
    height: Math.max(1, Math.round(viewportHeight * scale)),
  };
}

/** Return whether cockpit weather must refresh for elapsed time or movement. */
export function cockpitCloudRefreshDue({
  nowMs,
  fetchedAt,
  anchor,
  point,
  hasWeather,
}: {
  nowMs: number;
  fetchedAt: number;
  anchor: CloudAnchorPoint | null | undefined;
  point: CloudAnchorPoint | null | undefined;
  hasWeather: boolean;
}): boolean {
  if (!hasWeather) return true;
  if (!Number.isFinite(nowMs) || !Number.isFinite(fetchedAt)) return true;
  if (!anchor || !point) return true;
  return (
    nowMs - fetchedAt >= WEATHER_REFRESH_MS ||
    greatCircleM(
      { lat: anchor.latitude, lon: anchor.longitude },
      { lat: point.latitude, lon: point.longitude },
    ) >= WEATHER_MOVE_REFRESH_M
  );
}

function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}

/**
 * Frame-budget fps clamp: the kernel's deterministic `frameCost` hint in [0, 1]
 * mapped onto the 12–15 fps window (0 → fastest bound, 1 → slowest bound). Returns
 * the minimum milliseconds that must elapse between rendered frames.
 */
export function cloudFrameIntervalMs(frameCost: number): number {
  const cost = clamp01(frameCost);
  const fps = CLOUD_MAX_FPS + (CLOUD_MIN_FPS - CLOUD_MAX_FPS) * cost;
  return 1000 / fps;
}

/** Target deck strength for a weather profile at the camera's altitude. */
export function cloudStrength(
  profile: Pick<WeatherEffectProfile, 'available' | 'cloud'>,
  altitudeCloudFactor: number,
): number {
  if (!profile.available) return 0;
  return clamp01(profile.cloud * altitudeCloudFactor);
}

/**
 * One parallax band of the 2.5D deck. `altitude` places the band vertically
 * (0 = horizon, 1 = top of frame), `density` scales its coverage and opacity,
 * `drift` scales how fast the band translates with the wind, `depth` its
 * parallax size (near bands render larger and faster).
 */
export interface CloudDeckLayer {
  readonly altitude: number;
  readonly density: number;
  readonly drift: number;
  readonly depth: number;
}

/** Fog veil (design-token tinted wash) opacity in [0, 1]. */
export const cloudFogVeil = (profile: WeatherEffectProfile): number =>
  clamp01(Math.max(profile.fog * 0.85, profile.haze * 0.55));

/** Storm darkness (design-token scrim) opacity in [0, 1]. */
export const cloudDarkness = (profile: WeatherEffectProfile): number =>
  clamp01(profile.storm * 0.62 + profile.fog * 0.18);

/**
 * Derives the layered 2.5D deck from a weather profile: three parallax cloud
 * bands (far veil, main deck, low scud) whose densities follow the profile's
 * cloud cover, weighted by the precipitation mix. Clear weather yields bands
 * with zero density — the renderer then draws nothing at all.
 */
export function cloudDeckLayers(profile: WeatherEffectProfile): readonly CloudDeckLayer[] {
  const cover = clamp01(profile.cloud);
  const precipWeight = clamp01(0.55 * profile.rain + 0.8 * profile.snow + 0.45 * profile.storm);
  const deck = clamp01(cover * (0.72 + 0.28 * precipWeight));
  return [
    // Far veil: thin, slow, high.
    { altitude: 0.82, density: clamp01(cover * 0.42), drift: 0.35, depth: 0.55 },
    // Main deck: the body of the cloudscape.
    { altitude: 0.55, density: deck, drift: 0.7, depth: 1 },
    // Low scud: near, sparse, fast — sells the 2.5D parallax.
    { altitude: 0.3, density: clamp01(deck * 0.5 * (0.5 + precipWeight)), drift: 1.35, depth: 1.5 },
  ];
}

/** Wind drift scale constants (upstream `render()`: 0.008 + wind * 0.022). */
export const CLOUD_WIND_SCALE_BASE = 0.008;
export const CLOUD_WIND_SCALE_GAIN = 0.022;
/** Wrapping-track units per second the base wind scale translates a depth-1 band. */
export const CLOUD_DRIFT_TRACK_RATE = 2.5;

/**
 * Screen-space wind drift along a layer's wrapping track, in track units per
 * second: the x-component of upstream's wind uniform (`sin(dir) * scale`) with
 * the layer's parallax depth applied. Positive drifts rightward.
 */
export function cloudDriftRate(wind: number, windDirectionDeg: number, layerDrift: number): number {
  const dir = (((windDirectionDeg % 360) + 360) % 360) * (Math.PI / 180);
  const scale = CLOUD_WIND_SCALE_BASE + clamp01(wind) * CLOUD_WIND_SCALE_GAIN;
  return Math.sin(dir) * scale * CLOUD_DRIFT_TRACK_RATE * layerDrift;
}

/** Band count per layer: denser weather stacks more clouds (3–8). */
export function cloudBandCount(density: number): number {
  return 3 + Math.round(clamp01(density) * 5);
}

/**
 * Deterministic per-layer band placement for the renderer: `count` horizontal
 * centres on a wrapping unit track, offset by the wind drift accumulated since
 * t = 0. Pure so tests can pin the deck's layout at any timestamp.
 */
export function cloudBandOffsets(
  seed: number,
  count: number,
  driftUnits: number,
): readonly number[] {
  const bands: number[] = [];
  const safeCount = Math.max(0, Math.floor(count));
  for (let i = 0; i < safeCount; i++) {
    // Deterministic hash (frac(sin) lattice, same family as upstream's shader hash).
    const s = Math.sin((seed + i * 127.1) * 43.7585) * 43_758.5453;
    const base = s - Math.floor(s);
    bands.push((base + driftUnits) % 1);
  }
  return bands;
}
