import { describe, expect, it } from 'vitest';
import type { SatelliteElements } from '../../contracts';
import fixture from './__fixtures__/iss-omm.json';
import {
  type EcfMetres,
  elementsKey,
  epochMillis,
  propagateEcf,
  siderealTime,
  subSatellitePoint,
  toSatrec,
} from './propagation';

const iss = fixture.elements satisfies SatelliteElements;

/**
 * Reference sub-satellite points for the fixture, computed with the Rust core's own SGP4:
 * `lavender_geo::orbit::Propagator::position` (sgp4 crate 2.4.0, WGS84 constants, IAU GMST,
 * Bowring geodetic), run on 2026-10-12 from a scratch binary against this repository's
 * crates. satellite.js uses the WGS72 SGP4 constants, so agreement is expected to tens of
 * metres, not bit-for-bit: tolerance 0.002° in latitude/longitude (~220 m) and 100 m in
 * altitude.
 */
const RUST_REFERENCE = [
  { time: '2024-03-08T12:00:00Z', lat: 51.462377, lon: -0.358535, altM: 417_885.3 },
  { time: '2024-03-08T12:30:00Z', lat: -24.743479, lon: 94.421401, altM: 421_943.4 },
  { time: '2024-03-08T13:00:00Z', lat: -24.561463, lon: -135.549327, altM: 426_900.9 },
  { time: '2024-03-08T13:30:00Z', lat: 51.473855, lon: -40.802683, altM: 418_458.2 },
  { time: '2024-03-08T22:00:00Z', lat: -51.223075, lon: 6.724143, altM: 436_085.8 },
] as const;
const DEG_TOLERANCE = 0.002;
const ALT_TOLERANCE_M = 100;

function satrecOf(elements: SatelliteElements) {
  const satrec = toSatrec(elements);
  if (!satrec) throw new Error('fixture elements did not initialise');
  return satrec;
}

describe('satellite propagation', () => {
  it('propagates the fixture times the Rust core is cross-checked at', () => {
    expect(RUST_REFERENCE.map((r) => r.time)).toEqual(fixture.times);
  });

  it.each(RUST_REFERENCE)('agrees with lavender-geo at $time', ({ time, lat, lon, altM }) => {
    const point = subSatellitePoint(satrecOf(iss), new Date(time));
    expect(point).not.toBeNull();
    expect(Math.abs((point?.lat ?? Number.NaN) - lat)).toBeLessThan(DEG_TOLERANCE);
    expect(Math.abs((point?.lon ?? Number.NaN) - lon)).toBeLessThan(DEG_TOLERANCE);
    expect(Math.abs((point?.altM ?? Number.NaN) - altM)).toBeLessThan(ALT_TOLERANCE_M);
  });

  it('keeps the ISS at ISS altitude and inside its inclination over one orbit', () => {
    const satrec = satrecOf(iss);
    const start = Date.parse(iss.epoch);
    const altitudesKm: number[] = [];
    for (let s = 0; s <= 93 * 60; s += 60) {
      const point = subSatellitePoint(satrec, new Date(start + s * 1000));
      expect(point).not.toBeNull();
      if (!point) continue;
      altitudesKm.push(point.altM / 1000);
      // Geodetic latitude may exceed the 51.64° inclination by up to ~0.2°.
      expect(Math.abs(point.lat)).toBeLessThanOrEqual(52);
    }
    const mean = altitudesKm.reduce((a, b) => a + b, 0) / altitudesKm.length;
    expect(mean).toBeGreaterThanOrEqual(400);
    expect(mean).toBeLessThanOrEqual(430);
    // Ellipsoid flattening and eccentricity spread the geodetic altitude around the mean.
    expect(Math.min(...altitudesKm)).toBeGreaterThan(400);
    expect(Math.max(...altitudesKm)).toBeLessThan(440);
  });

  it('matches the Vallado SGP4 verification case for satellite 00005 (TEME, km)', () => {
    // Vallado, Crawford, Hujsak & Kelso (2006), "Revisiting Spacetrack Report #3",
    // sgp4-ver.tle / tcppver.out: 00005 at t = 0 and t = 360 min. The OMM epoch carries
    // milliseconds only (0.6 ms short of the TLE epoch), hence the 10 m tolerance.
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
    const satrec = satrecOf(vanguard);
    const epoch = epochMillis(vanguard.epoch);
    const cases = [
      { minutes: 0, teme: [7022.46529266, -1400.08296755, 0.03995155] },
      { minutes: 360, teme: [-7154.03120202, -3783.17682504, -3536.19412294] },
    ];
    for (const { minutes, teme } of cases) {
      const date = new Date(epoch + minutes * 60_000);
      // ECF → TEME is a rotation about z by GMST; compare in TEME via the inverse rotation.
      const out: EcfMetres = { x: 0, y: 0, z: 0 };
      const gmst = siderealTime(date);
      expect(propagateEcf(satrec, date, gmst, out)).toBe(true);
      const [c, s] = [Math.cos(gmst), Math.sin(gmst)];
      const temeKm = [(c * out.x - s * out.y) / 1000, (s * out.x + c * out.y) / 1000, out.z / 1000];
      temeKm.forEach((value, i) => {
        expect(Math.abs(value - (teme[i] ?? Number.NaN))).toBeLessThan(0.01);
      });
    }
  });

  it('reads zone-less OMM epochs as UTC', () => {
    expect(epochMillis('2024-03-08T12:00:00')).toBe(Date.UTC(2024, 2, 8, 12));
    expect(epochMillis('2024-03-08T12:00:00.000000Z')).toBe(Date.UTC(2024, 2, 8, 12));
    expect(epochMillis('2024-03-08T13:00:00+01:00')).toBe(Date.UTC(2024, 2, 8, 12));
  });

  it('rejects unusable element sets instead of throwing', () => {
    expect(toSatrec({ ...iss, eccentricity: 1.2 })).toBeNull();
    expect(toSatrec({ ...iss, meanMotion: 0 })).toBeNull();
    expect(toSatrec({ ...iss, epoch: 'not a date' })).toBeNull();
    expect(toSatrec({ ...iss, inclinationDeg: Number.NaN })).toBeNull();
  });

  it('keys element sets by their orbital content', () => {
    expect(elementsKey(iss)).toBe(elementsKey({ ...iss, name: 'renamed' }));
    expect(elementsKey(iss)).not.toBe(elementsKey({ ...iss, epoch: '2024-03-09T00:00:00Z' }));
  });
});
