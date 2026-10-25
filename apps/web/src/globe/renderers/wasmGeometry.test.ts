import { beforeAll, describe, expect, it } from 'vitest';
import {
  deltaDecodeCoordinatesWasm,
  loadLavenderWasm,
  pointInPolygonWasm,
  simplifyRdpWasm,
} from './wasmGeometry';

describe('wasmGeometry (Zig WASM Vector Geometry Kernel)', () => {
  beforeAll(async () => {
    await loadLavenderWasm();
  });

  describe('deltaDecodeCoordinatesWasm', () => {
    it('decodes cumulative delta coordinates into geographic coordinates', () => {
      // [x0, y0, dx1, dy1, dx2, dy2]
      const deltas = [10000, 20000, 100, -200, -50, 300];
      const decoded = deltaDecodeCoordinatesWasm(deltas, 1000);

      expect(decoded.length).toBe(3);
      expect(decoded[0]?.[0]).toBeCloseTo(10.0, 5);
      expect(decoded[0]?.[1]).toBeCloseTo(20.0, 5);

      expect(decoded[1]?.[0]).toBeCloseTo(10.1, 5);
      expect(decoded[1]?.[1]).toBeCloseTo(19.8, 5);

      expect(decoded[2]?.[0]).toBeCloseTo(10.05, 5);
      expect(decoded[2]?.[1]).toBeCloseTo(20.1, 5);
    });

    it('handles empty delta input safely', () => {
      expect(deltaDecodeCoordinatesWasm([])).toEqual([]);
      expect(deltaDecodeCoordinatesWasm([10])).toEqual([]);
    });
  });

  describe('simplifyRdpWasm', () => {
    it('simplifies collinear points down to line endpoints', () => {
      const collinear: Array<[number, number]> = [
        [0.0, 0.0],
        [1.0, 1.0],
        [2.0, 2.0],
        [3.0, 3.0],
        [4.0, 4.0],
      ];

      const simplified = simplifyRdpWasm(collinear, 0.1);
      expect(simplified.length).toBe(2);
      expect(simplified[0]).toEqual([0.0, 0.0]);
      expect(simplified[1]).toEqual([4.0, 4.0]);
    });

    it('preserves vertices with sharp deviations', () => {
      const spiked: Array<[number, number]> = [
        [0.0, 0.0],
        [1.0, 0.0],
        [2.0, 5.0],
        [3.0, 0.0],
        [4.0, 0.0],
      ];

      const simplified = simplifyRdpWasm(spiked, 1.0);
      expect(simplified.length).toBe(3);
      expect(simplified[0]).toEqual([0.0, 0.0]);
      expect(simplified[1]).toEqual([2.0, 5.0]);
      expect(simplified[2]).toEqual([4.0, 0.0]);
    });

    it('returns original points if count <= 2', () => {
      const segment: Array<[number, number]> = [
        [10.0, 20.0],
        [15.0, 25.0],
      ];
      expect(simplifyRdpWasm(segment, 0.1)).toEqual(segment);
    });
  });

  describe('pointInPolygonWasm', () => {
    it('detects points strictly inside and outside polygon ring', () => {
      // Square from [-1, -1] to [1, 1]
      const box: Array<[number, number]> = [
        [-1.0, -1.0],
        [1.0, -1.0],
        [1.0, 1.0],
        [-1.0, 1.0],
      ];

      expect(pointInPolygonWasm(0.0, 0.0, box)).toBe(true);
      expect(pointInPolygonWasm(0.5, 0.5, box)).toBe(true);
      expect(pointInPolygonWasm(-0.5, -0.5, box)).toBe(true);

      expect(pointInPolygonWasm(2.0, 0.0, box)).toBe(false);
      expect(pointInPolygonWasm(0.0, 2.0, box)).toBe(false);
      expect(pointInPolygonWasm(-3.0, -3.0, box)).toBe(false);
    });

    it('detects points with GeoPoint vertex objects', () => {
      const geoBox = [
        { lon: 10, lat: 20 },
        { lon: 12, lat: 20 },
        { lon: 12, lat: 22 },
        { lon: 10, lat: 22 },
      ];

      expect(pointInPolygonWasm(11, 21, geoBox)).toBe(true);
      expect(pointInPolygonWasm(9, 21, geoBox)).toBe(false);
      expect(pointInPolygonWasm(11, 25, geoBox)).toBe(false);
    });
  });
});
