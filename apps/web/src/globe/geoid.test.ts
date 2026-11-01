import { describe, expect, it, vi } from 'vitest';
import {
  ellipsoidalToMslDisplayM,
  ensureGeoidReady,
  geoidHeight,
  isGeoidReady,
  orthometricToEllipsoidal,
} from './geoid';

const TOLERANCE_M = 2.5;

const LONDON = { lat: 51.5072, lon: -0.1275, nExpected: 46.1 };
const AUSTIN = { lat: 30.2672, lon: -97.7431, nExpected: -26.9 };
const SF = { lat: 37.7749, lon: -122.4194, nExpected: -32.2 };
const DENVER = { lat: 39.7392, lon: -104.9903, nExpected: -17.3 };
/** SFO runway 28R touchdown area — the cockpit/OSD field report's coordinates. */
const SFO = { lat: 37.616, lon: -122.368 };

describe('geoid', () => {
  it('geoidHeight throws a clear error if called before the grid is ready', async () => {
    vi.resetModules();
    const fresh = await import('./geoid');
    expect(() => fresh.geoidHeight(0, 0)).toThrow(/not ready|ensureGeoidReady/i);
  });

  it('ensureGeoidReady() resolves and is idempotent (safe to call repeatedly)', async () => {
    await ensureGeoidReady();
    await ensureGeoidReady();
    await ensureGeoidReady();
    expect(isGeoidReady()).toBe(true);
  });

  it('geoidHeight matches known EGM96 undulation values within ±2.5 m', async () => {
    await ensureGeoidReady();
    for (const { lat, lon, nExpected } of [LONDON, AUSTIN, SF, DENVER]) {
      const n = geoidHeight(lat, lon);
      expect(Math.abs(n - nExpected)).toBeLessThanOrEqual(TOLERANCE_M);
    }
  });

  it('orthometricToEllipsoidal adds the geoid undulation to the MSL height', async () => {
    await ensureGeoidReady();
    const hMslM = 15;
    const hEllipsoidal = orthometricToEllipsoidal(hMslM, LONDON.lat, LONDON.lon);
    // London geoid ≈ +46.1 -> 15 + 46.1 = 61.1, expect ≈ 61 within tolerance.
    expect(Math.abs(hEllipsoidal - 61)).toBeLessThanOrEqual(TOLERANCE_M);
    const n = geoidHeight(LONDON.lat, LONDON.lon);
    expect(hEllipsoidal).toBe(hMslM + n);
  });

  it('geoidHeight wraps longitude consistently (359.87 === -0.13)', async () => {
    await ensureGeoidReady();
    const wrapped = geoidHeight(51.5, 359.87);
    const normal = geoidHeight(51.5, -0.13);
    expect(Math.abs(wrapped - normal)).toBeLessThan(1e-9);
  });

  it('the SFO deck case: an ellipsoidal height equal to N reads as 0 m MSL', async () => {
    await ensureGeoidReady();
    const n = geoidHeight(SFO.lat, SFO.lon);
    expect(ellipsoidalToMslDisplayM(n, n)).toBe(0);
  });

  it('the reported SFO cockpit OSD height turns into a small positive MSL number', async () => {
    await ensureGeoidReady();
    const n = geoidHeight(SFO.lat, SFO.lon);
    expect(n).toBeLessThan(-25);
    expect(n).toBeGreaterThan(-40);
    const displayed = ellipsoidalToMslDisplayM(-15, n);
    expect(displayed).toBeGreaterThan(10);
    expect(displayed).toBeLessThan(25);
  });

  it('a positive undulation lowers the readout — the correction subtracts N', async () => {
    await ensureGeoidReady();
    const n = geoidHeight(LONDON.lat, LONDON.lon);
    expect(n).toBeGreaterThan(40);
    const cruise = ellipsoidalToMslDisplayM(10000, n);
    expect(cruise).toBe(10000 - n);
    expect(cruise).toBeGreaterThan(9950);
    expect(cruise).toBeLessThan(9960);
  });

  it('ellipsoidalToMslDisplayM round-trips orthometricToEllipsoidal', async () => {
    await ensureGeoidReady();
    for (const point of [LONDON, AUSTIN, SF, DENVER]) {
      const n = geoidHeight(point.lat, point.lon);
      const ellipsoidal = orthometricToEllipsoidal(120, point.lat, point.lon);
      expect(Math.abs(ellipsoidalToMslDisplayM(ellipsoidal, n) - 120)).toBeLessThan(1e-9);
    }
  });

  it('an unavailable geoid returns the uncorrected height, never NaN or blank', () => {
    for (const missing of [null, undefined, Number.NaN, 'x' as unknown as number]) {
      expect(ellipsoidalToMslDisplayM(-15, missing)).toBe(-15);
    }
  });

  it('a non-finite height is passed through untouched', async () => {
    await ensureGeoidReady();
    const n = geoidHeight(SFO.lat, SFO.lon);
    expect(ellipsoidalToMslDisplayM(null, n)).toBeNull();
    expect(ellipsoidalToMslDisplayM(undefined, n)).toBeUndefined();
    expect(Number.isNaN(ellipsoidalToMslDisplayM(Number.NaN, n))).toBe(true);
  });
});
