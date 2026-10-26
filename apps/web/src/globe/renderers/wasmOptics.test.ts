import { beforeAll, describe, expect, it } from 'vitest';
import {
  calculateGsd,
  calculateGsdTs,
  calculateKeyholeGeometry,
  calculateKeyholeGeometryTs,
  calculateNiirs,
  calculateNiirsTs,
  calculateOna,
  calculateOnaTs,
  calculateScopeTerminusAlpha,
  calculateScopeTerminusAlphaTs,
  calculateSonarSweepGain,
  calculateSonarSweepGainTs,
  estimateSunElevation,
  estimateSunElevationTs,
  loadLavenderWasm,
} from './wasmOptics';

describe('wasmOptics', () => {
  beforeAll(async () => {
    await loadLavenderWasm();
  });

  describe('GSD & NIIRS sensor models', () => {
    it('computes GSD with WASM matching TS reference', () => {
      const altitudes = [500, 1500, 10000, 50000, 250000, 800000];
      for (const alt of altitudes) {
        const ts = calculateGsdTs(alt);
        const wasm = calculateGsd(alt);
        expect(wasm).toBeCloseTo(ts, 4);
      }
    });

    it('computes NIIRS with WASM matching TS reference', () => {
      const gsds = [0.05, 0.1, 0.375, 1.0, 5.0, 50.0];
      for (const gsd of gsds) {
        const ts = calculateNiirsTs(gsd);
        const wasm = calculateNiirs(gsd);
        expect(wasm).toBeCloseTo(ts, 3);
      }
    });
  });

  describe('Off-nadir angle (ONA)', () => {
    it('computes ONA with WASM matching TS reference', () => {
      const pitches = [-90, -60, -45, -30, 0, 15];
      for (const p of pitches) {
        const ts = calculateOnaTs(p);
        const wasm = calculateOna(p);
        expect(wasm).toBeCloseTo(ts, 3);
      }
    });
  });

  describe('Solar position estimation', () => {
    it('computes solar elevation matching TS reference', () => {
      const unixSec = 1774180800; // Noon UTC at equinox
      const coords = [
        { lat: 0, lon: 0 },
        { lat: 45, lon: 15 },
        { lat: -35, lon: 140 },
        { lat: 70, lon: -20 },
      ];
      for (const c of coords) {
        const ts = estimateSunElevationTs(c.lat, c.lon, unixSec);
        const wasm = estimateSunElevation(c.lat, c.lon, unixSec);
        expect(wasm).toBeCloseTo(ts, 2);
      }
    });
  });

  describe('Cyber Sonar sweep gain', () => {
    it('computes beam sweep gain matching TS reference', () => {
      const sweepHead = 120;
      const bearings = [100, 115, 125, 135, 140, 180, 300];
      for (const b of bearings) {
        const ts = calculateSonarSweepGainTs(b, sweepHead, 24, 10);
        const wasm = calculateSonarSweepGain(b, sweepHead, 24, 10);
        expect(wasm).toBeCloseTo(ts, 3);
      }
    });
  });

  describe('Scope mask terminus alpha and keyhole', () => {
    it('computes terminus alpha ramp matching TS reference', () => {
      const alts = [5_000_000, 7_000_000, 8_000_000, 8_500_000, 9_500_000, 11_000_000];
      for (const a of alts) {
        const ts = calculateScopeTerminusAlphaTs(a);
        const wasm = calculateScopeTerminusAlpha(a);
        expect(wasm).toBeCloseTo(ts, 2);
      }
    });

    it('computes keyhole geometry matching TS reference', () => {
      const ts = calculateKeyholeGeometryTs(1920, 1080, 0.11);
      const wasm = calculateKeyholeGeometry(1920, 1080, 0.11);
      expect(wasm.cx).toBeCloseTo(ts.cx, 2);
      expect(wasm.cy).toBeCloseTo(ts.cy, 2);
      expect(wasm.radiusPx).toBeCloseTo(ts.radiusPx, 2);
      expect(wasm.featherPx).toBeCloseTo(ts.featherPx, 2);
    });
  });
});
