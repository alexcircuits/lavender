import { Cartesian3, Cartographic, Math as CesiumMath, Ellipsoid } from '@cesium/core';
import { describe, expect, it } from 'vitest';
import type { Route } from '../contracts';
import {
  advanceRouteFlight,
  approachValue,
  createRouteFlight,
  levelCameraRoll,
  parseRouteCoordinates,
  probeMeshFloorM,
  ROUTE_CINEMA,
  ROUTE_M_S,
  type RouteFlightFrame,
  type RouteFlightOptions,
  type RouteFlightState,
  routeAltitudeOffsetM,
  routeColdSeedFloorM,
  routeCorridorCells,
  routeEyeHeightM,
  routeFloorHoldM,
  routeRampFraction,
  routeSpeedProfile,
  signedTurnRad,
} from './routeCinematics';

const FRAME_S = 1 / 60;
const CRUISE_M_S = ROUTE_M_S;

/** Roll of an applied camera frame in degrees (+ = right wing down). */
function rollDegOf(eye: Cartesian3, direction: Cartesian3, up: Cartesian3): number {
  const surfaceUp = Ellipsoid.WGS84.geodeticSurfaceNormal(eye, new Cartesian3());
  const right = Cartesian3.normalize(
    Cartesian3.cross(direction, surfaceUp, new Cartesian3()),
    new Cartesian3(),
  );
  const levelUp = Cartesian3.normalize(
    Cartesian3.cross(right, direction, new Cartesian3()),
    new Cartesian3(),
  );
  const sin = Cartesian3.dot(Cartesian3.cross(levelUp, up, new Cartesian3()), direction);
  return CesiumMath.toDegrees(Math.atan2(sin, Cartesian3.dot(levelUp, up)));
}

/** Unit horizontal component of a pitched camera direction at eye. */
function horizontalOf(eye: Cartesian3, direction: Cartesian3): Cartesian3 {
  const surfaceUp = Ellipsoid.WGS84.geodeticSurfaceNormal(eye, new Cartesian3());
  const vertical = Cartesian3.multiplyByScalar(
    surfaceUp,
    Cartesian3.dot(direction, surfaceUp),
    new Cartesian3(),
  );
  return Cartesian3.normalize(
    Cartesian3.subtract(direction, vertical, new Cartesian3()),
    new Cartesian3(),
  );
}

function flightFrom(
  lonLat: readonly (readonly [number, number])[],
  options: Partial<RouteFlightOptions> = {},
): RouteFlightState {
  const { pts, cumM } = parseRouteCoordinates(lonLat);
  return createRouteFlight({ pts, cumM, ...options });
}

interface RecordedFrame {
  readonly progress: number;
  readonly speed: number;
  readonly bankDeg: number;
  readonly heightM: number;
  readonly aglM: number;
  readonly traveledM: number;
  readonly eye: Cartesian3;
  readonly direction: Cartesian3;
  readonly up: Cartesian3;
}

function record(flight: RouteFlightState, { maxFrames = 20000 } = {}): RecordedFrame[] {
  const frames: RecordedFrame[] = [];
  for (let i = 0; i < maxFrames; i += 1) {
    const frame: RouteFlightFrame = advanceRouteFlight(flight, FRAME_S);
    if (frame.arming || !frame.eye || !frame.direction || !frame.up) continue;
    frames.push({
      progress: frame.progress,
      speed: frame.groundSpeedMps ?? 0,
      bankDeg: frame.bankDeg,
      heightM: frame.heightM ?? 0,
      aglM: frame.aglM ?? 0,
      traveledM: flight.traveled,
      eye: Cartesian3.clone(frame.eye),
      direction: Cartesian3.clone(frame.direction),
      up: Cartesian3.clone(frame.up),
    });
    if (frame.finished) break;
  }
  expect(frames.at(-1)?.progress).toBeGreaterThanOrEqual(1);
  return frames;
}

function maxStep(values: readonly number[]): number {
  let worst = 0;
  for (let i = 1; i < values.length; i += 1) {
    const a = values[i];
    const b = values[i - 1];
    if (a !== undefined && b !== undefined) {
      worst = Math.max(worst, Math.abs(a - b));
    }
  }
  return worst;
}

// Austin route with two sharp corners: first left, then right
const TWO_TURN_ROUTE: readonly [number, number][] = [
  [-97.76, 30.26],
  [-97.748, 30.26],
  [-97.748, 30.27], // left (east -> north)
  [-97.735, 30.27], // right (north -> east)
  [-97.735, 30.28],
  [-97.722, 30.28],
];

const STRAIGHT_ROUTE: readonly [number, number][] = [
  [-97.76, 30.26],
  [-97.7, 30.26],
  [-97.64, 30.26],
];

describe('routeCinematics', () => {
  describe('speed profile and easing', () => {
    it('routeSpeedProfile is continuous, normalized, and zero at both ends', () => {
      const r = routeRampFraction(30);
      expect(r).toBeGreaterThan(0);
      expect(r).toBeLessThanOrEqual(0.35);
      expect(routeSpeedProfile(0, r).distance).toBe(0);
      expect(routeSpeedProfile(0, r).speed).toBe(0);
      expect(Math.abs(routeSpeedProfile(1, r).distance - 1)).toBeLessThan(1e-12);
      expect(routeSpeedProfile(1, r).speed).toBe(0);

      // Derivative matches speed profile
      const cruiseShare = 1 / (1 - r);
      for (let u = 0.01; u < 1; u += 0.02) {
        const h = 1e-5;
        const derivative =
          (routeSpeedProfile(u + h, r).distance - routeSpeedProfile(u - h, r).distance) / (2 * h);
        const reported = routeSpeedProfile(u, r).speed * cruiseShare;
        expect(Math.abs(derivative - reported)).toBeLessThan(1e-3);
      }

      expect(routeRampFraction(1)).toBe(0.35);
      expect(routeRampFraction(0)).toBe(0);
    });

    it('eases in and out with bounded acceleration and monotonic distance', () => {
      const flight = flightFrom(TWO_TURN_ROUTE);
      const frames = record(flight);
      const speeds = frames.map((f) => f.speed);
      const cruise = CRUISE_M_S.normal;

      expect(speeds[0]).toBeLessThan(cruise * 0.05);
      expect(speeds.at(-1)).toBeLessThan(cruise * 0.05);
      expect(maxStep(speeds)).toBeLessThan(0.6);

      // Flat cruise plateau
      const plateau = frames
        .filter((f) => f.progress > 0.4 && f.progress < 0.6)
        .map((f) => f.speed);
      expect(plateau.length).toBeGreaterThan(10);
      expect(maxStep(plateau)).toBeLessThan(1e-9);

      // Monotonic distance reaching total length
      const traveled = frames.map((f) => f.traveledM);
      for (let i = 1; i < traveled.length; i += 1) {
        const cur = traveled[i];
        const prev = traveled[i - 1];
        if (cur !== undefined && prev !== undefined) {
          expect(cur).toBeGreaterThanOrEqual(prev - 1e-9);
        }
      }
      const lastTraveled = traveled.at(-1);
      expect(lastTraveled).toBeDefined();
      if (lastTraveled !== undefined) {
        expect(Math.abs(lastTraveled - flight.totalM)).toBeLessThan(1e-6);
      }
    });

    it('preserves mean ground speed across slow, normal, fast speeds', () => {
      for (const speed of ['slow', 'normal', 'fast'] as const) {
        const flight = flightFrom(TWO_TURN_ROUTE, { speed });
        const expectedS = flight.totalM / CRUISE_M_S[speed];
        expect(Math.abs(flight.durationS - expectedS)).toBeLessThan(1e-6);

        const frames = record(flight);
        const elapsedS = frames.length * FRAME_S;
        expect(Math.abs(elapsedS - expectedS)).toBeLessThan(0.5);
      }
    });

    it('enforces a 0.5s duration floor for tiny routes', () => {
      const tiny = flightFrom([
        [-97.76, 30.26],
        [-97.75995, 30.26],
      ]);
      expect(tiny.totalM).toBeLessThan(10);
      expect(tiny.durationS).toBe(0.5);
      expect(tiny.totalM / tiny.durationS).toBeLessThan(CRUISE_M_S.normal);
    });
  });

  describe('banking and orientation kinematics', () => {
    it('banks into corners smoothly, capped at maxBankDeg, and levels at ends', () => {
      const frames = record(flightFrom(TWO_TURN_ROUTE));
      const banks = frames.map((f) => f.bankDeg);

      expect(Math.abs(banks[0] ?? 0)).toBeLessThan(0.01);
      expect(Math.abs(banks.at(-1) ?? 0)).toBeLessThan(1.5);

      const peak = Math.max(...banks.map(Math.abs));
      expect(peak).toBeLessThanOrEqual(ROUTE_CINEMA.maxBankDeg + 1e-9);
      expect(peak).toBeGreaterThan(3);

      expect(Math.min(...banks)).toBeLessThan(-3);
      expect(Math.max(...banks)).toBeGreaterThan(3);

      const firstRoll = banks.find((deg) => Math.abs(deg) > 3);
      expect(firstRoll).toBeLessThan(0); // first turn is left

      expect(maxStep(banks)).toBeLessThan(0.25);

      // Verify that camera orientation roll matches reported frame bank
      for (const frame of frames) {
        if (Math.abs(frame.bankDeg) > 3) {
          const actualRoll = rollDegOf(frame.eye, frame.direction, frame.up);
          expect(Math.abs(actualRoll - frame.bankDeg)).toBeLessThan(0.1);
        }
      }
    });

    it('never banks on a straight route', () => {
      const banks = record(flightFrom(STRAIGHT_ROUTE)).map((f) => f.bankDeg);
      expect(Math.max(...banks.map(Math.abs))).toBeLessThan(0.05);
    });

    it('saturates the bank cap on a hairpin turn', () => {
      const hairpin: readonly [number, number][] = [
        [-97.76, 30.26],
        [-97.74, 30.26],
        [-97.76, 30.2605],
      ];
      const banks = record(flightFrom(hairpin, { speed: 'slow' })).map((f) => f.bankDeg);
      const peak = Math.max(...banks.map(Math.abs));
      expect(peak).toBeLessThanOrEqual(ROUTE_CINEMA.maxBankDeg + 1e-9);
      expect(peak).toBeGreaterThan(ROUTE_CINEMA.maxBankDeg - 1.5);
    });

    it('keeps up vector orthonormal and locks pitch to -32°', () => {
      const frames = record(flightFrom(TWO_TURN_ROUTE));
      for (const frame of frames) {
        expect(Math.abs(Cartesian3.magnitude(frame.up) - 1)).toBeLessThan(1e-9);
        expect(Math.abs(Cartesian3.magnitude(frame.direction) - 1)).toBeLessThan(1e-9);
        expect(Math.abs(Cartesian3.dot(frame.up, frame.direction))).toBeLessThan(1e-9);

        const surfaceUp = Ellipsoid.WGS84.geodeticSurfaceNormal(frame.eye, new Cartesian3());
        const pitchDeg = -CesiumMath.toDegrees(
          Math.asin(-Cartesian3.dot(frame.direction, surfaceUp)),
        );
        expect(Math.abs(pitchDeg - ROUTE_CINEMA.pitchDeg)).toBeLessThan(0.5);
      }
    });

    it('reverses heading on an exact U-turn and remains deterministic', () => {
      const uTurn: readonly [number, number][] = [
        [-97.76, 30.26],
        [-97.74, 30.26],
        [-97.76, 30.26],
      ];
      const flight = flightFrom(uTurn, { speed: 'slow', floorFn: () => 0 });
      const p0 = flight.pts[0];
      const p1 = flight.pts[1];
      expect(p0).toBeDefined();
      expect(p1).toBeDefined();
      if (!p0 || !p1) return;
      const outbound = Cartesian3.normalize(
        Cartesian3.subtract(p1, p0, new Cartesian3()),
        new Cartesian3(),
      );
      const returning = Cartesian3.negate(outbound, new Cartesian3());
      const frames = record(flight);
      const halfM = flight.totalM / 2;

      let worstOut = 1;
      let worstBack = 1;
      for (const frame of frames) {
        const heading = horizontalOf(frame.eye, frame.direction);
        if (frame.traveledM < halfM - 200) {
          worstOut = Math.min(worstOut, Cartesian3.dot(heading, outbound));
        } else if (frame.traveledM > halfM + 200) {
          worstBack = Math.min(worstBack, Cartesian3.dot(heading, returning));
        }
      }
      expect(worstOut).toBeGreaterThan(0.98);
      expect(worstBack).toBeGreaterThan(0.98);

      const banks = frames.map((f) => f.bankDeg);
      let flips = 0;
      for (let i = 1; i < banks.length; i += 1) {
        const cur = banks[i];
        const prev = banks[i - 1];
        if (cur !== undefined && prev !== undefined) {
          if (Math.abs(cur) > 2 && Math.abs(prev) > 2 && Math.sign(cur) !== Math.sign(prev)) {
            flips += 1;
          }
        }
      }
      expect(flips).toBe(0);
      expect(Math.max(...banks.map(Math.abs))).toBeGreaterThan(3);
    });

    it('signedTurnRad is deterministic at exactly 180°', () => {
      const up = new Cartesian3(0, 0, 1);
      const east = new Cartesian3(1, 0, 0);
      const west = new Cartesian3(-1, 0, 0);
      const north = new Cartesian3(0, 1, 0);

      expect(signedTurnRad(east, west, up)).toBe(Math.PI);
      expect(signedTurnRad(west, east, up)).toBe(Math.PI);
      expect(signedTurnRad(east, north, up)).toBeLessThan(0);
      expect(signedTurnRad(north, east, up)).toBeGreaterThan(0);
    });
  });

  describe('altitude breathing and terrain clearance', () => {
    it('altitude breathes around mean AGL and lifts into turns', () => {
      const flight = flightFrom(TWO_TURN_ROUTE, { floorFn: () => 0 });
      const heights = record(flight).map((f) => f.aglM);
      const min = Math.min(...heights);
      const max = Math.max(...heights);

      expect(min).toBeGreaterThanOrEqual(ROUTE_CINEMA.meanHeightM - ROUTE_CINEMA.breathM - 1e-6);
      expect(max).toBeLessThanOrEqual(
        ROUTE_CINEMA.meanHeightM + ROUTE_CINEMA.breathM + ROUTE_CINEMA.turnLiftM + 1e-6,
      );
      expect(max - min).toBeGreaterThan(5);

      const verticalMps = maxStep(heights) / FRAME_S;
      expect(verticalMps).toBeLessThan(7);

      let sum = 0;
      const samples = 2000;
      for (let i = 0; i < samples; i += 1) {
        sum += routeAltitudeOffsetM((i / samples) * 2200, 0);
      }
      expect(Math.abs(sum / samples)).toBeLessThan(0.2);
    });

    it('clears a rise BEFORE camera reaches it using lookahead', () => {
      const ridgeFloor = (_lat: number, lon: number) => (lon > -97.745 && lon < -97.73 ? 900 : 0);
      const flight = flightFrom(TWO_TURN_ROUTE, { floorFn: ridgeFloor });
      const frames = record(flight);

      const floorAtFrame = (frame: RecordedFrame) => {
        const carto = Cartographic.fromCartesian(frame.eye);
        return ridgeFloor(
          CesiumMath.toDegrees(carto.latitude),
          CesiumMath.toDegrees(carto.longitude),
        );
      };

      let approachSamples = 0;
      for (let i = 0, ahead = 0; i < frames.length; i += 1) {
        const frame = frames[i];
        if (!frame) continue;
        const carto = Cartographic.fromCartesian(frame.eye);
        expect(carto.height - floorAtFrame(frame)).toBeGreaterThanOrEqual(
          ROUTE_CINEMA.minClearanceM - 1e-6,
        );

        while (
          ahead < frames.length - 1 &&
          (frames[ahead]?.traveledM ?? 0) < frame.traveledM + 130
        ) {
          ahead += 1;
        }
        const frameAhead = frames[ahead];
        if (frameAhead) {
          const floorAhead = floorAtFrame(frameAhead);
          if (floorAhead > floorAtFrame(frame)) {
            approachSamples += 1;
            expect(carto.height - floorAhead).toBeGreaterThanOrEqual(
              ROUTE_CINEMA.minClearanceM - 1e-6,
            );
          }
        }
      }
      expect(approachSamples).toBeGreaterThan(10);
      expect(routeEyeHeightM(500, -100000)).toBe(500 + ROUTE_CINEMA.minClearanceM);
    });

    it('cold corridor never descends blind, holding seed until terrain resolves', () => {
      const ALBUQUERQUE: readonly [number, number][] = [
        [-106.61, 35.08],
        [-106.55, 35.08],
      ];
      const REAL_FLOOR_M = 1600;
      const cold = flightFrom(ALBUQUERQUE, { floorFn: () => null, cameraHeightM: 2400 });

      let first = advanceRouteFlight(cold, FRAME_S);
      while (first.arming) first = advanceRouteFlight(cold, FRAME_S);
      expect(first.heightM).toBeGreaterThan(REAL_FLOOR_M);

      // Terrain lands mid-flight
      const warming = flightFrom(ALBUQUERQUE, { floorFn: () => null, cameraHeightM: 2400 });
      let minClearance = Number.POSITIVE_INFINITY;
      for (let i = 0; ; i += 1) {
        if (i === 60) warming.floorFn = () => REAL_FLOOR_M;
        const frame = advanceRouteFlight(warming, FRAME_S);
        if (frame.arming) continue;
        minClearance = Math.min(minClearance, (frame.heightM ?? 0) - REAL_FLOOR_M);
        if (frame.finished) break;
      }
      expect(minClearance).toBeGreaterThanOrEqual(ROUTE_CINEMA.minClearanceM - 1e-6);
      expect(warming.armS).toBeGreaterThan(0);
      expect(warming.floorKnown).toBe(true);
    });

    it('arms for terrain rather than racing the warm', () => {
      const ALBUQUERQUE: readonly [number, number][] = [
        [-106.61, 35.08],
        [-106.55, 35.08],
      ];
      const REAL_FLOOR_M = 1600;
      let cold = true;
      const flight = flightFrom(ALBUQUERQUE, {
        floorFn: () => (cold ? null : REAL_FLOOR_M),
        cameraHeightM: 826,
      });

      for (let i = 0; i < 20; i += 1) {
        const frame = advanceRouteFlight(flight, FRAME_S);
        expect(frame.arming).toBe(true);
        expect(frame.eye).toBeUndefined();
      }
      expect(flight.traveled).toBe(0);
      expect(flight.armS).toBeGreaterThan(0.3);

      cold = false;
      let minHeight = Number.POSITIVE_INFINITY;
      for (let i = 0; i < 20000; i += 1) {
        const frame = advanceRouteFlight(flight, FRAME_S);
        if (frame.arming) continue;
        minHeight = Math.min(minHeight, frame.heightM ?? 0);
        if (frame.finished) break;
      }
      expect(minHeight).toBeGreaterThanOrEqual(REAL_FLOOR_M + ROUTE_CINEMA.minClearanceM - 1e-6);
    });

    it('rendered mesh probe fires once for cold cells', () => {
      const ALBUQUERQUE: readonly [number, number][] = [
        [-106.61, 35.08],
        [-106.55, 35.08],
      ];
      const REAL_FLOOR_M = 1600;
      const probeCalls: number[] = [];
      const flight = flightFrom(ALBUQUERQUE, {
        floorFn: () => null,
        cameraHeightM: 826,
        probeFn: (cells) => {
          probeCalls.push(cells.length);
          return { heightM: REAL_FLOOR_M, sampled: cells.length, requested: cells.length };
        },
      });

      expect(flight.floorKnown).toBe(true);
      expect(flight.floorFromMeshProbe).toBe(true);

      for (let i = 0; i < 100; i += 1) {
        const frame = advanceRouteFlight(flight, FRAME_S);
        expect(frame.arming).not.toBe(true);
      }
      expect(probeCalls.length).toBe(1);
    });

    it('probeMeshFloorM reports coverage and height correctly', () => {
      expect(probeMeshFloorM(null, [{ lat: 1, lon: 1 }])).toEqual({
        heightM: Number.NaN,
        sampled: 0,
        requested: 1,
      });
      const partial = probeMeshFloorM(
        { sampleHeight: (c) => (c.longitude > 0 ? 340 : undefined) },
        [
          { lat: 1, lon: 1 },
          { lat: 1, lon: -1 },
        ],
      );
      expect(partial.heightM).toBe(340);
      expect(partial.sampled).toBe(1);
      expect(partial.requested).toBe(2);
    });

    it('partially warm corridor is unresolved and probes only cold cells', () => {
      const ALBUQUERQUE: readonly [number, number][] = [
        [-106.61, 35.08],
        [-106.55, 35.08],
      ];
      const probedCells: string[][] = [];
      let warmCell: string | null = null;

      const partial = flightFrom(ALBUQUERQUE, {
        cameraHeightM: 2400,
        floorFn: (lat, lon) => {
          if (warmCell === null) warmCell = `${lat},${lon}`;
          return `${lat},${lon}` === warmCell ? 200 : null;
        },
        probeFn: (cells) => {
          probedCells.push(cells.map((c) => `${c.lat},${c.lon}`));
          return { heightM: 1600, sampled: cells.length, requested: cells.length };
        },
      });

      expect(probedCells.length).toBe(1);
      const probed = probedCells[0];
      expect(probed).toBeDefined();
      if (probed && warmCell) {
        expect(probed.length).toBeGreaterThanOrEqual(1);
        expect(probed.includes(warmCell)).toBe(false);
      }
      expect(partial.floorM).toBe(1600);
    });

    it('shaping floor is smoothed both ways and raw clamp clears cliffs instantly', () => {
      expect(routeFloorHoldM(Number.NaN, 120, FRAME_S)).toBe(120);
      expect(routeFloorHoldM(400, Number.NaN, FRAME_S)).toBe(400);

      const rising = routeFloorHoldM(100, 900, FRAME_S);
      expect(rising).toBeGreaterThan(100);
      expect(rising).toBeLessThan(130);

      const sinking = routeFloorHoldM(900, 100, FRAME_S);
      expect(sinking).toBeLessThan(900);
      expect(sinking).toBeGreaterThan(880);

      // Raw sample clamp
      const flight = flightFrom(STRAIGHT_ROUTE, { floorFn: () => 0 });
      advanceRouteFlight(flight, FRAME_S);
      flight.floorFn = () => 5000;
      const cliff = advanceRouteFlight(flight, FRAME_S);
      expect(cliff.heightM).toBeGreaterThanOrEqual(5000 + ROUTE_CINEMA.minClearanceM - 1e-6);
      expect(flight.floorM).toBeLessThan(1000);
    });

    it('flattens roll and altitude shaping under reduced motion', () => {
      const flight = flightFrom(TWO_TURN_ROUTE, { floorFn: () => 0, reducedMotion: true });
      const frames = record(flight);
      expect(Math.max(...frames.map((f) => Math.abs(f.bankDeg)))).toBe(0);
      for (const frame of frames) {
        expect(Math.abs(frame.aglM - ROUTE_CINEMA.meanHeightM)).toBeLessThan(1e-6);
      }
      expect(frames[0]?.speed).toBeLessThan(2);
      expect(frames.at(-1)?.speed).toBeLessThan(2);
    });
  });

  describe('completion and helpers', () => {
    it('lands on the last waypoint wings level with progress 1.0', () => {
      const flight = flightFrom(STRAIGHT_ROUTE, { speed: 'fast' });
      const frames = record(flight);
      const last = frames.at(-1);
      expect(last).toBeDefined();
      if (!last) return;
      expect(last.progress).toBe(1);

      const endCoord = STRAIGHT_ROUTE.at(-1);
      expect(endCoord).toBeDefined();
      if (!endCoord) return;
      const endpoint = Cartesian3.fromDegrees(...endCoord, 0);
      const endCarto = Cartographic.fromCartesian(last.eye);
      const endGround = Cartesian3.fromRadians(endCarto.longitude, endCarto.latitude, 0);
      expect(Cartesian3.distance(endpoint, endGround)).toBeLessThan(1);
      expect(last.bankDeg).toBe(0);
    });

    it('levelCameraRoll levels roll without altering heading, pitch, or destination', () => {
      let setViewCalls: Array<unknown> = [];
      const cam = {
        heading: 1.2,
        pitch: -0.5,
        roll: 0.15,
        setView: (opts: unknown) => {
          setViewCalls.push(opts);
        },
      };

      const changed = levelCameraRoll(cam);
      expect(changed).toBe(true);
      expect(setViewCalls.length).toBe(1);
      expect(setViewCalls[0]).toEqual({
        orientation: { heading: 1.2, pitch: -0.5, roll: 0 },
      });

      // Already level does not setView
      setViewCalls = [];
      cam.roll = 0;
      expect(levelCameraRoll(cam)).toBe(false);
      expect(setViewCalls.length).toBe(0);
    });

    it('parseRouteCoordinates handles Route object and coordinate arrays', () => {
      const route: Route = {
        ok: true,
        profile: 'car',
        distanceM: 1000,
        durationS: 60,
        geometry: [
          [13.4, 52.5],
          [13.41, 52.51],
        ],
      };
      const parsed = parseRouteCoordinates(route);
      expect(parsed.pts.length).toBe(2);
      expect(parsed.cumM.length).toBe(2);
      expect(parsed.cumM[0]).toBe(0);
      expect(parsed.cumM[1]).toBeGreaterThan(0);

      // Empty coordinates fallback
      const empty = parseRouteCoordinates([]);
      expect(empty.pts.length).toBe(2);
      expect(empty.cumM).toEqual([0, 0]);
    });

    it('routeColdSeedFloorM caps at 5000m and never goes below path', () => {
      expect(routeColdSeedFloorM(0, 2400)).toBe(2140);
      expect(routeColdSeedFloorM(0, 17_000_000)).toBe(5000);
      expect(routeColdSeedFloorM(0, 100)).toBe(0);
      expect(routeColdSeedFloorM(0, Number.NaN)).toBe(0);
    });

    it('flight warms its own corridor rolling ahead', () => {
      const warmed: Array<readonly unknown[]> = [];
      const flight = flightFrom(
        [
          [-106.61, 35.08],
          [-106.55, 35.08],
        ],
        {
          floorFn: () => null,
          warmFn: (cells) => warmed.push(cells),
        },
      );
      expect(warmed.length).toBe(1);
      const w0 = warmed[0];
      expect(w0).toBeDefined();
      if (w0) {
        expect(w0.length).toBeGreaterThan(10);
        expect(w0.length).toBeLessThanOrEqual(96);
      }

      for (let i = 0; i < 360; i += 1) advanceRouteFlight(flight, FRAME_S);
      expect(warmed.length).toBeGreaterThan(1);
    });

    it('approachValue is frame-rate independent and adopts cold target', () => {
      expect(approachValue(Number.NaN, 7, 2, FRAME_S)).toBe(7);
      const oneBigStep = approachValue(0, 10, 2, 0.5);
      let manySmallSteps = 0;
      for (let i = 0; i < 30; i += 1) {
        manySmallSteps = approachValue(manySmallSteps, 10, 2, 0.5 / 30);
      }
      expect(Math.abs(oneBigStep - manySmallSteps)).toBeLessThan(1e-9);

      const afterStall = approachValue(0, 10, 2, 100);
      expect(afterStall).toBeLessThanOrEqual(10);
      expect(afterStall).toBeGreaterThan(9.99);
    });

    it('routeCorridorCells returns bounded and ordered cells', () => {
      const flight = flightFrom(TWO_TURN_ROUTE);
      const cells = routeCorridorCells(flight, 0, 1e9, 12);
      expect(cells.length).toBe(12);
    });
  });
});
