/**
 * Camera kinematics and trajectory shaping for flights and route dollies.
 *
 * Implements first-order smoothing (approachValue), banked coordinated turns
 * (routeBankTargetDeg), altitude breathing & lookahead turn lift
 * (routeAltitudeOffsetM), and closed-form C¹ trapezoid speed profiles (routeSpeedProfile).
 */

/** Mean AGL the dolly rides at — the altitude shaping oscillates AROUND this. */
export const ROUTE_CAMERA_HEIGHT_M = 260;
/** Locked look-down angle. */
export const ROUTE_PITCH_DEG = -32;
/** Seconds of ease-in (and again ease-out) at the ends of the flight. */
export const ROUTE_RAMP_S = 2.4;
/** Ramps never eat more than this share of a SHORT route's runtime. */
export const ROUTE_RAMP_MAX_FRACTION = 0.35;
/** Maximum bank roll in degrees. */
export const ROUTE_BANK_MAX_DEG = 10;
/** Degrees of bank per °/s of planned turn. */
export const ROUTE_BANK_PER_DEG_S = 0.44;
/** Zero-mean altitude breathing over long straights (metres). */
export const ROUTE_BREATH_M = 20;
export const ROUTE_BREATH_WAVELENGTH_M = 2200;
/** Extra height at full bank — the lookahead rise into a turn (metres). */
export const ROUTE_TURN_LIFT_M = 26;
/** Hard clearance above the sampled ground floor (metres). */
export const ROUTE_MIN_CLEARANCE_M = 90;

export const ROUTE_CINEMA = Object.freeze({
  meanHeightM: ROUTE_CAMERA_HEIGHT_M,
  pitchDeg: ROUTE_PITCH_DEG,
  maxBankDeg: ROUTE_BANK_MAX_DEG,
  minClearanceM: ROUTE_MIN_CLEARANCE_M,
  breathM: ROUTE_BREATH_M,
  turnLiftM: ROUTE_TURN_LIFT_M,
  rampS: ROUTE_RAMP_S,
});

/**
 * Frame-rate independent first-order approach (exponential smoothing).
 * `1 - e^(-rate·dt)` rather than `rate·dt` so a dropped frame cannot overshoot.
 * @param current Current value (NaN adopts the target).
 * @param target Value to approach.
 * @param ratePerS Approach rate, 1/s.
 * @param dt Seconds elapsed.
 * @returns The smoothed value.
 */
export function approachValue(
  current: number,
  target: number,
  ratePerS: number,
  dt: number,
): number {
  if (!Number.isFinite(current)) return target;
  if (!Number.isFinite(target)) return current;
  const k = 1 - Math.exp(-Math.max(0, ratePerS) * Math.max(0, dt));
  return current + (target - current) * k;
}

/**
 * Share of the flight's runtime spent easing in (and again easing out).
 * @param durationS Flight duration in seconds.
 * @returns Ramp fraction in [0, ROUTE_RAMP_MAX_FRACTION].
 */
export function routeRampFraction(durationS: number): number {
  if (!(durationS > 0)) return 0;
  return Math.min(ROUTE_RAMP_MAX_FRACTION, ROUTE_RAMP_S / durationS);
}

export interface RouteSpeedProfile {
  readonly distance: number;
  readonly speed: number;
}

/**
 * Trapezoid speed profile — smoothstep up, cruise, smoothstep down.
 *
 * Distance is the closed-form integral of the speed curve, so position is
 * exact (no per-frame accumulation drift) and speed is continuous everywhere:
 * zero at both ends, and C¹ across the ramp/plateau joins because smoothstep
 * has zero slope at 0 and 1.
 * @param u Normalized time, 0..1.
 * @param r Ramp fraction from routeRampFraction().
 * @returns Arc fraction 0..1, and speed as a multiple of the cruise speed.
 */
export function routeSpeedProfile(u: number, r: number): RouteSpeedProfile {
  const t = Math.min(1, Math.max(0, Number.isFinite(u) ? u : 0));
  if (!(r > 0)) return { distance: t, speed: 1 };
  const area = 1 - r; // ∫₀¹ v du — both ramps together contribute r
  const rampArea = (p: number) => r * (p * p * p - (p * p * p * p) / 2);
  const smoothstep = (p: number) => p * p * (3 - 2 * p);
  if (t <= r) {
    const p = t / r;
    return { distance: rampArea(p) / area, speed: smoothstep(p) };
  }
  if (t >= 1 - r) {
    const p = (1 - t) / r;
    return { distance: (area - rampArea(p)) / area, speed: smoothstep(p) };
  }
  return { distance: (r / 2 + (t - r)) / area, speed: 1 };
}

/**
 * Bank angle for a planned turn rate — proportional, capped, honest about zero.
 * @param turnRateDegS Planned turn rate, °/s (+ = turning right).
 * @param reducedMotion Flatten the roll entirely.
 * @param maxBankDeg Cap on bank angle in degrees (defaults to ROUTE_BANK_MAX_DEG).
 * @returns Degrees of roll, + = right wing down.
 */
export function routeBankTargetDeg(
  turnRateDegS: number,
  reducedMotion = false,
  maxBankDeg = ROUTE_BANK_MAX_DEG,
): number {
  if (reducedMotion || !Number.isFinite(turnRateDegS)) return 0;
  const raw = turnRateDegS * ROUTE_BANK_PER_DEG_S;
  return Math.max(-maxBankDeg, Math.min(maxBankDeg, raw));
}

/**
 * Altitude offset around the route's mean AGL: a zero-mean breath over long
 * straights, plus a small rise into turns for lookahead. Breathing fades out
 * as the bank comes in so the two never fight.
 * @param arcM Distance travelled along the route in metres.
 * @param bankFraction Current bank as a fraction of the cap, −1..1.
 * @param reducedMotion Flatten to a constant altitude.
 * @returns Metres to add to the mean AGL.
 */
export function routeAltitudeOffsetM(
  arcM: number,
  bankFraction: number,
  reducedMotion = false,
): number {
  if (reducedMotion) return 0;
  const bank = Number.isFinite(bankFraction) ? Math.max(-1, Math.min(1, bankFraction)) : 0;
  const straight = 1 - Math.abs(bank);
  const s = Number.isFinite(arcM) ? arcM : 0;
  const breath =
    Math.sin((2 * Math.PI * s) / ROUTE_BREATH_WAVELENGTH_M) * ROUTE_BREATH_M * straight;
  return breath + Math.abs(bank) * ROUTE_TURN_LIFT_M;
}

/**
 * Eye height above the ellipsoid.
 * Whatever the shaping asks for, the eye stays ROUTE_MIN_CLEARANCE_M above the floor.
 */
export function routeEyeHeightM(floorM: number, offsetM: number): number {
  const base = Number.isFinite(floorM) ? floorM : 0;
  const shaped = base + ROUTE_CAMERA_HEIGHT_M + (Number.isFinite(offsetM) ? offsetM : 0);
  return Math.max(base + ROUTE_MIN_CLEARANCE_M, shaped);
}
