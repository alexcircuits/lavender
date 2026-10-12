import { describe, expect, it } from 'vitest';
import { apsides, orbitalPeriodMin, semiMajorAxisKm } from './orbit';

describe('orbit figures', () => {
  it('derives the period from mean motion', () => {
    expect(orbitalPeriodMin(15.5)).toBeCloseTo(92.903, 3);
    expect(orbitalPeriodMin(1.00273791)).toBeCloseTo(1436.07, 1); // geostationary
  });

  it('derives the semi-major axis from mean motion', () => {
    // A sidereal day's mean motion is the geostationary radius, 42 164 km.
    expect(semiMajorAxisKm(1.00273791)).toBeCloseTo(42_164, -1);
  });

  it('puts the ISS at about 400 km', () => {
    // ISS elements, 2024: n = 15.4996 rev/day, e = 0.0004.
    const iss = apsides(15.4996, 0.0004);
    expect(iss?.perigeeKm).toBeGreaterThan(400);
    expect(iss?.perigeeKm).toBeLessThan(425);
    expect(iss?.apogeeKm).toBeGreaterThan(iss?.perigeeKm ?? 0);
    expect((iss?.apogeeKm ?? 0) - (iss?.perigeeKm ?? 0)).toBeCloseTo(2 * 6795 * 0.0004, 0);
  });

  it('spreads a Molniya orbit from low perigee to high apogee', () => {
    const molniya = apsides(2.006, 0.74);
    expect(molniya?.perigeeKm).toBeLessThan(1_000);
    expect(molniya?.apogeeKm).toBeGreaterThan(39_000);
  });

  it('refuses non-physical elements', () => {
    expect(orbitalPeriodMin(0)).toBeNull();
    expect(orbitalPeriodMin(-1)).toBeNull();
    expect(semiMajorAxisKm(Number.NaN)).toBeNull();
    expect(apsides(15, 1)).toBeNull();
    expect(apsides(15, -0.1)).toBeNull();
  });
});
