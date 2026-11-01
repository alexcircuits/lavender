import { describe, expect, it } from 'vitest';
import {
  approachValue,
  ROUTE_BANK_MAX_DEG,
  ROUTE_BANK_PER_DEG_S,
  ROUTE_BREATH_M,
  ROUTE_BREATH_WAVELENGTH_M,
  ROUTE_CAMERA_HEIGHT_M,
  ROUTE_CINEMA,
  ROUTE_MIN_CLEARANCE_M,
  ROUTE_RAMP_MAX_FRACTION,
  ROUTE_TURN_LIFT_M,
  routeAltitudeOffsetM,
  routeBankTargetDeg,
  routeEyeHeightM,
  routeRampFraction,
  routeSpeedProfile,
} from './cameraCinematics';

describe('routeBankTargetDeg', () => {
  it('derives bank angle proportionally from planned turn rate', () => {
    // 10 deg/s * 0.44 = 4.4 deg
    expect(routeBankTargetDeg(10)).toBeCloseTo(10 * ROUTE_BANK_PER_DEG_S, 5);
    // -10 deg/s * 0.44 = -4.4 deg
    expect(routeBankTargetDeg(-10)).toBeCloseTo(-10 * ROUTE_BANK_PER_DEG_S, 5);
  });

  it('is honest about zero turn rate', () => {
    expect(routeBankTargetDeg(0)).toBe(0);
  });

  it('clamps bank angle to default maximum (ROUTE_BANK_MAX_DEG = 10)', () => {
    // Sharp turns saturate the cap without rolling past it
    expect(routeBankTargetDeg(50)).toBe(ROUTE_BANK_MAX_DEG);
    expect(routeBankTargetDeg(-50)).toBe(-ROUTE_BANK_MAX_DEG);
  });

  it('respects custom maxBankDeg cap (e.g. 25 deg)', () => {
    expect(routeBankTargetDeg(100, false, 25)).toBe(25);
    expect(routeBankTargetDeg(-100, false, 25)).toBe(-25);
    expect(routeBankTargetDeg(30, false, 25)).toBeCloseTo(30 * 0.44, 5);
  });

  it('zeros bank angle completely under reduced motion', () => {
    expect(routeBankTargetDeg(20, true)).toBe(0);
    expect(routeBankTargetDeg(-20, true)).toBe(0);
  });

  it('returns zero for non-finite turn rates', () => {
    expect(routeBankTargetDeg(Number.NaN)).toBe(0);
    expect(routeBankTargetDeg(Number.POSITIVE_INFINITY)).toBe(0);
  });
});

describe('approachValue exponential smoothing', () => {
  it('adopts target immediately when current is NaN (cold start)', () => {
    expect(approachValue(Number.NaN, 42, 2.0, 0.016)).toBe(42);
  });

  it('preserves current when target is NaN', () => {
    expect(approachValue(42, Number.NaN, 2.0, 0.016)).toBe(42);
  });

  it('is frame-rate independent across large vs small time steps', () => {
    const rate = 2.0;
    const totalDt = 0.5;
    const target = 100;

    // One big step of 0.5s
    const singleStep = approachValue(0, target, rate, totalDt);

    // 30 small steps summing to 0.5s
    let multiSteps = 0;
    const subDt = totalDt / 30;
    for (let i = 0; i < 30; i += 1) {
      multiSteps = approachValue(multiSteps, target, rate, subDt);
    }

    expect(Math.abs(singleStep - multiSteps)).toBeLessThan(1e-6);
  });

  it('approaches target without overshooting even after a stall (large dt)', () => {
    const afterStall = approachValue(0, 50, 2.0, 1000);
    expect(afterStall).toBeLessThanOrEqual(50);
    expect(afterStall).toBeGreaterThan(49.999);
  });

  it('returns current if dt <= 0 or rate <= 0', () => {
    expect(approachValue(20, 50, 2.0, 0)).toBe(20);
    expect(approachValue(20, 50, 2.0, -1)).toBe(20);
    expect(approachValue(20, 50, 0, 0.1)).toBe(20);
    expect(approachValue(20, 50, -1, 0.1)).toBe(20);
  });
});

describe('routeSpeedProfile', () => {
  it('is normalized, continuous, and zero at both ends', () => {
    const r = 0.2;
    const start = routeSpeedProfile(0, r);
    expect(start.distance).toBe(0);
    expect(start.speed).toBe(0);

    const end = routeSpeedProfile(1, r);
    expect(Math.abs(end.distance - 1)).toBeLessThan(1e-12);
    expect(end.speed).toBe(0);
  });

  it('reaches full cruise speed (1) in plateau region', () => {
    const r = 0.25;
    const cruise = routeSpeedProfile(0.5, r);
    expect(cruise.speed).toBe(1);
    expect(cruise.distance).toBeCloseTo(0.5, 5);
  });

  it('returns linear progress when ramp fraction r <= 0', () => {
    const profile = routeSpeedProfile(0.4, 0);
    expect(profile.distance).toBe(0.4);
    expect(profile.speed).toBe(1);
  });

  it('produces monotonically increasing distance', () => {
    const r = 0.2;
    let prevDistance = -1;
    for (let i = 0; i <= 50; i += 1) {
      const u = i / 50;
      const { distance } = routeSpeedProfile(u, r);
      expect(distance).toBeGreaterThanOrEqual(prevDistance);
      prevDistance = distance;
    }
  });
});

describe('routeAltitudeOffsetM', () => {
  it('is zero-mean breathing over one wavelength on straights (bankFraction = 0)', () => {
    const samples = 1000;
    let sum = 0;
    for (let i = 0; i < samples; i += 1) {
      const arc = (i / samples) * ROUTE_BREATH_WAVELENGTH_M;
      sum += routeAltitudeOffsetM(arc, 0);
    }
    const mean = sum / samples;
    expect(Math.abs(mean)).toBeLessThan(1e-3);
  });

  it('lifts the camera into turns for lookahead as bank increases', () => {
    const straightOffset = routeAltitudeOffsetM(0, 0);
    const halfTurnOffset = routeAltitudeOffsetM(0, 0.5);
    const fullTurnOffset = routeAltitudeOffsetM(0, 1.0);

    expect(halfTurnOffset).toBeGreaterThan(straightOffset);
    expect(fullTurnOffset).toBeCloseTo(ROUTE_TURN_LIFT_M, 5);
    // Negative bank (left turn) produces symmetric lift
    expect(routeAltitudeOffsetM(0, -1.0)).toBeCloseTo(ROUTE_TURN_LIFT_M, 5);
  });

  it('suppresses breathing at full bank so turns and breathing never fight', () => {
    const liftA = routeAltitudeOffsetM(0, 1.0);
    const liftB = routeAltitudeOffsetM(ROUTE_BREATH_WAVELENGTH_M / 4, 1.0);
    expect(liftA).toBe(liftB);
    expect(liftA).toBe(ROUTE_TURN_LIFT_M);
  });

  it('returns 0 under reduced motion', () => {
    expect(routeAltitudeOffsetM(500, 0.5, true)).toBe(0);
    expect(routeAltitudeOffsetM(1000, 1.0, true)).toBe(0);
  });
});

describe('routeEyeHeightM', () => {
  it('adds camera height and shaping offset to base floor', () => {
    const height = routeEyeHeightM(100, 15);
    expect(height).toBe(100 + ROUTE_CAMERA_HEIGHT_M + 15);
  });

  it('strictly respects the hard minimum clearance above ground', () => {
    // If shaping offset dips very low (-200m)
    const height = routeEyeHeightM(50, -200);
    expect(height).toBe(50 + ROUTE_MIN_CLEARANCE_M);
  });
});

describe('routeRampFraction and ROUTE_CINEMA', () => {
  it('caps ramp fraction to ROUTE_RAMP_MAX_FRACTION on short flights', () => {
    expect(routeRampFraction(1.0)).toBe(ROUTE_RAMP_MAX_FRACTION);
  });

  it('scales down ramp fraction on longer flights', () => {
    expect(routeRampFraction(24.0)).toBeCloseTo(2.4 / 24.0, 5);
  });

  it('returns 0 for non-positive durations', () => {
    expect(routeRampFraction(0)).toBe(0);
    expect(routeRampFraction(-5)).toBe(0);
  });

  it('exports frozen ROUTE_CINEMA descriptor with valid tuning values', () => {
    expect(ROUTE_CINEMA.meanHeightM).toBe(ROUTE_CAMERA_HEIGHT_M);
    expect(ROUTE_CINEMA.maxBankDeg).toBe(ROUTE_BANK_MAX_DEG);
    expect(ROUTE_CINEMA.breathM).toBe(ROUTE_BREATH_M);
    expect(ROUTE_CINEMA.turnLiftM).toBe(ROUTE_TURN_LIFT_M);
  });
});
