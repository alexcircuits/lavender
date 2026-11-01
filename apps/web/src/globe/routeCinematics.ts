/**
 * Route flight cinematics and tour director (LB-166).
 *
 * Implements smooth street-following dolly flights along routes with:
 * - Closed-form trapezoid speed profiling (routeSpeedProfile)
 * - Coordinated banking into planned turns (routeBankTargetDeg)
 * - Altitude breathing over straights and lookahead turn lift (routeAltitudeOffsetM)
 * - Lookahead gaze direction with smooth turning rate
 * - Ground corridor warming and floor tracking with non-descent guarantees
 * - Pitch locking at -32°
 */

import {
  Cartesian3,
  Cartographic,
  Math as CesiumMath,
  Ellipsoid,
  Matrix3,
  Quaternion,
} from '@cesium/core';
import type { Route } from '../contracts';
import {
  approachValue,
  ROUTE_BANK_MAX_DEG,
  ROUTE_CAMERA_HEIGHT_M,
  ROUTE_MIN_CLEARANCE_M,
  ROUTE_PITCH_DEG,
  routeAltitudeOffsetM,
  routeBankTargetDeg,
  routeEyeHeightM,
  routeRampFraction,
  routeSpeedProfile,
} from './cameraCinematics';

export {
  approachValue,
  ROUTE_BANK_MAX_DEG,
  ROUTE_BANK_PER_DEG_S,
  ROUTE_BREATH_M,
  ROUTE_BREATH_WAVELENGTH_M,
  ROUTE_CAMERA_HEIGHT_M,
  ROUTE_CINEMA,
  ROUTE_MIN_CLEARANCE_M,
  ROUTE_PITCH_DEG,
  ROUTE_RAMP_MAX_FRACTION,
  ROUTE_RAMP_S,
  ROUTE_TURN_LIFT_M,
  routeAltitudeOffsetM,
  routeBankTargetDeg,
  routeEyeHeightM,
  routeRampFraction,
  routeSpeedProfile,
} from './cameraCinematics';

export type RouteSpeed = 'slow' | 'normal' | 'fast';

/** Mean ground speed in m/s by speed word. */
export const ROUTE_M_S: Record<RouteSpeed, number> = {
  slow: 20,
  normal: 40,
  fast: 90,
};

/** Seconds of travel the gaze leads the camera by. */
export const ROUTE_LOOKAHEAD_S = 6.5;
export const ROUTE_LOOKAHEAD_MIN_M = 120;
export const ROUTE_LOOKAHEAD_MAX_M = 600;

/** Exponential smoothing rate (1/s) for dolly gaze direction. */
export const ROUTE_DIR_SMOOTH_RATE = 1.6;

/** Seconds of path centered on camera the turn is measured over. */
export const ROUTE_BANK_WINDOW_S = 4;

/** Two cascaded first-order filters for roll entry/exit (1/s). */
export const ROUTE_BANK_LEAD_RATE = 2.2;
export const ROUTE_BANK_SETTLE_RATE = 1.6;

/** How fast the altitude follows the roll (1/s). */
export const ROUTE_LIFT_TRACK_RATE = 0.5;

/** How fast the altitude shaping is filtered to the camera (1/s). */
export const ROUTE_ALT_SMOOTH_RATE = 0.9;

/** How fast the shaping floor follows the sampled one (1/s). */
export const ROUTE_FLOOR_FALL_RATE = 0.9;

/** Below this |cos| chords are antipodal; resolve deterministic right turn. */
export const ANTIPODAL_EPS = 1e-6;

/** Metres of corridor warmed when flight starts and rolling window ahead. */
export const ROUTE_WARM_START_M = 4000;
export const ROUTE_WARM_AHEAD_M = 3000;
export const ROUTE_WARM_MAX_CELLS = 96;
export const ROUTE_WARM_INTERVAL_S = 2;
export const ROUTE_WARM_STEP_M = 55;

/** Ceiling on conservative floor seed while corridor is cold. */
export const ROUTE_UNKNOWN_FLOOR_CAP_M = 5000;

/** Hard cap on mesh sampleHeight cells per route. */
export const ROUTE_MESH_PROBE_CELLS = 8;

/** Seconds dolly arms waiting for real floor data. */
export const ROUTE_ARM_S = 1.2;

/** Cap on eye descent speed (m/s). */
export const ROUTE_MAX_DESCENT_MPS = 10;

export interface CorridorCell {
  readonly lat: number;
  readonly lon: number;
}

export interface MeshProbeResult {
  readonly heightM: number;
  readonly sampled: number;
  readonly requested: number;
}

export interface RouteFlightOptions {
  readonly pts: readonly Cartesian3[];
  readonly cumM: readonly number[];
  readonly speed?: RouteSpeed;
  readonly floorFn?: ((latDeg: number, lonDeg: number) => number | null | undefined) | null;
  readonly warmFn?: ((cells: readonly CorridorCell[]) => void) | null;
  readonly probeFn?:
    | ((cells: readonly CorridorCell[]) => MeshProbeResult | null | undefined)
    | null;
  readonly cameraHeightM?: number;
  readonly reducedMotion?: boolean;
}

export interface RouteFlightState {
  readonly kind: 'route';
  readonly speed: RouteSpeed;
  readonly pts: readonly Cartesian3[];
  readonly cumM: readonly number[];
  readonly totalM: number;
  readonly durationS: number;
  readonly ramp: number;
  u: number;
  traveled: number;
  floorFn?: ((latDeg: number, lonDeg: number) => number | null | undefined) | null;
  warmFn?: ((cells: readonly CorridorCell[]) => void) | null;
  probeFn?: ((cells: readonly CorridorCell[]) => MeshProbeResult | null | undefined) | null;
  readonly reducedMotion: boolean;
  floorM: number;
  floorKnown: boolean;
  coldSeedFloorM: number;
  safeHoldHeightM: number;
  coldFrames: number;
  floorFromMeshProbe: boolean;
  meshProbeSpent: boolean;
  meshProbeFloorM: number;
  meshProbeCoveredCold: boolean;
  armS: number;
  armed: boolean;
  departed: boolean;
  appliedHeightM: number;
  warmedCells: number;
  warmDueS: number;
  offsetM: number;
  bankLeadDeg: number;
  bankDeg: number;
  appliedBankDeg: number;
  liftBankDeg: number;
  headingDir: Cartesian3 | null;
  groundSpeedMps: number;
}

export interface RouteFlightFrame {
  readonly finished: boolean;
  readonly arming?: boolean;
  readonly progress: number;
  readonly bankDeg: number;
  readonly eye?: Cartesian3;
  readonly direction?: Cartesian3;
  readonly up?: Cartesian3;
  readonly heightM?: number;
  readonly aglM?: number;
  readonly groundSpeedMps?: number;
}

/** Parses route geometry or coordinate tuples into Cartesian3 vertices and cumulative distances. */
export function parseRouteCoordinates(
  routeOrCoords:
    | Route
    | readonly (readonly [number, number])[]
    | readonly { readonly lon: number; readonly lat: number; readonly height?: number }[],
): { pts: Cartesian3[]; cumM: number[] } {
  let coords: readonly (
    | readonly [number, number]
    | { readonly lon: number; readonly lat: number; readonly height?: number }
  )[];

  if ('geometry' in routeOrCoords && Array.isArray((routeOrCoords as Route).geometry)) {
    coords = (routeOrCoords as Route).geometry;
  } else if (Array.isArray(routeOrCoords)) {
    coords = routeOrCoords;
  } else {
    coords = [];
  }

  const pts: Cartesian3[] = [];
  for (const item of coords) {
    if (Array.isArray(item)) {
      pts.push(Cartesian3.fromDegrees(item[0], item[1], 0));
    } else if (item && typeof item === 'object' && 'lon' in item && 'lat' in item) {
      pts.push(Cartesian3.fromDegrees(item.lon, item.lat, item.height ?? 0));
    }
  }

  if (pts.length === 0) {
    pts.push(Cartesian3.fromDegrees(0, 0, 0), Cartesian3.fromDegrees(0, 0, 0));
  } else if (pts.length === 1) {
    const first = pts[0];
    if (first) pts.push(Cartesian3.clone(first));
  }

  const cumM: number[] = [0];
  for (let i = 1; i < pts.length; i++) {
    const prev = pts[i - 1];
    const curr = pts[i];
    const prevCum = cumM[i - 1] ?? 0;
    if (prev && curr) {
      cumM.push(prevCum + Cartesian3.distance(prev, curr));
    } else {
      cumM.push(prevCum);
    }
  }

  return { pts, cumM };
}

const _turnCross = new Cartesian3();

/**
 * Signed angle from horizontal direction `a` to `b` about `up`; positive = turning RIGHT.
 * Deterministic for 180° turns.
 */
export function signedTurnRad(a: Cartesian3, b: Cartesian3, up: Cartesian3): number {
  const cross = Cartesian3.cross(a, b, _turnCross);
  const sinLeft = Cartesian3.dot(cross, up);
  const cosTurn = Cartesian3.dot(a, b);
  if (cosTurn <= -1 + ANTIPODAL_EPS && Math.abs(sinLeft) < ANTIPODAL_EPS) {
    return Math.PI;
  }
  return -Math.atan2(sinLeft, cosTurn);
}

/** Point at arc length `s` along the polyline using binary search. */
export function arcPoint(
  state: { pts: readonly Cartesian3[]; cumM: readonly number[]; totalM: number },
  s: number,
  result: Cartesian3,
): Cartesian3 {
  const { pts, cumM, totalM } = state;
  const clamped = Math.min(totalM, Math.max(0, s));
  let lo = 0;
  let hi = cumM.length - 2;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    const midM = cumM[mid] ?? 0;
    if (midM <= clamped) lo = mid;
    else hi = mid - 1;
  }
  const cumLo = cumM[lo] ?? 0;
  const cumNext = cumM[lo + 1] ?? cumLo;
  const segLen = cumNext - cumLo;
  const t = segLen > 1e-6 ? Math.min(1, Math.max(0, (clamped - cumLo) / segLen)) : 0;
  const ptLo = pts[lo];
  const ptNext = pts[lo + 1];
  if (!ptLo || !ptNext) return result;
  return Cartesian3.lerp(ptLo, ptNext, t, result);
}

const _arcA = new Cartesian3();
const _arcB = new Cartesian3();
const _chordVertical = new Cartesian3();

/** Unit horizontal direction from arc `sA` to `sB`, or null when degenerate. */
export function horizontalChord(
  state: { pts: readonly Cartesian3[]; cumM: readonly number[]; totalM: number },
  sA: number,
  sB: number,
  up: Cartesian3,
  result: Cartesian3,
): Cartesian3 | null {
  const a = arcPoint(state, sA, _arcA);
  const b = arcPoint(state, sB, _arcB);
  Cartesian3.subtract(b, a, result);
  const vertical = Cartesian3.multiplyByScalar(up, Cartesian3.dot(result, up), _chordVertical);
  Cartesian3.subtract(result, vertical, result);
  const len = Cartesian3.magnitude(result);
  if (!(len > 1e-3)) return null;
  return Cartesian3.divideByScalar(result, len, result);
}

const _warmPos = new Cartesian3();
const _warmCarto = new Cartographic();

/** Coarse floor cells along a stretch of the route in travel order on a 0.001° grid. */
export function routeCorridorCells(
  state: { pts: readonly Cartesian3[]; cumM: readonly number[]; totalM: number },
  fromM: number,
  spanM: number,
  maxCells = ROUTE_WARM_MAX_CELLS,
): CorridorCell[] {
  const out: CorridorCell[] = [];
  if (!(state?.totalM > 0) || !(spanM > 0) || !(maxCells > 0)) return out;
  const start = Math.max(0, Math.min(state.totalM, fromM));
  const end = Math.max(0, Math.min(state.totalM, fromM + spanM));
  const seen = new Set<string>();
  for (let s = start; s <= end + 1e-6 && out.length < maxCells; s += ROUTE_WARM_STEP_M) {
    const point = arcPoint(state, s, _warmPos);
    const carto = Cartographic.fromCartesian(point, Ellipsoid.WGS84, _warmCarto);
    if (!carto) continue;
    const lat = Number(CesiumMath.toDegrees(carto.latitude).toFixed(3));
    const lon = Number(CesiumMath.toDegrees(carto.longitude).toFixed(3));
    const key = `${lat},${lon}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ lat, lon });
  }
  return out;
}

function warmRouteCorridor(state: RouteFlightState, fromM: number, spanM: number): number {
  if (typeof state?.warmFn !== 'function') return 0;
  const cells = routeCorridorCells(state, fromM, spanM);
  if (!cells.length) return 0;
  try {
    state.warmFn(cells);
  } catch {
    return 0;
  }
  state.warmedCells += cells.length;
  return cells.length;
}

/** Floor to hold while the corridor is cold (non-descent guarantee). */
export function routeColdSeedFloorM(pathHeightM: number, cameraHeightM?: number): number {
  const base = Number.isFinite(pathHeightM) ? pathHeightM : 0;
  if (cameraHeightM === undefined || !Number.isFinite(cameraHeightM)) return base;
  const implied = cameraHeightM - ROUTE_CAMERA_HEIGHT_M;
  return Math.max(base, Math.min(base + ROUTE_UNKNOWN_FLOOR_CAP_M, implied));
}

/** Tracks ground floor smoothly in both directions. */
export function routeFloorHoldM(previousM: number, sampledM: number, dt: number): number {
  if (!Number.isFinite(sampledM)) return previousM;
  if (!Number.isFinite(previousM)) return sampledM;
  return approachValue(previousM, sampledM, ROUTE_FLOOR_FALL_RATE, dt);
}

const _probeCarto = new Cartographic();

/** Probes highest rendered surface across corridor cells with coverage reporting. */
export function probeMeshFloorM(
  scene: { sampleHeight?: (carto: Cartographic) => number | undefined } | null | undefined,
  cells: readonly CorridorCell[],
): MeshProbeResult {
  const requested = Array.isArray(cells) ? cells.length : 0;
  const empty: MeshProbeResult = { heightM: Number.NaN, sampled: 0, requested };
  if (!scene || typeof scene.sampleHeight !== 'function' || !requested) {
    return empty;
  }
  let heightM = Number.NaN;
  let sampled = 0;
  for (const cell of cells) {
    try {
      const carto = Cartographic.fromDegrees(cell.lon, cell.lat, 0, _probeCarto);
      const height = scene.sampleHeight(carto);
      if (!Number.isFinite(height)) continue;
      sampled += 1;
      heightM = Number.isFinite(heightM) ? Math.max(heightM, height as number) : (height as number);
    } catch {
      /* tiles not ready for this cell */
    }
  }
  return { heightM, sampled, requested };
}

function acquireCorridorFloor(state: RouteFlightState, takeWhole: boolean): boolean {
  const cells = routeCorridorCells(
    state,
    state.traveled,
    ROUTE_WARM_START_M,
    ROUTE_MESH_PROBE_CELLS,
  );
  if (!cells.length) return false;
  const known: number[] = [];
  const cold: CorridorCell[] = [];
  if (typeof state.floorFn === 'function') {
    for (const cell of cells) {
      let floor: number | null | undefined = Number.NaN;
      try {
        floor = state.floorFn(cell.lat, cell.lon);
      } catch {
        /* a hostile read is the same as a cold one */
      }
      if (typeof floor === 'number' && Number.isFinite(floor)) known.push(floor);
      else cold.push(cell);
    }
  } else {
    cold.push(...cells);
  }

  if (cold.length && !state.meshProbeSpent && typeof state.probeFn === 'function') {
    state.meshProbeSpent = true;
    try {
      const probe = state.probeFn(cold);
      if (probe && Number.isFinite(probe.heightM)) {
        state.meshProbeFloorM = probe.heightM;
        state.floorFromMeshProbe = true;
        state.meshProbeCoveredCold = probe.sampled >= cold.length;
      }
    } catch {
      /* tiles not streamed, or mid-teardown */
    }
  }
  if (Number.isFinite(state.meshProbeFloorM)) known.push(state.meshProbeFloorM);
  if (!known.length) return false;

  const floor = Math.max(...known);
  state.floorM =
    takeWhole || !Number.isFinite(state.floorM) ? floor : Math.max(state.floorM, floor);
  if (cold.length && !state.meshProbeCoveredCold) return false;
  state.floorKnown = true;
  return true;
}

/** Builds the cinematic route flight state. */
export function createRouteFlight({
  pts,
  cumM,
  speed = 'normal',
  floorFn = null,
  warmFn = null,
  probeFn = null,
  cameraHeightM = Number.NaN,
  reducedMotion = false,
}: RouteFlightOptions): RouteFlightState {
  const totalM = cumM[cumM.length - 1] ?? 0;
  const cruiseMps = ROUTE_M_S[speed] ?? ROUTE_M_S.normal;
  const durationS = Math.max(0.5, totalM / cruiseMps);
  const firstPt = pts[0];
  const pathCarto = firstPt
    ? Cartographic.fromCartesian(firstPt, Ellipsoid.WGS84, _warmCarto)
    : null;
  const pathHeightM = pathCarto?.height ?? 0;

  const state: RouteFlightState = {
    kind: 'route',
    speed,
    pts,
    cumM,
    totalM,
    durationS,
    ramp: routeRampFraction(durationS),
    u: 0,
    traveled: 0,
    floorFn,
    warmFn,
    probeFn,
    reducedMotion,
    floorM: Number.NaN,
    floorKnown: false,
    coldSeedFloorM: routeColdSeedFloorM(pathHeightM, cameraHeightM),
    safeHoldHeightM: Number.isFinite(cameraHeightM) ? (cameraHeightM as number) : Number.NaN,
    coldFrames: 0,
    floorFromMeshProbe: false,
    meshProbeSpent: false,
    meshProbeFloorM: Number.NaN,
    meshProbeCoveredCold: false,
    armS: 0,
    armed: false,
    departed: false,
    appliedHeightM: Number.NaN,
    warmedCells: 0,
    warmDueS: ROUTE_WARM_INTERVAL_S,
    offsetM: Number.NaN,
    bankLeadDeg: 0,
    bankDeg: 0,
    appliedBankDeg: 0,
    liftBankDeg: 0,
    headingDir: null,
    groundSpeedMps: 0,
  };

  warmRouteCorridor(state, 0, ROUTE_WARM_START_M);
  acquireCorridorFloor(state, true);
  return state;
}

const _framePos = new Cartesian3();
const _frameAheadPos = new Cartesian3();
const _frameUp = new Cartesian3();
const _frameIn = new Cartesian3();
const _frameOut = new Cartesian3();
const _frameGaze = new Cartesian3();
const _frameRight = new Cartesian3();
const _frameLevelUp = new Cartesian3();
const _frameDir = new Cartesian3();
const _frameEye = new Cartesian3();
const _frameBankedUp = new Cartesian3();
const _frameCarto = new Cartographic();
const _frameAheadCarto = new Cartographic();
const _bankQuat = new Quaternion();
const _bankMatrix = new Matrix3();
const _headingQuat = new Quaternion();
const _headingMatrix = new Matrix3();

/** Advances one dolly frame, computing camera position, orientation, and bank roll. */
export function advanceRouteFlight(state: RouteFlightState, dt: number): RouteFlightFrame {
  const step = Math.min(0.25, Math.max(0, Number.isFinite(dt) ? dt : 0));

  const canAnswer = typeof state.floorFn === 'function' || typeof state.probeFn === 'function';
  if (canAnswer && !state.floorKnown && !state.departed && state.armS < ROUTE_ARM_S) {
    state.armS += step;
    if (!acquireCorridorFloor(state, true)) {
      return { arming: true, finished: false, progress: 0, bankDeg: 0 };
    }
  }
  state.departed = true;

  state.u = Math.min(1, (state.u || 0) + step / state.durationS);
  const profile = routeSpeedProfile(state.u, state.ramp);
  const traveled = state.totalM * profile.distance;
  state.traveled = traveled;
  const cruiseMps = state.totalM / (state.durationS * (1 - state.ramp));
  state.groundSpeedMps = cruiseMps * profile.speed;

  const pos = arcPoint(state, traveled, _framePos);
  const carto = Cartographic.fromCartesian(pos, Ellipsoid.WGS84, _frameCarto);
  if (!carto) {
    return { finished: true, progress: 1, bankDeg: 0 };
  }
  const up = Ellipsoid.WGS84.geodeticSurfaceNormalCartographic(carto, _frameUp);

  const halfWindowM = Math.max(20, (ROUTE_BANK_WINDOW_S * cruiseMps) / 2);
  const inbound = horizontalChord(state, traveled - halfWindowM, traveled, up, _frameIn);
  const outbound = horizontalChord(state, traveled, traveled + halfWindowM, up, _frameOut);
  const turnRateDegS =
    inbound && outbound
      ? CesiumMath.toDegrees(signedTurnRad(inbound, outbound, up)) / ROUTE_BANK_WINDOW_S
      : 0;
  const bankTarget = routeBankTargetDeg(turnRateDegS, state.reducedMotion);
  state.bankLeadDeg = approachValue(state.bankLeadDeg, bankTarget, ROUTE_BANK_LEAD_RATE, step);
  state.bankDeg = approachValue(state.bankDeg, state.bankLeadDeg, ROUTE_BANK_SETTLE_RATE, step);
  const appliedBankDeg = state.bankDeg * profile.speed;

  const lookaheadM = Math.min(
    ROUTE_LOOKAHEAD_MAX_M,
    Math.max(ROUTE_LOOKAHEAD_MIN_M, ROUTE_LOOKAHEAD_S * cruiseMps),
  );
  const gaze =
    horizontalChord(state, traveled, traveled + lookaheadM, up, _frameGaze) || outbound || inbound;
  if (gaze && !state.headingDir) {
    state.headingDir = Cartesian3.clone(gaze);
  } else if (gaze && state.headingDir) {
    const k = 1 - Math.exp(-ROUTE_DIR_SMOOTH_RATE * step);
    const turnToGaze = signedTurnRad(state.headingDir, gaze, up);
    const rotation = Matrix3.fromQuaternion(
      Quaternion.fromAxisAngle(up, -turnToGaze * k, _headingQuat),
      _headingMatrix,
    );
    const turned = Matrix3.multiplyByVector(rotation, state.headingDir, _frameDir);
    const vertical = Cartesian3.multiplyByScalar(up, Cartesian3.dot(turned, up), _chordVertical);
    Cartesian3.subtract(turned, vertical, turned);
    if (Cartesian3.magnitude(turned) > 1e-6) {
      Cartesian3.normalize(turned, state.headingDir);
    }
  }

  if (!state.headingDir) {
    const east = new Cartesian3(-pos.y, pos.x, 0);
    state.headingDir =
      Cartesian3.magnitude(east) > 1e-6
        ? Cartesian3.normalize(east, new Cartesian3())
        : new Cartesian3(1, 0, 0);
  }
  const heading = state.headingDir;

  const pitch = CesiumMath.toRadians(ROUTE_PITCH_DEG);
  const direction = Cartesian3.normalize(
    Cartesian3.add(
      Cartesian3.multiplyByScalar(heading, Math.cos(pitch), _frameDir),
      Cartesian3.multiplyByScalar(up, Math.sin(pitch), _frameLevelUp),
      _frameDir,
    ),
    _frameDir,
  );
  const right = Cartesian3.normalize(Cartesian3.cross(direction, up, _frameRight), _frameRight);
  const levelUp = Cartesian3.normalize(
    Cartesian3.cross(right, direction, _frameLevelUp),
    _frameLevelUp,
  );
  const bankRad = CesiumMath.toRadians(appliedBankDeg);
  let bankedUp = levelUp;
  if (Math.abs(bankRad) > 1e-6) {
    const quat = Quaternion.fromAxisAngle(direction, bankRad, _bankQuat);
    const rotation = Matrix3.fromQuaternion(quat, _bankMatrix);
    bankedUp = Matrix3.multiplyByVector(rotation, levelUp, _frameBankedUp);
  }

  state.warmDueS -= step;
  if (state.warmDueS <= 0) {
    state.warmDueS = ROUTE_WARM_INTERVAL_S;
    warmRouteCorridor(state, traveled, ROUTE_WARM_AHEAD_M);
  }

  let sampledFloor = Number.NaN;
  if (typeof state.floorFn === 'function') {
    const aheadPos = arcPoint(state, traveled + lookaheadM / 2, _frameAheadPos);
    const aheadCarto = Cartographic.fromCartesian(aheadPos, Ellipsoid.WGS84, _frameAheadCarto);
    try {
      const here = state.floorFn(
        CesiumMath.toDegrees(carto.latitude),
        CesiumMath.toDegrees(carto.longitude),
      );
      const ahead = aheadCarto
        ? state.floorFn(
            CesiumMath.toDegrees(aheadCarto.latitude),
            CesiumMath.toDegrees(aheadCarto.longitude),
          )
        : null;
      const warm = [here, ahead].filter(
        (value): value is number => typeof value === 'number' && Number.isFinite(value),
      );
      if (warm.length) sampledFloor = Math.max(...warm);
    } catch {
      sampledFloor = Number.NaN;
    }
  }

  if (!Number.isFinite(sampledFloor)) {
    state.floorM = Number.isFinite(state.floorM) ? state.floorM : state.coldSeedFloorM;
    state.coldFrames += 1;
  } else if (!state.floorKnown) {
    state.floorM = routeFloorHoldM(state.floorM, sampledFloor, step);
    state.floorKnown = true;
  } else {
    state.floorM = routeFloorHoldM(state.floorM, sampledFloor, step);
  }

  state.liftBankDeg = approachValue(state.liftBankDeg, appliedBankDeg, ROUTE_LIFT_TRACK_RATE, step);
  const bankFraction = state.liftBankDeg / ROUTE_BANK_MAX_DEG;
  state.offsetM = approachValue(
    state.offsetM,
    routeAltitudeOffsetM(traveled, bankFraction, state.reducedMotion),
    ROUTE_ALT_SMOOTH_RATE,
    step,
  );

  let heightM = Math.max(
    routeEyeHeightM(state.floorM, state.offsetM),
    (Number.isFinite(sampledFloor) ? sampledFloor : state.floorM) + ROUTE_MIN_CLEARANCE_M,
  );
  if (!state.floorKnown && Number.isFinite(state.safeHoldHeightM)) {
    heightM = Math.max(heightM, state.safeHoldHeightM);
  }
  if (Number.isFinite(state.appliedHeightM)) {
    heightM = Math.max(heightM, state.appliedHeightM - ROUTE_MAX_DESCENT_MPS * step);
  }
  state.appliedHeightM = heightM;
  const eye = Cartesian3.fromRadians(
    carto.longitude,
    carto.latitude,
    heightM,
    Ellipsoid.WGS84,
    _frameEye,
  );

  state.appliedBankDeg = appliedBankDeg;
  return {
    finished: state.u >= 1,
    eye: Cartesian3.clone(eye),
    direction: Cartesian3.clone(direction),
    up: Cartesian3.clone(bankedUp),
    bankDeg: appliedBankDeg,
    heightM,
    aglM: heightM - state.floorM,
    groundSpeedMps: state.groundSpeedMps,
    progress: state.u,
  };
}

/**
 * Resets camera roll to 0 (levels horizon) while preserving position, heading and pitch.
 */
export function levelCameraRoll(camera: {
  heading?: number;
  pitch?: number;
  roll?: number;
  setView?: (options: { orientation: { heading?: number; pitch?: number; roll?: number } }) => void;
}): boolean {
  try {
    if (!camera || typeof camera.setView !== 'function') return false;
    const roll = CesiumMath.negativePiToPi(camera.roll || 0);
    if (Math.abs(roll) < 1e-5) return false;
    const orientation: { heading?: number; pitch?: number; roll: number } = { roll: 0 };
    if (typeof camera.heading === 'number') orientation.heading = camera.heading;
    if (typeof camera.pitch === 'number') orientation.pitch = camera.pitch;
    camera.setView({
      orientation,
    });
    return true;
  } catch {
    return false;
  }
}
