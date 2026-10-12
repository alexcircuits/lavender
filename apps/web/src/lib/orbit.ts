/**
 * Orbit figures for the inspector, from mean elements alone (no propagation): period from mean
 * motion, and approximate perigee/apogee heights from the two-body semi-major axis. SGP4 mean
 * motion is a Kozai mean, so these are approximations to a few kilometres — labelled as such.
 */

/** Earth's gravitational parameter (WGS84), km³/s². */
export const EARTH_MU_KM3_S2 = 398_600.4418;
/** WGS84 equatorial radius, km (heights are above this sphere, as orbit catalogues quote). */
export const EARTH_RADIUS_KM = 6_378.137;
const MINUTES_PER_DAY = 1_440;
const SECONDS_PER_DAY = 86_400;

/** Orbital period in minutes: 1 440 / mean motion (revolutions per day). */
export function orbitalPeriodMin(meanMotionRevPerDay: number): number | null {
  if (!Number.isFinite(meanMotionRevPerDay) || meanMotionRevPerDay <= 0) return null;
  return MINUTES_PER_DAY / meanMotionRevPerDay;
}

/** Two-body semi-major axis in km from mean motion: a = (μ / n²)^⅓. */
export function semiMajorAxisKm(meanMotionRevPerDay: number): number | null {
  if (!Number.isFinite(meanMotionRevPerDay) || meanMotionRevPerDay <= 0) return null;
  const n = (meanMotionRevPerDay * 2 * Math.PI) / SECONDS_PER_DAY;
  return Math.cbrt(EARTH_MU_KM3_S2 / (n * n));
}

export interface Apsides {
  /** Lowest height above the equatorial radius, km. */
  readonly perigeeKm: number;
  /** Highest height above the equatorial radius, km. */
  readonly apogeeKm: number;
}

/** Approximate perigee and apogee heights, or null for non-physical elements. */
export function apsides(meanMotionRevPerDay: number, eccentricity: number): Apsides | null {
  const a = semiMajorAxisKm(meanMotionRevPerDay);
  if (a === null || !Number.isFinite(eccentricity) || eccentricity < 0 || eccentricity >= 1) {
    return null;
  }
  return {
    perigeeKm: a * (1 - eccentricity) - EARTH_RADIUS_KM,
    apogeeKm: a * (1 + eccentricity) - EARTH_RADIUS_KM,
  };
}
