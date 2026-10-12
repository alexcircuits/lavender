/**
 * SGP4 propagation of Lavender's mean elements with satellite.js (MIT). Elements arrive as the
 * OMM-derived `SatelliteElements` contract and are fed to `json2satrec` in OMM field names, so
 * the client and the Rust core (lavender-geo, `sgp4` crate) start from the same numbers.
 *
 * satellite.js uses the standard WGS72 SGP4 constants; lavender-geo uses WGS84. At map scale
 * they agree to tens of metres (see propagation.test.ts).
 */
import {
  eciToEcf,
  eciToGeodetic,
  gstime,
  json2satrec,
  type OMMJsonObject,
  type PositionAndVelocity,
  type PropagateOptions,
  propagate,
  type SatRec,
  SatRecError,
} from 'satellite.js';
import type { SatelliteElements } from '../../contracts';

const RAD_TO_DEG = 180 / Math.PI;
/** OMM epochs are UTC; CelesTrak omits the zone designator, which JS would read as local. */
const HAS_ZONE = /(?:[zZ]|[+-]\d{2}:?\d{2})$/;
/**
 * Plausible geocentric distances (km), a second guard after satellite.js's community decay
 * check: a diverged propagation is hidden rather than drawn inside the Earth or far beyond
 * the Moon.
 */
const MIN_RADIUS_KM = 6_300;
const MAX_RADIUS_KM = 500_000;

export type { SatRec } from 'satellite.js';

/** Earth-fixed position in metres. */
export interface EcfMetres {
  x: number;
  y: number;
  z: number;
}

export interface SubSatellitePoint {
  readonly lat: number;
  readonly lon: number;
  readonly altM: number;
}

/** A string that changes whenever the element set does (recompute the satrec only then). */
export function elementsKey(e: SatelliteElements): string {
  return [
    e.epoch,
    e.meanMotion,
    e.eccentricity,
    e.inclinationDeg,
    e.raanDeg,
    e.argOfPericenterDeg,
    e.meanAnomalyDeg,
    e.bstar,
  ].join('|');
}

export function toOmm(e: SatelliteElements, epochIso: string): OMMJsonObject {
  return {
    OBJECT_NAME: e.name,
    OBJECT_ID: e.intlDesignator ?? '',
    EPOCH: epochIso,
    MEAN_MOTION: e.meanMotion,
    ECCENTRICITY: e.eccentricity,
    INCLINATION: e.inclinationDeg,
    RA_OF_ASC_NODE: e.raanDeg,
    ARG_OF_PERICENTER: e.argOfPericenterDeg,
    MEAN_ANOMALY: e.meanAnomalyDeg,
    NORAD_CAT_ID: e.noradId,
    ELEMENT_SET_NO: 999,
    BSTAR: e.bstar,
    MEAN_MOTION_DOT: e.meanMotionDot,
    MEAN_MOTION_DDOT: e.meanMotionDdot,
  };
}

/** The epoch as Unix milliseconds (UTC when no zone is given), or NaN. */
export function epochMillis(epoch: string): number {
  const text = epoch.trim();
  return Date.parse(HAS_ZONE.test(text) ? text : `${text}Z`);
}

/** Initialises SGP4 for an element set, or returns null for unusable elements. */
export function toSatrec(e: SatelliteElements): SatRec | null {
  const epochMs = epochMillis(e.epoch);
  const numbers = [e.meanMotion, e.eccentricity, e.inclinationDeg, e.raanDeg, e.bstar];
  if (!Number.isFinite(epochMs) || !numbers.every(Number.isFinite)) return null;
  if (e.meanMotion <= 0 || e.eccentricity < 0 || e.eccentricity >= 1) return null;
  try {
    const satrec = json2satrec(toOmm(e, new Date(epochMs).toISOString()));
    return satrec.error === SatRecError.None ? satrec : null;
  } catch {
    return null;
  }
}

/** Greenwich mean sidereal time for a date (one per propagation pass). */
export function siderealTime(date: Date): number {
  return gstime(date);
}

/** Elements decayed long ago can make SGP4 "succeed" with garbage; satellite.js 7 detects it. */
const PROPAGATE_OPTIONS: PropagateOptions = { communityDecayCheckEnabled: true };

/** SGP4 state at `date`, or null when propagation fails, the object has decayed, or the
 *  position is implausible. */
function stateAt(satrec: SatRec, date: Date): PositionAndVelocity | null {
  const state = propagate(satrec, date, PROPAGATE_OPTIONS);
  if (!state || satrec.error !== SatRecError.None) return null;
  const { x, y, z } = state.position;
  const radius = Math.hypot(x, y, z);
  return radius > MIN_RADIUS_KM && radius < MAX_RADIUS_KM ? state : null;
}

/**
 * Writes the Earth-fixed position (metres) at `date` into `out`. Returns false when SGP4
 * fails (decayed, diverged), leaving `out` untouched.
 */
export function propagateEcf(satrec: SatRec, date: Date, gmst: number, out: EcfMetres): boolean {
  const state = stateAt(satrec, date);
  if (!state) return false;
  const ecf = eciToEcf(state.position, gmst);
  if (!Number.isFinite(ecf.x) || !Number.isFinite(ecf.y) || !Number.isFinite(ecf.z)) return false;
  out.x = ecf.x * 1000;
  out.y = ecf.y * 1000;
  out.z = ecf.z * 1000;
  return true;
}

/** Sub-satellite point (WGS84 geodetic) and altitude at `date`, or null on failure. */
export function subSatellitePoint(satrec: SatRec, date: Date): SubSatellitePoint | null {
  const state = stateAt(satrec, date);
  if (!state) return null;
  const geodetic = eciToGeodetic(state.position, gstime(date));
  let lon = geodetic.longitude * RAD_TO_DEG;
  if (lon > 180) lon -= 360;
  if (lon < -180) lon += 360;
  return { lat: geodetic.latitude * RAD_TO_DEG, lon, altM: geodetic.height * 1000 };
}
