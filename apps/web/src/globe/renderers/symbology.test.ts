import { describe, expect, it } from 'vitest';
import {
  aircraftAltitudeM,
  aircraftLabel,
  chooseLabelled,
  FIRE_MAX_PX,
  FIRE_MIN_PX,
  fireRampIndex,
  fireSizePx,
  hasValidPosition,
  isStale,
  LABEL_BUDGET,
  LABEL_MAX_HEIGHT_M,
  QUAKE_MAX_PX,
  QUAKE_MIN_PX,
  quakeRampIndex,
  quakeSizePx,
  trackRotationRad,
} from './symbology';

describe('aircraft altitude', () => {
  it('prefers geometric altitude and converts feet to metres', () => {
    expect(
      aircraftAltitudeM({ onGround: false, altitudeGeomFt: 35_000, altitudeBaroFt: 34_000 }),
    ).toBeCloseTo(10_668, 6);
    expect(aircraftAltitudeM({ onGround: false, altitudeBaroFt: 1_000 })).toBeCloseTo(304.8, 6);
  });

  it('is 0 on the ground or when unknown', () => {
    expect(aircraftAltitudeM({ onGround: true, altitudeGeomFt: 400 })).toBe(0);
    expect(aircraftAltitudeM({ onGround: false })).toBe(0);
    expect(aircraftAltitudeM({ onGround: false, altitudeBaroFt: Number.NaN })).toBe(0);
  });

  it('clamps implausible negative altitudes', () => {
    expect(aircraftAltitudeM({ onGround: false, altitudeBaroFt: -100_000 })).toBe(-500);
  });
});

describe('aircraft rotation', () => {
  it('turns clockwise tracks into counter-clockwise billboard rotation from north', () => {
    expect(trackRotationRad(0)).toBe(0);
    expect(trackRotationRad(90)).toBeCloseTo(-Math.PI / 2, 12);
    expect(trackRotationRad(180)).toBeCloseTo(-Math.PI, 12);
    expect(trackRotationRad(270)).toBeCloseTo((-3 * Math.PI) / 2, 12);
  });

  it('normalises out-of-range tracks and points unknown tracks north', () => {
    expect(trackRotationRad(450)).toBeCloseTo(-Math.PI / 2, 12);
    expect(trackRotationRad(-90)).toBeCloseTo((-3 * Math.PI) / 2, 12);
    expect(trackRotationRad(360)).toBe(0);
    expect(trackRotationRad(undefined)).toBe(0);
    expect(trackRotationRad(Number.NaN)).toBe(0);
  });
});

describe('earthquake symbology', () => {
  it('grows with magnitude, clamped to the size range', () => {
    expect(quakeSizePx(-1)).toBe(QUAKE_MIN_PX);
    expect(quakeSizePx(2.5)).toBeCloseTo(10.25, 9);
    expect(quakeSizePx(5)).toBeCloseTo(16.5, 9);
    expect(quakeSizePx(9.5)).toBe(QUAKE_MAX_PX);
    expect(quakeSizePx(Number.NaN)).toBe(QUAKE_MIN_PX);
    for (let m = 0; m < 9; m += 0.5)
      expect(quakeSizePx(m + 0.5)).toBeGreaterThanOrEqual(quakeSizePx(m));
  });

  it('maps magnitude onto the five-step hazard ramp', () => {
    expect([1, 2.9, 3, 3.9, 4, 4.9, 5, 5.9, 6, 8.2].map(quakeRampIndex)).toEqual([
      0, 0, 1, 1, 2, 2, 3, 3, 4, 4,
    ]);
    expect(quakeRampIndex(Number.NaN)).toBe(0);
  });
});

describe('fire symbology', () => {
  it('sizes dynamically based on count and maxFrp within [6, 24] px', () => {
    expect(fireSizePx(1, 0)).toBe(FIRE_MIN_PX);
    expect(fireSizePx(1, -5)).toBe(FIRE_MIN_PX);
    expect(fireSizePx(1, Number.NaN)).toBe(FIRE_MIN_PX);
    expect(fireSizePx(1, 10)).toBeGreaterThan(FIRE_MIN_PX);
    expect(fireSizePx(1, 100)).toBeGreaterThan(fireSizePx(1, 10));
    expect(fireSizePx(1, 5000)).toBeLessThanOrEqual(FIRE_MAX_PX);

    expect(fireSizePx(5, 50)).toBeGreaterThan(fireSizePx(1, 50));
    expect(fireSizePx(50, 50)).toBeGreaterThan(fireSizePx(5, 50));
    expect(fireSizePx(1000, 5000)).toBe(FIRE_MAX_PX);
  });

  it('maps FRP onto the five-step hazard ramp', () => {
    expect([0, 9.9, 10, 49.9, 50, 99.9, 100, 249.9, 250, 1000].map(fireRampIndex)).toEqual([
      0, 0, 1, 1, 2, 2, 3, 3, 4, 4,
    ]);
    expect(fireRampIndex(Number.NaN)).toBe(0);
  });
});

describe('stale treatment', () => {
  it('applies to stale and unavailable feeds only', () => {
    expect(isStale({ state: 'live' })).toBe(false);
    expect(isStale({ state: 'stale', ageS: 600 })).toBe(true);
    expect(isStale({ state: 'degraded', reason: 'partial', ageS: 30 })).toBe(false);
    expect(isStale({ state: 'unavailable', reason: 'down' })).toBe(true);
  });
});

describe('positions', () => {
  it('accepts finite positions inside the latitude range only', () => {
    expect(hasValidPosition({ position: { lat: 10, lon: 200 } })).toBe(true);
    expect(hasValidPosition({ position: { lat: 91, lon: 0 } })).toBe(false);
    expect(hasValidPosition({ position: { lat: 0, lon: Number.NaN } })).toBe(false);
  });
});

describe('label budget', () => {
  const candidates = Array.from({ length: 250 }, (_, i) => ({ id: `a${i}`, distanceSq: 250 - i }));

  it('labels the nearest aircraft up to the budget below the height limit', () => {
    const chosen = chooseLabelled(candidates, null, 200_000);
    expect(chosen.size).toBe(LABEL_BUDGET);
    expect(chosen.has('a249')).toBe(true); // nearest
    expect(chosen.has('a0')).toBe(false); // farthest
  });

  it('always labels the selected aircraft, on top of the budget', () => {
    const chosen = chooseLabelled(candidates, 'a0', 200_000);
    expect(chosen.has('a0')).toBe(true);
    expect(chosen.size).toBe(LABEL_BUDGET + 1);
  });

  it('labels only the selected aircraft above the height limit', () => {
    expect([...chooseLabelled(candidates, 'a7', LABEL_MAX_HEIGHT_M + 1)]).toEqual(['a7']);
    expect(chooseLabelled(candidates, null, 20_000_000).size).toBe(0);
  });

  it('ignores a selection that is not a candidate', () => {
    expect(chooseLabelled([], 'gone', 1_000).size).toBe(0);
  });

  it('labels with the callsign, else the registration', () => {
    expect(aircraftLabel({ callsign: ' BAW123 ', registration: 'G-ABCD' })).toBe('BAW123');
    expect(aircraftLabel({ callsign: '   ', registration: 'G-ABCD' })).toBe('G-ABCD');
    expect(aircraftLabel({})).toBeNull();
  });
});
