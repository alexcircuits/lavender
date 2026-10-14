import { describe, expect, it } from 'vitest';
import {
  annotationCentre,
  EARTH_RADIUS_M,
  geometryError,
  greatCircleM,
  measurement,
  pathLengthM,
  ringAreaM2,
} from './geometry';

describe('approximate spherical measurements', () => {
  it('matches the equatorial quarter-circle and sums directed path segments', () => {
    const p = [
      { lat: 0, lon: 0 },
      { lat: 0, lon: 90 },
      { lat: 0, lon: 180 },
    ];
    expect(
      greatCircleM(p[0] as { lat: number; lon: number }, p[1] as { lat: number; lon: number }),
    ).toBeCloseTo((Math.PI * EARTH_RADIUS_M) / 2, 5);
    expect(pathLengthM(p)).toBeCloseTo(Math.PI * EARTH_RADIUS_M, 5);
    expect(measurement('line', p)).toMatch(/^Approximate /);
  });
  it('uses the short dateline arc and centres across the seam', () => {
    const p = [
      { lat: 0, lon: 179 },
      { lat: 0, lon: -179 },
    ];
    expect(
      greatCircleM(p[0] as { lat: number; lon: number }, p[1] as { lat: number; lon: number }),
    ).toBeCloseTo((2 * Math.PI * EARTH_RADIUS_M) / 180, 4);
    expect(Math.abs(annotationCentre(p)?.lon ?? Number.NaN)).toBeCloseTo(180);
  });
  it('matches a spherical octant independently of winding', () => {
    const ring = [
      { lat: 0, lon: 0 },
      { lat: 0, lon: 90 },
      { lat: 90, lon: 0 },
    ];
    expect(ringAreaM2(ring)).toBeCloseTo((Math.PI * EARTH_RADIUS_M ** 2) / 2, 0);
    expect(ringAreaM2([...ring].reverse())).toBeCloseTo(ringAreaM2(ring), 0);
    expect(geometryError('area', ring)).toBeNull();
  });
  it('handles a polar ring and equivalent wrapped longitudes', () => {
    const polar = [
      { lat: 89, lon: 0 },
      { lat: 89, lon: 120 },
      { lat: 89, lon: -120 },
    ];
    expect(geometryError('area', polar)).toBeNull();
    expect(ringAreaM2(polar)).toBeGreaterThan(1e10);
    expect(ringAreaM2(polar)).toBeLessThan(2e10);
    expect(annotationCentre(polar)?.lat).toBeCloseTo(90);
    const seam = [
      { lat: 0, lon: 179.9 },
      { lat: 0, lon: -179.9 },
      { lat: 0.1, lon: -179.9 },
      { lat: 0.1, lon: 179.9 },
    ];
    const shifted = seam.map((p) => ({ ...p, lon: p.lon < 0 ? p.lon + 360 : p.lon }));
    expect(ringAreaM2(seam)).toBeCloseTo(ringAreaM2(shifted), 1);
    expect(geometryError('area', seam)).toBeNull();
  });
  it('refuses crossed, degenerate, duplicate, near-antipodal and hemisphere-ambiguous geometry', () => {
    expect(
      geometryError('area', [
        { lat: 0, lon: 0 },
        { lat: 1, lon: 1 },
        { lat: 0, lon: 1 },
        { lat: 1, lon: 0 },
      ]),
    ).toMatch(/cross/);
    expect(
      geometryError('area', [
        { lat: 0, lon: 0 },
        { lat: 0, lon: 1 },
        { lat: 0, lon: 2 },
      ]),
    ).toMatch(/surface/);
    expect(
      geometryError('line', [
        { lat: 0, lon: 0 },
        { lat: 0, lon: 0 },
      ]),
    ).toMatch(/apart/);
    expect(
      geometryError('line', [
        { lat: 0, lon: 0 },
        { lat: 0, lon: 180 },
      ]),
    ).toMatch(/antipodal/);
    expect(
      geometryError('area', [
        { lat: 0, lon: 0 },
        { lat: 0, lon: 120 },
        { lat: 0, lon: -120 },
      ]),
    ).toMatch(/hemisphere/);
  });
});
