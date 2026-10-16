/**
 * The feed query box against Cesium's own camera (its view rectangle and ellipsoid picks need
 * no GPU, only the few scene fields faked below). Cesium's `computeViewRectangle` bounds the
 * four screen corners; `widenToEdges` adds the screen edges (view.ts). The box is shared by
 * every viewport feed, and cell-based providers plan their upstream queries from it: aircraft
 * in 5° cells, at most 4 before the worldwide military feed, vessels in 10° cells, at most 9
 * (the defaults in crates/lavender-core/src/config.rs, planned by lavender-geo `plan_cells`).
 * These tests pin what the widening changes for them: the view's own centre and edges are
 * queried, and no regional view tips over a cell budget because of it (ENG-980 review).
 */
import { Cartesian2, Cartesian3, Cartographic, Math as CesiumMath, Ellipsoid } from '@cesium/core';
import { Camera, GeographicProjection, Rectangle, SceneMode } from '@cesium/engine';
import { describe, expect, it } from 'vitest';
import type { BBox } from '../contracts';
import { type GeoSample, toViewState, viewEdgeSamples } from './view';

interface Pose {
  readonly lat: number;
  readonly lon: number;
  readonly heightKm: number;
  readonly pitchDeg: number;
}

function cameraOn(width: number, height: number): Camera {
  const scene = {
    drawingBufferWidth: width,
    drawingBufferHeight: height,
    canvas: { clientWidth: width, clientHeight: height, width, height },
    mapProjection: new GeographicProjection(),
    mode: SceneMode.SCENE3D,
    pixelRatio: 1,
  };
  return new Camera(scene as unknown as ConstructorParameters<typeof Camera>[0]);
}

/** The query box before and after widening, and the edge points the screen shows. */
function viewBoxes(pose: Pose, width = 1440, height = 900) {
  const camera = cameraOn(width, height);
  camera.setView({
    destination: Cartesian3.fromDegrees(pose.lon, pose.lat, pose.heightKm * 1000),
    orientation: { heading: 0, pitch: CesiumMath.toRadians(pose.pitchDeg), roll: 0 },
  });
  const rectangle = camera.computeViewRectangle(Ellipsoid.WGS84, new Rectangle());
  const pick = (x: number, y: number): GeoSample | null => {
    const hit = camera.pickEllipsoid(new Cartesian2(x, y), Ellipsoid.WGS84);
    if (!hit) return null;
    const point = Cartographic.fromCartesian(hit);
    return {
      lat: CesiumMath.toDegrees(point.latitude),
      lon: CesiumMath.toDegrees(point.longitude),
    };
  };
  const edges = viewEdgeSamples(width, height, pick);
  const metres = pose.heightKm * 1000;
  return {
    corners: toViewState(rectangle, null, metres).bbox,
    widened: toViewState(rectangle, null, metres, edges).bbox,
    centre: pick(width / 2, height / 2),
    edges: edges.filter((e): e is GeoSample => e !== null),
  };
}

/** Grid cells of `sizeDeg` covering a box (null: worldwide), counted as `plan_cells` does. */
function cellCount(bbox: BBox | null, sizeDeg: number): number {
  const box = bbox ?? { west: -180, south: -90, east: 180, north: 90 };
  const rows = Math.ceil(180 / sizeDeg);
  const columns = Math.ceil(360 / sizeDeg);
  const lower = (v: number, origin: number, n: number) =>
    Math.min(n - 1, Math.max(0, Math.floor((v - origin) / sizeDeg)));
  const upper = (v: number, origin: number, n: number, from: number) =>
    Math.max(from, Math.min(n - 1, Math.max(0, Math.ceil((v - origin) / sizeDeg) - 1)));
  const south = lower(box.south, -90, rows);
  const north = upper(box.north, -90, rows, south);
  const west = lower(box.west, -180, columns);
  const east =
    box.west > box.east
      ? columns + upper(box.east, -180, columns, 0)
      : upper(box.east, -180, columns, west);
  return (north - south + 1) * Math.min(east - west + 1, columns);
}

const contains = (box: BBox | null, p: GeoSample, slack = 1e-3) =>
  box === null ||
  (p.lat >= box.south - slack &&
    p.lat <= box.north + slack &&
    (box.west <= box.east
      ? p.lon >= box.west - slack && p.lon <= box.east + slack
      : p.lon >= box.west - slack || p.lon <= box.east + slack));

const POSES: Pose[] = [-90, -60, -40].flatMap((pitchDeg) =>
  [0, 30, 45, 55, 60, 70].flatMap((lat) =>
    [300, 500, 750, 1_000, 1_250, 1_500, 2_000, 3_000].map((heightKm) => ({
      lat,
      lon: 10,
      heightKm,
      pitchDeg,
    })),
  ),
);

const AIRCRAFT = { cellDeg: 5, maxCells: 4 };
const VESSELS = { cellDeg: 10, maxCells: 9 };

describe('the feed query box from Cesium’s camera', () => {
  it('never tips a view over the aircraft or vessel cell budgets by widening it', () => {
    const changed: string[] = [];
    let regional = 0;
    for (const pose of POSES) {
      const { corners, widened } = viewBoxes(pose);
      for (const [feed, { cellDeg, maxCells }] of [
        ['aircraft', AIRCRAFT],
        ['vessels', VESSELS],
      ] as const) {
        const before = cellCount(corners, cellDeg) <= maxCells;
        if (before) regional += 1;
        if (before !== cellCount(widened, cellDeg) <= maxCells) {
          changed.push(`${feed} ${JSON.stringify(pose)}`);
        }
      }
    }
    expect(regional).toBeGreaterThan(40); // the grid does cover cell-planned views
    expect(changed).toEqual([]);
  });

  it('keeps straight-down regional boxes within a degree of Cesium’s, never narrowing them', () => {
    // Tilted views grow more (up to ~4° north at 1 000 km over 70° N, pitch −60°): there the
    // corner box really does leave the far part of the screen unqueried.
    for (const pose of POSES.filter((p) => p.heightKm <= 1_000 && p.pitchDeg === -90)) {
      const { corners, widened } = viewBoxes(pose);
      if (!corners || !widened) throw new Error(`regional view has a box: ${JSON.stringify(pose)}`);
      expect(widened.south).toBeLessThanOrEqual(corners.south);
      expect(widened.north).toBeGreaterThanOrEqual(corners.north);
      expect(widened.west).toBe(corners.west);
      expect(widened.east).toBe(corners.east);
      expect(corners.south - widened.south).toBeLessThan(1);
      expect(widened.north - corners.north).toBeLessThan(1);
    }
  });

  it('queries the view’s own centre and edges, which the corner box can miss', () => {
    let missed = 0;
    for (const pose of POSES) {
      const { corners, widened, centre, edges } = viewBoxes(pose);
      if (centre && !edges.every((e) => contains(corners, e))) missed += 1;
      if (centre) expect(contains(widened, centre)).toBe(true);
      for (const edge of edges) expect(contains(widened, edge)).toBe(true);
    }
    // Tilted and wide views: Cesium's corner box leaves parts of the screen unqueried.
    expect(missed).toBeGreaterThan(0);
  });
});
