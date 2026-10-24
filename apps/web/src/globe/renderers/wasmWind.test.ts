import { beforeAll, describe, expect, it } from 'vitest';
import type { WindGridData } from '../../platform';
import {
  bakeAllStreamlinesWasm,
  bakeStreamlineWasm,
  clearWasmWindCache,
  DEGREE_METERS,
  GOLDEN_ANGLE,
  groupWindPaths,
  loadLavenderWasm,
  metersPerDegreeLon,
  normalizeLongitude,
  sampleWasmWind,
  WIND_CELL_DEGREES,
  WIND_PATH_LIMIT,
  WIND_PATH_STEPS,
} from './wasmWind';

function createUniformField(u = 12, v = 0): WindGridData {
  return {
    nx: 4,
    ny: 3,
    lo1: -180,
    la1: 90,
    dx: 90,
    dy: 90,
    di: 90,
    dj: 90,
    u: new Float32Array(12).fill(u),
    v: new Float32Array(12).fill(v),
  };
}

describe('wasmWind', () => {
  beforeAll(async () => {
    await loadLavenderWasm();
  });

  describe('normalizeLongitude', () => {
    it('normalizes angles to the half-open world interval [-180, 180)', () => {
      const eps = 1e-5;
      expect(normalizeLongitude(180)).toBeCloseTo(-180, eps);
      expect(normalizeLongitude(-180)).toBeCloseTo(-180, eps);
      expect(normalizeLongitude(0)).toBeCloseTo(0, eps);
      expect(normalizeLongitude(360)).toBeCloseTo(0, eps);
      expect(normalizeLongitude(270)).toBeCloseTo(-90, eps);
      expect(normalizeLongitude(-270)).toBeCloseTo(90, eps);
      expect(normalizeLongitude(190)).toBeCloseTo(-170, eps);
      expect(normalizeLongitude(-190)).toBeCloseTo(170, eps);
      expect(normalizeLongitude(540)).toBeCloseTo(-180, eps);
    });
  });

  describe('metersPerDegreeLon', () => {
    it('calculates geographic distance per longitude degree as a function of latitude', () => {
      expect(metersPerDegreeLon(0)).toBeCloseTo(DEGREE_METERS, 1);
      expect(metersPerDegreeLon(60)).toBeCloseTo(DEGREE_METERS * 0.5, 1);
      expect(metersPerDegreeLon(90)).toBeCloseTo(0, 1);
      expect(metersPerDegreeLon(-90)).toBeCloseTo(0, 1);
    });
  });

  describe('sampleWasmWind', () => {
    it('performs bilinear interpolation on a 0..360 grid matching known reference points', () => {
      const uData = new Float32Array([1.0, 2.0, 3.0, 4.0]);
      const vData = new Float32Array([5.0, 6.0, 7.0, 8.0]);
      const grid: WindGridData = {
        nx: 2,
        ny: 2,
        lo1: 0.0,
        la1: 90.0,
        dx: 180.0,
        dy: 90.0,
        di: 180.0,
        dj: 90.0,
        u: uData,
        v: vData,
      };

      const uv = { u: 0, v: 0 };

      // Corner (0, 90): top-left
      expect(sampleWasmWind(grid, 0.0, 90.0, uv)).toBe(true);
      expect(uv.u).toBeCloseTo(1.0, 4);
      expect(uv.v).toBeCloseTo(5.0, 4);

      // Corner (180, 90): top-right
      expect(sampleWasmWind(grid, 180.0, 90.0, uv)).toBe(true);
      expect(uv.u).toBeCloseTo(2.0, 4);
      expect(uv.v).toBeCloseTo(6.0, 4);

      // Midpoint (90, 45): interpolated center
      expect(sampleWasmWind(grid, 90.0, 45.0, uv)).toBe(true);
      expect(uv.u).toBeCloseTo(2.5, 4);
      expect(uv.v).toBeCloseTo(6.5, 4);

      // Bottom-left (0, 0)
      expect(sampleWasmWind(grid, 0.0, 0.0, uv)).toBe(true);
      expect(uv.u).toBeCloseTo(3.0, 4);
      expect(uv.v).toBeCloseTo(7.0, 4);

      // Bottom-right (180, 0)
      expect(sampleWasmWind(grid, 180.0, 0.0, uv)).toBe(true);
      expect(uv.u).toBeCloseTo(4.0, 4);
      expect(uv.v).toBeCloseTo(8.0, 4);
    });

    it('wraps longitude seamlessly across the 0 / 360 seam', () => {
      const uData = new Float32Array([1.0, 2.0, 3.0, 4.0]);
      const vData = new Float32Array([5.0, 6.0, 7.0, 8.0]);
      const grid: WindGridData = {
        nx: 2,
        ny: 2,
        lo1: 0.0,
        la1: 90.0,
        dx: 180.0,
        dy: 90.0,
        u: uData,
        v: vData,
      };

      const uvPos = { u: 0, v: 0 };
      const uvNeg = { u: 0, v: 0 };

      // In a 0..360 grid, 359.99° and -0.01° must produce matching interpolated values
      expect(sampleWasmWind(grid, 359.99, 90.0, uvPos)).toBe(true);
      expect(sampleWasmWind(grid, -0.01, 90.0, uvNeg)).toBe(true);
      expect(uvPos.u).toBeCloseTo(uvNeg.u, 2);
      expect(uvPos.v).toBeCloseTo(uvNeg.v, 2);
      expect(uvPos.u).toBeCloseTo(1.0, 2);
      expect(uvPos.v).toBeCloseTo(5.0, 2);

      // 360° and 0° should be strictly identical
      sampleWasmWind(grid, 360.0, 45.0, uvPos);
      sampleWasmWind(grid, 0.0, 45.0, uvNeg);
      expect(uvPos.u).toBeCloseTo(uvNeg.u, 5);
      expect(uvPos.v).toBeCloseTo(uvNeg.v, 5);
    });

    it('wraps longitude seamlessly across the -180 / 180 antimeridian seam', () => {
      // Grid starting at -180: columns at -180, -90, 0, 90
      const uData = new Float32Array([10, 20, 30, 40, 10, 20, 30, 40]);
      const vData = new Float32Array(8).fill(0);
      const grid180: WindGridData = {
        nx: 4,
        ny: 2,
        lo1: -180.0,
        la1: 90.0,
        dx: 90.0,
        dy: 90.0,
        u: uData,
        v: vData,
      };

      const uvNeg180 = { u: 0, v: 0 };
      const uvPos180 = { u: 0, v: 0 };

      // lon = -180 and lon = +180 are the exact same date-line meridian
      expect(sampleWasmWind(grid180, -180.0, 45.0, uvNeg180)).toBe(true);
      expect(sampleWasmWind(grid180, 180.0, 45.0, uvPos180)).toBe(true);
      expect(uvNeg180.u).toBeCloseTo(uvPos180.u, 4);
      expect(uvNeg180.v).toBeCloseTo(uvPos180.v, 4);
      expect(uvNeg180.u).toBeCloseTo(10.0, 4);

      // Continuity across seam: 179.9° and -179.9°
      const uvEast = { u: 0, v: 0 };
      const uvWest = { u: 0, v: 0 };
      expect(sampleWasmWind(grid180, 179.9, 90.0, uvEast)).toBe(true);
      expect(sampleWasmWind(grid180, -179.9, 90.0, uvWest)).toBe(true);
      expect(uvEast.u).toBeCloseTo(10.0, 1);
      expect(uvWest.u).toBeCloseTo(10.0, 1);
    });

    it('clamps latitude beyond poles without throwing', () => {
      const grid: WindGridData = {
        nx: 2,
        ny: 2,
        lo1: 0.0,
        la1: 90.0,
        dx: 180.0,
        dy: 90.0,
        u: new Float32Array([1.0, 2.0, 3.0, 4.0]),
        v: new Float32Array([5.0, 6.0, 7.0, 8.0]),
      };

      const uv = { u: 0, v: 0 };
      expect(sampleWasmWind(grid, 0.0, 100.0, uv)).toBe(true);
      expect(uv.u).toBeCloseTo(1.0, 4);
      expect(uv.v).toBeCloseTo(5.0, 4);

      expect(sampleWasmWind(grid, 0.0, -100.0, uv)).toBe(true);
      expect(uv.u).toBeCloseTo(3.0, 4);
      expect(uv.v).toBeCloseTo(7.0, 4);
    });

    it('rejects invalid inputs gracefully', () => {
      const grid: WindGridData = {
        nx: 2,
        ny: 2,
        u: new Float32Array(4),
        v: new Float32Array(4),
      };
      const uv = { u: 0, v: 0 };
      expect(sampleWasmWind(grid, Number.NaN, 0, uv)).toBe(false);
      expect(sampleWasmWind(grid, 0, Number.POSITIVE_INFINITY, uv)).toBe(false);
      expect(
        sampleWasmWind({ nx: 0, ny: 0, u: new Float32Array(0), v: new Float32Array(0) }, 0, 0, uv),
      ).toBe(false);
    });
  });

  describe('bakeStreamlineWasm', () => {
    it('traces ordered streamline points monotonically eastward along a uniform wind field', () => {
      const grid = createUniformField(12.0, 0.0);
      const outBuffer = new Float32Array(66); // 33 points * 2 coords

      const count = bakeStreamlineWasm(grid, 0.0, 0.0, 16, 1800.0, outBuffer);
      expect(count).toBe(33);

      // Seed center point is at index 16
      const centerLon = outBuffer[16 * 2 + 0] ?? 0;
      const centerLat = outBuffer[16 * 2 + 1] ?? 0;
      expect(centerLon).toBeCloseTo(0.0, 4);
      expect(centerLat).toBeCloseTo(0.0, 4);

      // Longitudes strictly progress eastward from upstream to downstream
      for (let i = 1; i < count; i++) {
        const prevLon = outBuffer[(i - 1) * 2 + 0] ?? 0;
        const curLon = outBuffer[i * 2 + 0] ?? 0;
        const curLat = outBuffer[i * 2 + 1] ?? 0;

        expect(curLon).toBeGreaterThan(prevLon);
        expect(Math.abs(curLon - prevLon)).toBeLessThan(1.0);
        expect(curLat).toBeCloseTo(0.0, 5);
      }

      // Upstream points (0..15) are west of seed (lon < 0)
      expect(outBuffer[0 * 2 + 0] ?? 0).toBeLessThan(0);
      // Downstream points (17..32) are east of seed (lon > 0)
      expect(outBuffer[32 * 2 + 0] ?? 0).toBeGreaterThan(0);
    });

    it('curves streamline points northward when northward wind component is present', () => {
      const uBend = new Float32Array(12).fill(20.0);
      const vBend = new Float32Array([
        -20.0, -10.0, 0.0, 10.0, -20.0, -10.0, 0.0, 10.0, -20.0, -10.0, 0.0, 10.0,
      ]);
      const bendGrid: WindGridData = {
        nx: 4,
        ny: 3,
        lo1: -180.0,
        la1: 90.0,
        dx: 90.0,
        dy: 90.0,
        u: uBend,
        v: vBend,
      };

      const outBuffer = new Float32Array(66);
      const count = bakeStreamlineWasm(bendGrid, 0.0, 0.0, 16, 1800.0, outBuffer);
      expect(count).toBe(33);

      const centerLon = outBuffer[16 * 2 + 0] ?? 0;
      const centerLat = outBuffer[16 * 2 + 1] ?? 0;
      expect(centerLon).toBeCloseTo(0.0, 4);
      expect(centerLat).toBeCloseTo(0.0, 4);

      // Both endpoints bend northward relative to center
      const startLat = outBuffer[0 * 2 + 1] ?? 0;
      const endLat = outBuffer[32 * 2 + 1] ?? 0;
      expect(startLat).toBeGreaterThan(centerLat);
      expect(endLat).toBeGreaterThan(centerLat);
    });

    it('returns 0 points for calm wind fields (< 0.15 m/s)', () => {
      const calmGrid = createUniformField(0.05, 0.05);
      const outBuffer = new Float32Array(66);
      const count = bakeStreamlineWasm(calmGrid, 0.0, 0.0, 16, 1800.0, outBuffer);
      expect(count).toBe(0);
    });

    it('rejects seeds beyond polar limits (|lat| > 88.5°)', () => {
      const grid = createUniformField(12.0, 0.0);
      const outBuffer = new Float32Array(66);
      expect(bakeStreamlineWasm(grid, 0.0, 89.0, 16, 1800.0, outBuffer)).toBe(0);
      expect(bakeStreamlineWasm(grid, 0.0, -89.0, 16, 1800.0, outBuffer)).toBe(0);
    });

    it('returns 0 when output buffer is too small (< 6 floats for 3 points)', () => {
      const grid = createUniformField(12.0, 0.0);
      const tinyBuffer = new Float32Array(4);
      expect(bakeStreamlineWasm(grid, 0.0, 0.0, 16, 1800.0, tinyBuffer)).toBe(0);
    });
  });

  describe('bakeAllStreamlinesWasm', () => {
    it('is deterministic and bounded by requested count and step limits', () => {
      const grid = createUniformField(12.0, 0.0);
      const first = bakeAllStreamlinesWasm(grid, { count: 30 });
      const second = bakeAllStreamlinesWasm(grid, { count: 30 });

      expect(first).toEqual(second);
      expect(first.length).toBe(30);

      for (const path of first) {
        expect(path.coordinates.length).toBeGreaterThanOrEqual(3);
        expect(path.coordinates.length).toBeLessThanOrEqual(WIND_PATH_STEPS * 2 + 1);

        for (let i = 1; i < path.coordinates.length; i++) {
          const prev = path.coordinates[i - 1] ?? [0, 0];
          const curr = path.coordinates[i] ?? [0, 0];
          expect(curr[0]).toBeGreaterThan(prev[0]);
          expect(Math.abs(curr[0] - prev[0])).toBeLessThan(1.0);
          expect(curr[1]).toBeCloseTo(prev[1], 4);
        }
      }
    });

    it('implements equal-area golden-angle distribution across sphere', () => {
      const grid = createUniformField(12.0, 0.0);
      const count = 120;
      const paths = bakeAllStreamlinesWasm(grid, { count });

      expect(paths.length).toBe(count);

      // Verify golden angle distribution of seeds:
      // Longitudes should be distributed uniformly across all 4 quadrants [-180..-90, -90..0, 0..90, 90..180]
      const quadrantCounts = [0, 0, 0, 0];
      // Equal-area latitude: sin(lat) is uniformly distributed
      // Quartiles of sin(lat) in [-sin(88°), sin(88°)]
      const latQuartileCounts = [0, 0, 0, 0];

      for (let i = 0; i < count; i++) {
        const seedLon = normalizeLongitude(i * GOLDEN_ANGLE);
        const sin88 = Math.sin((88 * Math.PI) / 180);
        const z = (((i + 0.5) / count) * 2 - 1) * sin88;
        const seedLat = (Math.asin(z) * 180) / Math.PI;

        // Longitude quadrant check
        if (seedLon >= -180 && seedLon < -90) {
          quadrantCounts[0] = (quadrantCounts[0] ?? 0) + 1;
        } else if (seedLon >= -90 && seedLon < 0) {
          quadrantCounts[1] = (quadrantCounts[1] ?? 0) + 1;
        } else if (seedLon >= 0 && seedLon < 90) {
          quadrantCounts[2] = (quadrantCounts[2] ?? 0) + 1;
        } else {
          quadrantCounts[3] = (quadrantCounts[3] ?? 0) + 1;
        }

        // Latitude z-quartile check (equal area in spherical projection)
        const normZ = (z / sin88 + 1) / 2; // [0, 1]
        const quartile = Math.min(3, Math.floor(normZ * 4));
        latQuartileCounts[quartile] = (latQuartileCounts[quartile] ?? 0) + 1;

        // Each path must contain its seed point [seedLon, seedLat]
        const path = paths[i];
        expect(path).toBeDefined();
        const hasSeedPoint = path?.coordinates.some(
          (pt) => Math.abs(pt[0] - seedLon) < 1e-3 && Math.abs(pt[1] - seedLat) < 1e-3,
        );
        expect(hasSeedPoint).toBe(true);
      }

      // In an equal-area low-discrepancy distribution, each quadrant has ~25% of seeds (around 30)
      for (const qCount of quadrantCounts) {
        expect(qCount).toBeGreaterThanOrEqual(20);
        expect(qCount).toBeLessThanOrEqual(40);
      }

      // In an equal-area distribution, each z-quartile has exactly equal count (30)
      for (const zCount of latQuartileCounts) {
        expect(zCount).toBe(30);
      }
    });

    it('enforces WIND_PATH_LIMIT and WIND_PATH_STEPS bounds on oversized options', () => {
      const grid = createUniformField(12.0, 0.0);
      const paths = bakeAllStreamlinesWasm(grid, { count: 10000, steps: 100 });
      expect(paths.length).toBeLessThanOrEqual(WIND_PATH_LIMIT);
      expect(paths.every((p) => p.coordinates.length <= 33)).toBe(true);
    });

    it('returns empty array for calm or invalid fields', () => {
      expect(bakeAllStreamlinesWasm(createUniformField(0.0, 0.0))).toEqual([]);
      expect(bakeAllStreamlinesWasm(createUniformField(Number.NaN, 1.0))).toEqual([]);
      expect(
        bakeAllStreamlinesWasm({
          nx: 0,
          ny: 0,
          u: new Float32Array(0),
          v: new Float32Array(0),
        }),
      ).toEqual([]);
    });

    it('clears cached grid upload on clearWasmWindCache', () => {
      clearWasmWindCache();
      const grid = createUniformField(12.0, 0.0);
      const paths = bakeAllStreamlinesWasm(grid, { count: 10 });
      expect(paths.length).toBe(10);
      clearWasmWindCache();
    });
  });

  describe('groupWindPaths', () => {
    it('groups baked paths into 30° spatial cells preserving all paths', () => {
      const grid = createUniformField(12.0, 0.0);
      const paths = bakeAllStreamlinesWasm(grid, { count: 50 });
      const groups = groupWindPaths(paths);

      expect(WIND_CELL_DEGREES).toBe(30);
      const totalGroupedPaths = groups.reduce((sum, g) => sum + g.paths.length, 0);
      expect(totalGroupedPaths).toBe(paths.length);

      // Verify grouping is deterministic
      expect(groupWindPaths(paths)).toEqual(groups);
    });

    it('throws RangeError for invalid cell sizes', () => {
      expect(() => groupWindPaths([], 0)).toThrow(RangeError);
      expect(() => groupWindPaths([], -10)).toThrow(RangeError);
      expect(() => groupWindPaths([], 200)).toThrow(RangeError);
      expect(() => groupWindPaths([], Number.NaN)).toThrow(RangeError);
    });
  });
});
