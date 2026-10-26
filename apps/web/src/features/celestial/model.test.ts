import { describe, expect, it } from 'vitest';
import {
  CELESTIAL_PLANE_EPSILON,
  celestialScreenAngle,
  circularAngleDistance,
  earthDiscScreenRadius,
  GLOBE_ENTER_CLEARANCE_PX,
  GLOBE_EXIT_CLEARANCE_PX,
  isFullGlobeInsideKeyhole,
  normalizeAngle,
} from './model';

describe('celestial ring model (legacy port)', () => {
  it('normalises angles into [0, 2π)', () => {
    expect(normalizeAngle(-Math.PI / 2)).toBeCloseTo((3 * Math.PI) / 2, 12);
    expect(normalizeAngle(3 * Math.PI)).toBeCloseTo(Math.PI, 12);
    expect(normalizeAngle(Number.NaN)).toBe(0);
  });

  it('measures the shortest arc between angles', () => {
    expect(circularAngleDistance(0.1, 0.2)).toBeCloseTo(-0.1, 12);
    expect(circularAngleDistance(0, (3 * Math.PI) / 2)).toBeCloseTo(Math.PI / 2, 12);
  });

  it('projects the camera-plane bearing and fades unstable vectors', () => {
    const right = celestialScreenAngle(1, 0, 0);
    expect(right.angle).toBeCloseTo(0, 12);
    expect(right.stable).toBe(true);
    // Up component negates: screen bearings grow clockwise.
    const up = celestialScreenAngle(0, 1, 0);
    expect(up.angle).toBeCloseTo(-Math.PI / 2 + 2 * Math.PI, 10);
    // Aligned with the camera axis: keep the last bearing, fade out.
    const degenerate = celestialScreenAngle(0.001, 0.001, 1.23);
    expect(degenerate.stable).toBe(false);
    expect(degenerate.angle).toBeCloseTo(normalizeAngle(1.23), 12);
    expect(degenerate.opacity).toBeLessThan(1);
    expect(CELESTIAL_PLANE_EPSILON).toBe(0.045);
  });

  it('hides the ring until the disc clears the keyhole, with hysteresis', () => {
    const geometry = (clearance: number) => ({
      earthCenterX: 0,
      earthCenterY: 0,
      earthRadius: 100,
      keyholeCenterX: 0,
      keyholeCenterY: 0,
      keyholeRadius: 100 + clearance,
    });
    // Not visible yet: needs the larger enter clearance.
    expect(isFullGlobeInsideKeyhole(geometry(GLOBE_ENTER_CLEARANCE_PX), false)).toBe(true);
    expect(isFullGlobeInsideKeyhole(geometry(GLOBE_ENTER_CLEARANCE_PX - 1), false)).toBe(false);
    // Already visible: the smaller exit clearance keeps it shown.
    expect(isFullGlobeInsideKeyhole(geometry(GLOBE_EXIT_CLEARANCE_PX), true)).toBe(true);
    expect(isFullGlobeInsideKeyhole(geometry(GLOBE_EXIT_CLEARANCE_PX - 1), true)).toBe(false);
    // Degenerate geometry never shows the ring.
    expect(isFullGlobeInsideKeyhole(null, false)).toBe(false);
    expect(
      isFullGlobeInsideKeyhole(
        {
          earthCenterX: Number.NaN,
          earthCenterY: 0,
          earthRadius: 1,
          keyholeCenterX: 0,
          keyholeCenterY: 0,
          keyholeRadius: 500,
        },
        false,
      ),
    ).toBe(false);
  });

  it('derives the disc radius from angular size, upstream formula', () => {
    // At 4 earth radii the angular radius is asin(1/4) ≈ 14.48°.
    const r = earthDiscScreenRadius(4 * 6_378_137, 1000, (60 * Math.PI) / 180);
    expect(r).not.toBeNull();
    expect(r as number).toBeGreaterThan(0);
    // Inside/at the surface and degenerate FOVs are refused.
    expect(earthDiscScreenRadius(6_378_137, 1000, 1)).toBeNull();
    expect(earthDiscScreenRadius(10 * 6_378_137, 0, 1)).toBeNull();
    expect(earthDiscScreenRadius(10 * 6_378_137, 1000, Math.PI)).toBeNull();
  });
});
