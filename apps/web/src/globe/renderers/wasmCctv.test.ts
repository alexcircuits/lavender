import { beforeAll, describe, expect, it } from 'vitest';
import { batchCctvWireframeWasm, computeCctvFrustumWasm } from './wasmCctv';
import { loadLavenderWasm } from './wasmPropagation';

describe('wasmCctv (Zig WASM Viewshed & Frustum Geometry)', () => {
  beforeAll(async () => {
    await loadLavenderWasm();
  });

  describe('computeCctvFrustumWasm', () => {
    it('computes 3D camera viewing frustum with correct mount and corners', () => {
      const pose = {
        lat: 51.5074,
        lon: -0.1278, // London
        groundAltM: 15,
        mountHeightM: 20,
        headingDeg: 90, // East
        pitchDeg: -10, // Pitched down
        fovDeg: 75,
        rangeM: 600,
      };

      const g = computeCctvFrustumWasm(pose);

      // Mount: altitude should be ground + mount = 35m
      expect(g.mount[0]).toBeCloseTo(-0.1278, 4);
      expect(g.mount[1]).toBeCloseTo(51.5074, 4);
      expect(g.mount[2]).toBe(35);

      // Cap center: heading east, lon should increase
      expect(g.capCenter[0]).toBeGreaterThan(g.mount[0]);
      // Pitched down, cap center alt should be below mount
      expect(g.capCenter[2]).toBeLessThan(g.mount[2]);

      // Top corners should be higher altitude than bottom corners
      expect(g.tl[2]).toBeGreaterThan(g.bl[2]);
      expect(g.tr[2]).toBeGreaterThan(g.br[2]);
    });

    it('handles negative pitches and zero heights safely', () => {
      const g = computeCctvFrustumWasm({
        lat: 0,
        lon: 0,
        rangeM: 100,
      });

      expect(g.mount[0]).toBe(0);
      expect(g.mount[1]).toBe(0);
      expect(g.mount[2]).toBe(24); // default mountHeight
    });
  });

  describe('batchCctvWireframeWasm', () => {
    it('generates flat 48-float vertex buffers for multiple cameras', () => {
      const cameras = [
        {
          lat: 40.7128,
          lon: -74.006,
          headingDeg: 180,
          rangeM: 500,
        },
        {
          lat: 34.0522,
          lon: -118.2437,
          headingDeg: 90,
          rangeM: 1000,
        },
      ];

      const wireframe = batchCctvWireframeWasm(cameras);

      // 2 cameras * 48 floats per camera = 96 floats
      expect(wireframe.length).toBe(96);

      // First camera mount point
      expect(wireframe[0]).toBeCloseTo(-74.006, 2);
      expect(wireframe[1]).toBeCloseTo(40.7128, 2);
      expect(wireframe[2]).toBe(24);

      // Second camera mount point starts at index 48
      expect(wireframe[48]).toBeCloseTo(-118.2437, 2);
      expect(wireframe[49]).toBeCloseTo(34.0522, 2);
      expect(wireframe[50]).toBe(24);
    });

    it('returns empty buffer for empty camera list', () => {
      expect(batchCctvWireframeWasm([]).length).toBe(0);
    });
  });
});
