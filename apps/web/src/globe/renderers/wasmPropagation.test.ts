import { beforeAll, describe, expect, it } from 'vitest';
import type { SatelliteElements } from '../../contracts';
import fixture from './__fixtures__/iss-omm.json';
import { type EcfMetres, propagateEcf, siderealTime, toSatrec } from './propagation';
import {
  getActiveLavenderWasm,
  isLavenderWasmLoaded,
  loadLavenderWasm,
  propagateBatchWasm,
  propagateWasmEcf,
} from './wasmPropagation';

const iss = fixture.elements satisfies SatelliteElements;

const vanguard: SatelliteElements = {
  noradId: 5,
  name: 'VANGUARD 1',
  group: 'verification',
  epoch: '2000-06-27T18:50:19.733568Z',
  meanMotion: 10.82419157,
  eccentricity: 0.1859667,
  inclinationDeg: 34.2682,
  raanDeg: 348.7242,
  argOfPericenterDeg: 331.7664,
  meanAnomalyDeg: 19.3264,
  bstar: 0.28098e-4,
  meanMotionDot: 0.00000023,
  meanMotionDdot: 0,
};

function satrecOf(elements: SatelliteElements) {
  const satrec = toSatrec(elements);
  if (!satrec) throw new Error('fixture elements did not initialise');
  return satrec;
}

describe('wasmPropagation', () => {
  beforeAll(async () => {
    await loadLavenderWasm();
  });

  describe('WASM loader', () => {
    it('loads and initializes the WebAssembly kernel module', () => {
      expect(isLavenderWasmLoaded()).toBe(true);
      const wasm = getActiveLavenderWasm();
      expect(wasm).not.toBeNull();
      expect(wasm?.instance).toBeInstanceOf(WebAssembly.Instance);
      expect(wasm?.module).toBeInstanceOf(WebAssembly.Module);
      expect(wasm?.memory).toBeInstanceOf(WebAssembly.Memory);
    });

    it('returns the cached active context on subsequent calls', async () => {
      const first = getActiveLavenderWasm();
      const second = await loadLavenderWasm();
      expect(second).toBe(first);
    });
  });

  describe('propagateWasmEcf', () => {
    it('computes ECF coordinates matching propagation.ts within 1 millimetre at epoch', () => {
      const satrec = satrecOf(iss);
      const date = new Date(iss.epoch);
      const gmst = siderealTime(date);

      const jsOut: EcfMetres = { x: 0, y: 0, z: 0 };
      const wasmOut: EcfMetres = { x: 0, y: 0, z: 0 };

      const jsSuccess = propagateEcf(satrec, date, gmst, jsOut);
      const wasmSuccess = propagateWasmEcf(satrec, date, gmst, wasmOut);

      expect(jsSuccess).toBe(true);
      expect(wasmSuccess).toBe(true);

      const dx = Math.abs(wasmOut.x - jsOut.x);
      const dy = Math.abs(wasmOut.y - jsOut.y);
      const dz = Math.abs(wasmOut.z - jsOut.z);
      const distanceDiff = Math.hypot(dx, dy, dz);

      // 1 millimetre = 0.001 metres
      expect(distanceDiff).toBeLessThan(0.001);
      expect(dx).toBeLessThan(0.001);
      expect(dy).toBeLessThan(0.001);
      expect(dz).toBeLessThan(0.001);
    });

    it('agrees with propagation.ts within 1 millimetre over 60 minutes of propagation', () => {
      const satrec = satrecOf(iss);
      const epochMs = Date.parse(iss.epoch);

      // Check every 10 minutes over the first hour
      for (let min = 0; min <= 60; min += 10) {
        const date = new Date(epochMs + min * 60_000);
        const gmst = siderealTime(date);

        const jsOut: EcfMetres = { x: 0, y: 0, z: 0 };
        const wasmOut: EcfMetres = { x: 0, y: 0, z: 0 };

        const jsOk = propagateEcf(satrec, date, gmst, jsOut);
        const wasmOk = propagateWasmEcf(satrec, date, gmst, wasmOut);

        expect(jsOk).toBe(true);
        expect(wasmOk).toBe(true);

        const distanceDiff = Math.hypot(
          wasmOut.x - jsOut.x,
          wasmOut.y - jsOut.y,
          wasmOut.z - jsOut.z,
        );
        expect(distanceDiff).toBeLessThan(0.001);
      }
    });

    it('matches Vanguard 1 verification case within 1 millimetre', () => {
      const satrec = satrecOf(vanguard);
      const date = new Date(vanguard.epoch);
      const gmst = siderealTime(date);

      const jsOut: EcfMetres = { x: 0, y: 0, z: 0 };
      const wasmOut: EcfMetres = { x: 0, y: 0, z: 0 };

      expect(propagateEcf(satrec, date, gmst, jsOut)).toBe(true);
      expect(propagateWasmEcf(satrec, date, gmst, wasmOut)).toBe(true);

      const distanceDiff = Math.hypot(
        wasmOut.x - jsOut.x,
        wasmOut.y - jsOut.y,
        wasmOut.z - jsOut.z,
      );
      expect(distanceDiff).toBeLessThan(0.001);
    });

    it('returns false and leaves output unchanged for invalid satellites', () => {
      const satrec = satrecOf(iss);
      const invalidSatrec = { ...satrec, ecco: 1.5 };
      const out: EcfMetres = { x: 123, y: 456, z: 789 };

      const ok = propagateWasmEcf(invalidSatrec, new Date(iss.epoch), 0, out);
      expect(ok).toBe(false);
      expect(out).toEqual({ x: 123, y: 456, z: 789 });
    });
  });

  describe('propagateBatchWasm', () => {
    it('batch propagates an array of satellites into flat Float32Array buffers', () => {
      const date = new Date(iss.epoch);
      const gmst = siderealTime(date);
      const satrec = satrecOf(iss);

      const satellites = [iss, vanguard, iss];
      const count = satellites.length;
      const outCoords = new Float32Array(count * 3);

      const successCount = propagateBatchWasm(satellites, date, gmst, outCoords);
      expect(successCount).toBe(count);

      // Verify each satellite's coordinates match single propagation within single-precision tolerance
      const singleIss: EcfMetres = { x: 0, y: 0, z: 0 };
      expect(propagateWasmEcf(satrec, date, gmst, singleIss)).toBe(true);

      const issX = outCoords[0] ?? 0;
      const issY = outCoords[1] ?? 0;
      const issZ = outCoords[2] ?? 0;

      // Satellite 0 (ISS)
      expect(Math.abs(issX - Math.fround(singleIss.x))).toBeLessThanOrEqual(0.1);
      expect(Math.abs(issY - Math.fround(singleIss.y))).toBeLessThanOrEqual(0.1);
      expect(Math.abs(issZ - Math.fround(singleIss.z))).toBeLessThanOrEqual(0.1);

      // Satellite 2 (ISS again)
      expect(outCoords[6]).toBe(issX);
      expect(outCoords[7]).toBe(issY);
      expect(outCoords[8]).toBe(issZ);

      // Satellite 1 (Vanguard)
      const singleVanguard: EcfMetres = { x: 0, y: 0, z: 0 };
      expect(propagateWasmEcf(satrecOf(vanguard), date, gmst, singleVanguard)).toBe(true);
      const vanX = outCoords[3] ?? 0;
      const vanY = outCoords[4] ?? 0;
      const vanZ = outCoords[5] ?? 0;
      expect(Math.abs(vanX - Math.fround(singleVanguard.x))).toBeLessThanOrEqual(0.1);
      expect(Math.abs(vanY - Math.fround(singleVanguard.y))).toBeLessThanOrEqual(0.1);
      expect(Math.abs(vanZ - Math.fround(singleVanguard.z))).toBeLessThanOrEqual(0.1);
    });

    it('supports batch propagation with SatRec instances directly', () => {
      const date = new Date(iss.epoch);
      const gmst = siderealTime(date);
      const satrecIss = satrecOf(iss);
      const satrecVanguard = satrecOf(vanguard);

      const satrecs = [satrecIss, satrecVanguard];
      const outCoords = new Float32Array(satrecs.length * 3);

      const successCount = propagateBatchWasm(satrecs, date, gmst, outCoords);
      expect(successCount).toBe(2);

      const singleIss: EcfMetres = { x: 0, y: 0, z: 0 };
      propagateWasmEcf(satrecIss, date, gmst, singleIss);
      const issX = outCoords[0] ?? 0;
      const issY = outCoords[1] ?? 0;
      const issZ = outCoords[2] ?? 0;
      expect(Math.abs(issX - Math.fround(singleIss.x))).toBeLessThanOrEqual(0.1);
      expect(Math.abs(issY - Math.fround(singleIss.y))).toBeLessThanOrEqual(0.1);
      expect(Math.abs(issZ - Math.fround(singleIss.z))).toBeLessThanOrEqual(0.1);
    });

    it('returns 0 for empty array', () => {
      const outCoords = new Float32Array(0);
      const count = propagateBatchWasm([], new Date(), 0, outCoords);
      expect(count).toBe(0);
    });

    it('handles arrays with invalid satellites gracefully', () => {
      const date = new Date(iss.epoch);
      const gmst = siderealTime(date);
      const invalidSat: SatelliteElements = { ...iss, eccentricity: 1.5 };

      const satellites = [iss, invalidSat];
      const outCoords = new Float32Array(satellites.length * 3);

      const successCount = propagateBatchWasm(satellites, date, gmst, outCoords);
      expect(successCount).toBe(1);

      // First satellite should have non-zero coords
      const x0 = outCoords[0] ?? 0;
      const y0 = outCoords[1] ?? 0;
      const z0 = outCoords[2] ?? 0;
      expect(Math.hypot(x0, y0, z0)).toBeGreaterThan(6_000_000);
      // Second satellite should have zero coords
      expect(outCoords[3]).toBe(0);
      expect(outCoords[4]).toBe(0);
      expect(outCoords[5]).toBe(0);
    });
  });
});
