import { describe, expect, it } from 'vitest';
import { deriveWeatherEffectProfileTs } from '../../globe/renderers/wasmSkyKinematics';
import {
  CLOUD_MAX_FPS,
  CLOUD_MAX_RENDER_HEIGHT,
  CLOUD_MAX_RENDER_WIDTH,
  CLOUD_MIN_FPS,
  CLOUD_VISIBLE_THRESHOLD,
  cloudBandCount,
  cloudBandOffsets,
  cloudDarkness,
  cloudDeckLayers,
  cloudDriftRate,
  cloudFogVeil,
  cloudFrameIntervalMs,
  cloudStrength,
  cockpitCloudRefreshDue,
  cockpitCloudRenderSize,
  WEATHER_MOVE_REFRESH_M,
  WEATHER_REFRESH_MS,
} from './cloudDeck';

const anchor = { latitude: 30, longitude: -97 };

describe('cockpitCloudRenderSize', () => {
  // Legacy parity vectors (legacy/src/cockpitCloudEffects.test.mjs).
  it('keeps the framebuffer low resolution on large displays', () => {
    expect(cockpitCloudRenderSize(2048, 1152)).toEqual({ width: 520, height: 293 });
    expect(cockpitCloudRenderSize(1280, 720)).toEqual({ width: 520, height: 293 });
  });

  it('never upscales or collapses to zero', () => {
    expect(cockpitCloudRenderSize(640, 360)).toEqual({ width: 269, height: 151 });
    expect(cockpitCloudRenderSize(0, Number.NaN)).toEqual({ width: 1, height: 1 });
  });

  it('respects the legacy caps', () => {
    expect(CLOUD_MAX_RENDER_WIDTH).toBe(520);
    expect(CLOUD_MAX_RENDER_HEIGHT).toBe(320);
  });
});

describe('cockpitCloudRefreshDue', () => {
  it('refreshes without weather, after 5 minutes, or after 25 km of movement', () => {
    expect(
      cockpitCloudRefreshDue({
        nowMs: 1000,
        fetchedAt: 500,
        anchor,
        point: anchor,
        hasWeather: false,
      }),
    ).toBe(true);
    expect(
      cockpitCloudRefreshDue({
        nowMs: 60_000,
        fetchedAt: 0,
        anchor,
        point: { latitude: 30.01, longitude: -97 },
        hasWeather: true,
      }),
    ).toBe(false);
    expect(
      cockpitCloudRefreshDue({
        nowMs: WEATHER_REFRESH_MS,
        fetchedAt: 0,
        anchor,
        point: anchor,
        hasWeather: true,
      }),
    ).toBe(true);
    expect(
      cockpitCloudRefreshDue({
        nowMs: 60_000,
        fetchedAt: 0,
        anchor,
        point: { latitude: 30.3, longitude: -97 },
        hasWeather: true,
      }),
    ).toBe(true);
  });

  it('treats missing anchors as due', () => {
    expect(
      cockpitCloudRefreshDue({
        nowMs: 1000,
        fetchedAt: 0,
        anchor: null,
        point: anchor,
        hasWeather: true,
      }),
    ).toBe(true);
    expect(WEATHER_MOVE_REFRESH_M).toBe(25_000);
  });
});

describe('cloudFrameIntervalMs (12–15 fps frame budget)', () => {
  it('maps cost 0 onto the fastest bound and cost 1 onto the slowest', () => {
    expect(cloudFrameIntervalMs(0)).toBeCloseTo(1000 / CLOUD_MAX_FPS, 6);
    expect(cloudFrameIntervalMs(1)).toBeCloseTo(1000 / CLOUD_MIN_FPS, 6);
  });

  it('clamps out-of-range costs and increases the interval with the cost', () => {
    expect(cloudFrameIntervalMs(-3)).toBe(cloudFrameIntervalMs(0));
    expect(cloudFrameIntervalMs(2)).toBe(cloudFrameIntervalMs(1));
    expect(cloudFrameIntervalMs(Number.NaN)).toBe(cloudFrameIntervalMs(0));
    expect(cloudFrameIntervalMs(0.9)).toBeGreaterThan(cloudFrameIntervalMs(0.2));
  });
});

describe('cloudStrength', () => {
  it('zeros unavailable weather and scales cloud cover by the altitude factor', () => {
    expect(cloudStrength({ available: false, cloud: 0.9 }, 1)).toBe(0);
    expect(cloudStrength({ available: true, cloud: 0.5 }, 1)).toBeCloseTo(0.5, 6);
    expect(cloudStrength({ available: true, cloud: 0.5 }, 0.2)).toBeCloseTo(0.1, 6);
  });
});

describe('cloudDeckLayers / fog / darkness', () => {
  it('derives three parallax bands, zero-density for clear weather', () => {
    const clear = deriveWeatherEffectProfileTs({ weatherCode: 0, cloudCoverPct: 4 });
    const layers = cloudDeckLayers(clear);
    expect(layers).toHaveLength(3);
    for (const layer of layers) expect(layer.density).toBe(0);

    const storm = deriveWeatherEffectProfileTs({ weatherCode: 95, precipitationMm: 12 });
    const stormLayers = cloudDeckLayers(storm);
    expect(stormLayers.every((l) => l.density > 0)).toBe(true);
    // The main deck carries the most coverage.
    expect(stormLayers[1]?.density).toBeGreaterThan(stormLayers[0]?.density ?? 0);
  });

  it('raises fog veil and storm darkness with the profile', () => {
    const clear = deriveWeatherEffectProfileTs({ weatherCode: 0, cloudCoverPct: 4 });
    expect(cloudFogVeil(clear)).toBe(0);
    expect(cloudDarkness(clear)).toBe(0);

    const fog = deriveWeatherEffectProfileTs({ weatherCode: 45, visibilityM: 600 });
    expect(cloudFogVeil(fog)).toBeGreaterThan(0.6);

    const storm = deriveWeatherEffectProfileTs({ weatherCode: 95, precipitationMm: 12 });
    expect(cloudDarkness(storm)).toBeGreaterThan(0.3);
    expect(cloudDarkness(storm)).toBeLessThanOrEqual(1);
  });
});

describe('cloudDriftRate / band layout', () => {
  it('drifts eastward for a westerly wind and stands still without wind', () => {
    // Wind FROM 90° (easterly): x-component sin(90°) > 0 → drifts right.
    expect(cloudDriftRate(0.8, 90, 1)).toBeGreaterThan(0);
    expect(cloudDriftRate(0, 90, 1)).toBeCloseTo(0.008 * 2.5, 9);
    // A north wind (from 0°) has no x-component.
    expect(cloudDriftRate(1, 0, 1)).toBeCloseTo(0, 9);
  });

  it('stacks 3–8 bands with density and lays them out deterministically', () => {
    expect(cloudBandCount(0)).toBe(3);
    expect(cloudBandCount(1)).toBe(8);
    const a = cloudBandOffsets(7, 5, 0.25);
    const b = cloudBandOffsets(7, 5, 0.25);
    expect(a).toEqual(b);
    expect(a).toHaveLength(5);
    for (const offset of a) {
      expect(offset).toBeGreaterThanOrEqual(0);
      expect(offset).toBeLessThan(1);
    }
    // Drift wraps around the unit track: offsets are absolute from the zero-drift
    // base (the renderer passes accumulated drift directly), so compare against it.
    const base = cloudBandOffsets(7, 5, 0);
    const drifted = cloudBandOffsets(7, 5, 3.75);
    expect(drifted[0]).toBeCloseTo(((base[0] ?? 0) + 3.75) % 1, 9);
  });
});

describe('visibility threshold parity', () => {
  it('matches the upstream cutoff', () => {
    expect(CLOUD_VISIBLE_THRESHOLD).toBeCloseTo(0.035, 6);
  });
});
