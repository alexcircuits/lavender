import { beforeAll, describe, expect, it } from 'vitest';
import {
  buildStarVertices,
  buildStarVerticesTs,
  bvToRgb,
  bvToTemperature,
  cockpitAnchorCorrectionStep,
  deriveWeatherEffectProfile,
  deriveWeatherEffectProfileTs,
  loadLavenderWasm,
  normalizeHeading,
  raDecToUnitVector,
  STAR_VERTEX_FLOATS,
  sampleCameraMove,
  slewHeading,
  starAppearance,
  starBrightness,
  weatherAltitudeFactors,
} from './wasmSkyKinematics';

describe('wasmSkyKinematics', () => {
  beforeAll(async () => {
    await loadLavenderWasm();
  });

  describe('starfield photometry', () => {
    it('converts RA/Dec to unit vector', () => {
      const [x, y, z] = raDecToUnitVector(0, 0);
      expect(x).toBeCloseTo(1, 5);
      expect(y).toBeCloseTo(0, 5);
      expect(z).toBeCloseTo(0, 5);

      const [px, py, pz] = raDecToUnitVector(0, 90);
      expect(px).toBeCloseTo(0, 5);
      expect(py).toBeCloseTo(0, 5);
      expect(pz).toBeCloseTo(1, 5);
    });

    it('calculates star brightness and appearance bounds', () => {
      expect(starBrightness(6.5)).toBeCloseTo(1, 5);
      expect(starBrightness(1.5)).toBeGreaterThan(1);

      const faint = starAppearance(6.5);
      expect(faint.sigmaPx).toBeCloseTo(0.55, 2);
      expect(faint.alpha).toBeCloseTo(0.22, 2);

      const bright = starAppearance(-1.46); // Sirius
      expect(bright.sigmaPx).toBeCloseTo(1.35, 1);
      expect(bright.alpha).toBeCloseTo(1.0, 2);
    });

    it('calculates B-V color temperature and RGB', () => {
      const tempSun = bvToTemperature(0.65);
      expect(tempSun).toBeGreaterThan(5000);
      expect(tempSun).toBeLessThan(6500);

      const [r, g, b] = bvToRgb(0.65);
      expect(Math.max(r, g, b)).toBeCloseTo(1.0, 5);
    });

    it('builds star vertex buffer with WASM matching TS reference', () => {
      // 2 stars: [ra, dec, vmag, bv]
      const stars = new Float32Array([
        0.0,
        0.0,
        1.0,
        0.2, // Star 1
        90.0,
        45.0,
        4.5,
        0.8, // Star 2
      ]);

      const ts = buildStarVerticesTs(stars);
      expect(ts.count).toBe(2);
      expect(ts.vertices.length).toBe(2 * STAR_VERTEX_FLOATS);

      const wasm = buildStarVertices(stars);
      expect(wasm.count).toBe(2);
      for (let i = 0; i < wasm.vertices.length; i++) {
        const wasmVal = wasm.vertices[i] ?? 0;
        const tsVal = ts.vertices[i] ?? 0;
        expect(wasmVal).toBeCloseTo(tsVal, 3);
      }
    });
  });

  describe('kinematics & cockpit navigation', () => {
    it('normalizes heading into [0, 360)', () => {
      expect(normalizeHeading(0)).toBe(0);
      expect(normalizeHeading(360)).toBe(0);
      expect(normalizeHeading(-90)).toBe(270);
      expect(normalizeHeading(450)).toBe(90);
    });

    it('slews heading along shortest arc', () => {
      expect(slewHeading(350, 10, 5)).toBe(355);
      expect(slewHeading(10, 350, 5)).toBe(5);
      expect(slewHeading(10, 350, 30)).toBe(350);
    });

    it('computes bounded cockpit anchor correction', () => {
      const step = cockpitAnchorCorrectionStep(100, 250, 0.016);
      expect(step).toBeGreaterThan(0);
      expect(step).toBeLessThan(100);
    });

    it('samples 6-DOF camera moves across dateline', () => {
      const move = {
        from: { lat: 0, lon: 170, alt: 1000, heading: 350, pitch: -30, roll: 0 },
        to: { lat: 10, lon: -170, alt: 2000, heading: 10, pitch: -45, roll: 0 },
        easing: 'linear' as const,
      };

      const mid = sampleCameraMove(move, 0.5);
      expect(mid.lat).toBeCloseTo(5, 4);
      expect(Math.abs(mid.lon)).toBeCloseTo(180, 4);
      expect(mid.alt).toBeCloseTo(1500, 4);
      expect(mid.heading === 0 || mid.heading === 360).toBe(true);
      expect(mid.pitch).toBeCloseTo(-37.5, 4);
    });
  });

  describe('weather effects', () => {
    it('handles clear / missing weather gracefully', () => {
      const clear = deriveWeatherEffectProfile(null);
      expect(clear.available).toBe(false);
      expect(clear.rain).toBe(0);
    });

    it('derives thunderstorm and rain strengths from WMO codes', () => {
      const storm = deriveWeatherEffectProfile({
        weatherCode: 95,
        precipitationMm: 12,
        windKph: 60,
        cloudCoverPct: 90,
      });

      expect(storm.available).toBe(true);
      expect(storm.storm).toBeGreaterThan(0.5);
      expect(storm.rain).toBeGreaterThan(0.6);
      expect(storm.cloud).toBeGreaterThan(0.8);
    });

    it('maps the frame-cost budget hint identically in WASM and TS fallback', () => {
      const observations = [
        { weatherCode: 95, precipitationMm: 12, windKph: 60, cloudCoverPct: 90 },
        { weatherCode: 53, cloudCoverPct: 40 },
        { weatherCode: 73, precipitationMm: 8, windKph: 90, cloudCoverPct: 90, visibilityM: 12000 },
        { weatherCode: 45, visibilityM: 900, cloudCoverPct: 60 },
        { weatherCode: 0, cloudCoverPct: 8, visibilityM: 24000 },
      ] as const;

      for (const weather of observations) {
        const wasm = deriveWeatherEffectProfile(weather);
        expect(wasm.available).toBe(true);
        expect(wasm.frameCost).toBeGreaterThanOrEqual(0);
        expect(wasm.frameCost).toBeLessThanOrEqual(1);
        // The WASM kernel and the pure-TS reference must agree on the hint.
        const tsFallback = deriveWeatherEffectProfileTs(weather);
        expect(wasm.frameCost).toBeCloseTo(tsFallback.frameCost, 5);
        // ... and on visibility fog (legacy semantics: a missing observation is
        // not a 0 m observation — missing visibility contributes no fog).
        expect(wasm.fog).toBeCloseTo(tsFallback.fog, 5);
      }

      // Missing visibility never reads as pea-soup fog: clear sky stays clear and
      // a storm's fog stays 0 even though its observation carries no visibility.
      const clear = deriveWeatherEffectProfile({ weatherCode: 0, cloudCoverPct: 4 });
      expect(clear.fog).toBe(0);
      expect(deriveWeatherEffectProfileTs({ weatherCode: 0, cloudCoverPct: 4 }).fog).toBe(0);
      const noVisStorm = deriveWeatherEffectProfile({ weatherCode: 95, precipitationMm: 12 });
      expect(noVisStorm.fog).toBe(0);
      expect(deriveWeatherEffectProfileTs({ weatherCode: 95, precipitationMm: 12 }).fog).toBe(0);

      // Missing weather costs nothing; heavier weather costs strictly more.
      expect(deriveWeatherEffectProfile(null).frameCost).toBe(0);
      const drizzle = deriveWeatherEffectProfile({ weatherCode: 53 });
      const storm = deriveWeatherEffectProfile({ weatherCode: 95, precipitationMm: 12 });
      const snow = deriveWeatherEffectProfile({ weatherCode: 73, precipitationMm: 8 });
      expect(drizzle.frameCost).toBeGreaterThan(0);
      expect(drizzle.frameCost).toBeLessThan(storm.frameCost);
      expect(snow.frameCost).toBeGreaterThan(drizzle.frameCost);
    });

    it('calculates weather altitude attenuation factors', () => {
      const seaLevel = weatherAltitudeFactors(0);
      expect(seaLevel.precipitation).toBe(1);
      expect(seaLevel.cloud).toBe(1);

      const highAlt = weatherAltitudeFactors(25000);
      expect(highAlt.precipitation).toBe(0);
      expect(highAlt.cloud).toBe(0);
    });
  });
});
