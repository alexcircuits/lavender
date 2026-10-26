import { beforeAll, describe, expect, it } from 'vitest';
import {
  binGnssObservations,
  binGnssObservationsTs,
  classifyGnssLevel,
  classifyGnssLevelTs,
  classifyGnssObservation,
  classifyGnssObservationTs,
  discountGnssDegradedPercent,
  discountGnssDegradedPercentTs,
  type GnssObservation,
  loadLavenderWasm,
} from './wasmGnss';

describe('wasmGnss (Zig WASM GNSS Integrity & Binning Kernel)', () => {
  beforeAll(async () => {
    await loadLavenderWasm();
  });

  describe('DO-260A/B observation classification', () => {
    it('classifies gpsLost as degraded regardless of NIC/NACp', () => {
      expect(classifyGnssObservationTs(9, 10, true)).toBe(true);
      expect(classifyGnssObservationTs(null, null, true)).toBe(true);
      expect(classifyGnssObservation(9, 10, true)).toBe(true);
    });

    it('classifies NIC < 7 or NACp < 8 as degraded', () => {
      expect(classifyGnssObservationTs(6, 9, false)).toBe(true);
      expect(classifyGnssObservationTs(0, 10, false)).toBe(true);
      expect(classifyGnssObservationTs(8, 7, false)).toBe(true);
      expect(classifyGnssObservationTs(8, 0, false)).toBe(true);
      expect(classifyGnssObservation(6, 9, false)).toBe(true);
      expect(classifyGnssObservation(8, 7, false)).toBe(true);
    });

    it('classifies nominal NIC >= 7 and NACp >= 8 or missing fields as healthy', () => {
      expect(classifyGnssObservationTs(7, 8, false)).toBe(false);
      expect(classifyGnssObservationTs(11, 11, false)).toBe(false);
      expect(classifyGnssObservationTs(null, null, false)).toBe(false);
      expect(classifyGnssObservationTs(-1, -1, false)).toBe(false);
      expect(classifyGnssObservation(7, 8, false)).toBe(false);
      expect(classifyGnssObservation(null, null, false)).toBe(false);
    });
  });

  describe('1-aircraft false-positive discount (gpsjam.org formula)', () => {
    it('discounts the first degraded aircraft to 0%', () => {
      expect(discountGnssDegradedPercentTs(0, 0)).toBe(0);
      expect(discountGnssDegradedPercentTs(10, 0)).toBe(0);
      expect(discountGnssDegradedPercentTs(10, 1)).toBe(0);
      expect(discountGnssDegradedPercent(10, 1)).toBe(0);
    });

    it('computes 100 * (degraded - 1) / aircraft when degraded >= 2', () => {
      expect(discountGnssDegradedPercentTs(10, 2)).toBeCloseTo(10, 5);
      expect(discountGnssDegradedPercentTs(20, 5)).toBeCloseTo(20, 5);
      expect(discountGnssDegradedPercent(20, 5)).toBeCloseTo(20, 5);
    });
  });

  describe('anomaly severity level thresholds', () => {
    it('maps < 2% to low, 2%..10% to medium, and > 10% to high', () => {
      expect(classifyGnssLevelTs(0)).toBe('low');
      expect(classifyGnssLevelTs(1.99)).toBe('low');
      expect(classifyGnssLevelTs(2.0)).toBe('medium');
      expect(classifyGnssLevelTs(10.0)).toBe('medium');
      expect(classifyGnssLevelTs(10.01)).toBe('high');
      expect(classifyGnssLevelTs(50)).toBe('high');

      expect(classifyGnssLevel(1.5)).toBe('low');
      expect(classifyGnssLevel(5.0)).toBe('medium');
      expect(classifyGnssLevel(25.0)).toBe('high');
    });
  });

  describe('spatial cell binning', () => {
    it('bins observations into 0.5° cells, enforces minAircraft, and matches TS reference', () => {
      // Cell at lat [50.0, 50.5), lon [20.0, 20.5) -> row = (50+90)/0.5 = 280, col = (20+180)/0.5 = 400
      // 4 aircraft, 3 degraded -> discounted = 100 * (3 - 1) / 4 = 50% -> high
      // Cell at lat [10.0, 10.5), lon [10.0, 10.5) -> row = 200, col = 380
      // 20 aircraft, 2 degraded -> discounted = 100 * (2 - 1) / 20 = 5% -> medium
      // Cell at lat [-10.0, -9.5), lon [-10.0, -9.5) -> row = 160, col = 340
      // 5 aircraft, 1 degraded -> discounted = 0% -> low
      // Sparse cell with only 2 aircraft -> excluded by minAircraft = 3
      const observations: GnssObservation[] = [
        // High cell (280:400)
        { lat: 50.1, lon: 20.1, nic: 0, nacP: 0, gpsLost: false },
        { lat: 50.2, lon: 20.2, nic: 4, nacP: 5, gpsLost: false },
        { lat: 50.3, lon: 20.3, nic: 8, nacP: 9, gpsLost: true },
        { lat: 50.4, lon: 20.4, nic: 9, nacP: 10, gpsLost: false },
        // Medium cell (200:380): 2 degraded, 18 healthy
        { lat: 10.1, lon: 10.1, nic: 2, nacP: 3 },
        { lat: 10.2, lon: 10.2, nic: 5, nacP: 6 },
        ...Array.from({ length: 18 }, () => ({ lat: 10.25, lon: 10.25, nic: 9, nacP: 10 })),
        // Low cell (160:340): 1 degraded (discounted to 0%), 4 healthy
        { lat: -9.8, lon: -9.8, nic: 3, nacP: 4 },
        ...Array.from({ length: 4 }, () => ({ lat: -9.7, lon: -9.7, nic: 9, nacP: 10 })),
        // Sparse cell (only 2 aircraft)
        { lat: 0.1, lon: 0.1, nic: 0, nacP: 0 },
        { lat: 0.2, lon: 0.2, nic: 0, nacP: 0 },
        // Out-of-bounds / non-finite skipped
        { lat: 95, lon: 0, nic: 0, nacP: 0 },
        { lat: NaN, lon: 10, nic: 0, nacP: 0 },
      ];

      const tsCells = binGnssObservationsTs(observations, { cellDeg: 0.5, minAircraft: 3 });
      const wasmCells = binGnssObservations(observations, { cellDeg: 0.5, minAircraft: 3 });

      expect(tsCells).toHaveLength(3);
      expect(wasmCells).toEqual(tsCells);

      const lowCell = wasmCells.find((c) => c.id === '160:340');
      expect(lowCell).toBeDefined();
      expect(lowCell?.aircraft).toBe(5);
      expect(lowCell?.degraded).toBe(1);
      expect(lowCell?.percentDegraded).toBe(0);
      expect(lowCell?.level).toBe('low');
      expect(lowCell?.state).toBe('nominal');
      expect(lowCell?.interpretation).toBeUndefined();

      const medCell = wasmCells.find((c) => c.id === '200:380');
      expect(medCell).toBeDefined();
      expect(medCell?.aircraft).toBe(20);
      expect(medCell?.degraded).toBe(2);
      expect(medCell?.percentDegraded).toBe(5);
      expect(medCell?.level).toBe('medium');
      expect(medCell?.state).toBe('navigation-integrity-anomaly');
      expect(medCell?.interpretation).toBe('suspected-gnss-interference');

      const highCell = wasmCells.find((c) => c.id === '280:400');
      expect(highCell).toBeDefined();
      expect(highCell?.aircraft).toBe(4);
      expect(highCell?.degraded).toBe(3);
      expect(highCell?.percentDegraded).toBe(50);
      expect(highCell?.level).toBe('high');
      expect(highCell?.center.lat).toBeCloseTo(50.25, 5);
      expect(highCell?.center.lon).toBeCloseTo(20.25, 5);
      expect(highCell?.bbox).toEqual({
        west: 20,
        south: 50,
        east: 20.5,
        north: 50.5,
      });
    });

    it('returns empty array for empty or invalid inputs', () => {
      expect(binGnssObservations([])).toEqual([]);
      expect(binGnssObservations([{ lat: 10, lon: 10, degraded: true }], { cellDeg: 0 })).toEqual(
        [],
      );
    });
  });
});
