import { describe, expect, it } from 'vitest';
import type { BBox } from '../contracts';
import {
  bboxFromRectangle,
  type GeoSample,
  lonSpan,
  sameView,
  toViewState,
  viewEdgeSamples,
  widenToEdges,
  wrapLongitude,
} from './view';

const rad = (deg: number) => (deg * Math.PI) / 180;
const rect = (west: number, south: number, east: number, north: number) => ({
  west: rad(west),
  south: rad(south),
  east: rad(east),
  north: rad(north),
});

describe('view rectangle → bbox', () => {
  it('converts a regional view to degrees', () => {
    expect(bboxFromRectangle(rect(-10, 35, 20, 60))).toEqual({
      west: -10,
      south: 35,
      east: 20,
      north: 60,
    });
  });

  it('is null when the camera sees no ellipsoid', () => {
    expect(bboxFromRectangle(undefined)).toBeNull();
  });

  it("is null for Cesium's whole-globe rectangle (horizon or space in view)", () => {
    expect(bboxFromRectangle(rect(-180, -90, 180, 90))).toBeNull();
  });

  it('is null when the view is roughly a hemisphere or wider', () => {
    expect(bboxFromRectangle(rect(-100, -10, 85, 10))).toBeNull(); // 185° of longitude
    expect(bboxFromRectangle(rect(-20, -50, 20, 45))).toBeNull(); // 95° of latitude
    expect(bboxFromRectangle(rect(-89, -40, 89, 40))).not.toBeNull(); // 178° × 80°
  });

  it('keeps west > east for a view across the antimeridian', () => {
    expect(bboxFromRectangle(rect(170, -20, -170, 10))).toEqual({
      west: 170,
      south: -20,
      east: -170,
      north: 10,
    });
  });

  it('is null for a wide view across the antimeridian', () => {
    expect(bboxFromRectangle(rect(60, -20, -60, 10))).toBeNull(); // 240° of longitude
  });

  it('wraps out-of-range longitudes and rounds away float noise', () => {
    expect(bboxFromRectangle(rect(175, 0, 185, 5))).toEqual({
      west: 175,
      south: 0,
      east: -175,
      north: 5,
    });
    const box = bboxFromRectangle(rect(1.234567891, 2, 3, 4));
    expect(box?.west).toBe(1.2346);
  });

  it('rejects non-finite or degenerate rectangles', () => {
    expect(bboxFromRectangle(rect(Number.NaN, 0, 1, 1))).toBeNull();
    expect(bboxFromRectangle(rect(10, 5, 10, 6))).toBeNull();
  });
});

describe('ViewState', () => {
  it('deduplicates equal normalised views but preserves centre, height and bounds changes', () => {
    const view = toViewState(rect(0, 0, 10, 10), { lat: 5, lon: 5 }, 1_200_000);
    expect(sameView(null, view)).toBe(false);
    expect(sameView(view, toViewState(rect(0, 0, 10, 10), { lat: 5, lon: 5 }, 1_200_000))).toBe(
      true,
    );
    expect(sameView(view, { ...view, heightM: view.heightM + 1 })).toBe(false);
    expect(sameView(view, { ...view, centre: null })).toBe(false);
    expect(sameView(view, { ...view, centre: { lat: 6, lon: 5 } })).toBe(false);
    expect(sameView(view, { ...view, bbox: null })).toBe(false);
    expect(sameView(view, toViewState(rect(0, 0, 11, 10), { lat: 5, lon: 5 }, 1_200_000))).toBe(
      false,
    );
  });

  it('carries centre and height', () => {
    const view = toViewState(rect(0, 0, 10, 10), { lat: 5.000001, lon: 365 }, 1_200_000);
    expect(view).toEqual({
      bbox: { west: 0, south: 0, east: 10, north: 10 },
      centre: { lat: 5, lon: 5 },
      heightM: 1_200_000,
    });
  });

  it('allows a null centre (sky at the canvas centre) and sanitises height', () => {
    expect(toViewState(undefined, null, Number.NaN)).toEqual({
      bbox: null,
      centre: null,
      heightM: 0,
    });
  });
});

describe('longitude helpers', () => {
  it('wraps into [-180, 180]', () => {
    expect(wrapLongitude(190)).toBe(-170);
    expect(wrapLongitude(-190)).toBe(170);
    expect(wrapLongitude(540)).toBe(180);
    expect(wrapLongitude(-45)).toBe(-45);
  });

  it('measures spans across the antimeridian', () => {
    expect(lonSpan(-10, 20)).toBe(30);
    expect(lonSpan(170, -170)).toBe(20);
  });
});

/**
 * A straight-down pinhole camera over a spherical Earth, like Cesium's 60° perspective
 * frustum (the field of view spans the wider canvas side): the screen point (x, y) → the
 * point of the globe it shows, or null when it shows sky.
 */
function straightDown(lat: number, lon: number, heightM: number, width: number, height: number) {
  const R = 6_371_000;
  const [phi, lambda] = [rad(lat), rad(lon)];
  const n = [Math.cos(phi) * Math.cos(lambda), Math.cos(phi) * Math.sin(lambda), Math.sin(phi)];
  const east = [-Math.sin(lambda), Math.cos(lambda), 0];
  const north = [
    -Math.sin(phi) * Math.cos(lambda),
    -Math.sin(phi) * Math.sin(lambda),
    Math.cos(phi),
  ];
  const camera = n.map((c) => c * (R + heightM));
  const tanX = Math.tan(rad(30));
  const tanY = (tanX * height) / width;
  return (x: number, y: number): GeoSample | null => {
    const sx = tanX * ((2 * x) / width - 1);
    const sy = tanY * (1 - (2 * y) / height);
    const raw = [0, 1, 2].map((i) => -(n[i] ?? 0) + (east[i] ?? 0) * sx + (north[i] ?? 0) * sy);
    const length = Math.hypot(...raw);
    const d = raw.map((c) => c / length);
    const pd = camera.reduce((sum, c, i) => sum + c * (d[i] ?? 0), 0);
    const disc = pd * pd - (camera.reduce((sum, c) => sum + c * c, 0) - R * R);
    if (disc < 0) return null;
    const t = -pd - Math.sqrt(disc);
    const [hx, hy, hz] = camera.map((c, i) => c + t * (d[i] ?? 0)) as [number, number, number];
    return { lat: (Math.asin(hz / R) * 180) / Math.PI, lon: (Math.atan2(hy, hx) * 180) / Math.PI };
  };
}

/** What Cesium's computeViewRectangle sees: only the four screen corners. */
function cornersOnly(pick: (x: number, y: number) => GeoSample | null, w: number, h: number) {
  const corners = [pick(0, 0), pick(0, h), pick(w, h), pick(w, 0)].filter(
    (c): c is GeoSample => c !== null,
  );
  return rect(
    Math.min(...corners.map((c) => c.lon)),
    Math.min(...corners.map((c) => c.lat)),
    Math.max(...corners.map((c) => c.lon)),
    Math.max(...corners.map((c) => c.lat)),
  );
}

const contains = (box: BBox | null, lat: number, lon: number) =>
  box !== null && lat >= box.south && lat <= box.north && lon >= box.west && lon <= box.east;

describe('screen-edge widening of the view rectangle', () => {
  it('keeps high-latitude ships in a continental view (corners alone skew the box south)', () => {
    const [w, h] = [1280, 720];
    const pick = straightDown(60, 24.68, 5_000_000, w, h);
    const corners = cornersOnly(pick, w, h);
    // The bug: the corner box misses the centre of the view and the Gulf of Finland.
    expect(contains(bboxFromRectangle(corners), 60, 24.68)).toBe(false);

    const view = toViewState(
      corners,
      { lat: 60, lon: 24.68 },
      5_000_000,
      viewEdgeSamples(w, h, pick),
    );
    for (const [lat, lon] of [
      [60, 24.68],
      [59.5, 24.4],
      [60.15, 24.96],
    ] as const) {
      expect(contains(view.bbox, lat, lon)).toBe(true);
    }
    // The top edge reaches furthest north (bboxes are rounded to 1e-4°).
    expect(view.bbox?.north).toBeGreaterThanOrEqual((pick(w / 2, 0)?.lat ?? 90) - 1e-4);
    expect(view.bbox?.south).toBeLessThanOrEqual(corners.south * (180 / Math.PI) + 1e-4);
  });

  it('only widens a regional view, by the small curvature of its edges', () => {
    const [w, h] = [1280, 720];
    const pick = straightDown(20, -60, 400_000, w, h);
    const before = bboxFromRectangle(cornersOnly(pick, w, h));
    const after = bboxFromRectangle(
      widenToEdges(cornersOnly(pick, w, h), viewEdgeSamples(w, h, pick)),
    );
    if (!before || !after) throw new Error('regional views have a bbox');
    expect(after.north).toBeGreaterThanOrEqual(before.north);
    expect(after.south).toBeLessThanOrEqual(before.south);
    expect(after.west).toBeLessThanOrEqual(before.west);
    expect(after.east).toBeGreaterThanOrEqual(before.east);
    for (const side of ['west', 'south', 'east', 'north'] as const) {
      expect(Math.abs(after[side] - before[side])).toBeLessThan(0.05);
    }
  });

  it('widens longitude the short way across the antimeridian and ignores sky samples', () => {
    const widened = widenToEdges(rect(170, -20, -175, 10), [
      { lat: 0, lon: 168 },
      null,
      { lat: 12, lon: -172 },
      { lat: -25, lon: 179 },
    ]);
    expect(bboxFromRectangle(widened)).toEqual({ west: 168, south: -25, east: -172, north: 12 });
  });

  it('opens the box to the pole when the screen edges circle it', () => {
    const [w, h] = [1280, 720];
    const pick = straightDown(88, 0, 2_000_000, w, h);
    const widened = widenToEdges(cornersOnly(pick, w, h), viewEdgeSamples(w, h, pick));
    expect(widened?.north).toBeCloseTo(Math.PI / 2);
    expect(widened?.west).toBeCloseTo(-Math.PI);
    expect(widened?.east).toBeCloseTo(Math.PI);
  });

  it('follows edges that show sky to the horizon, where a tilted view sees furthest', () => {
    // A view tilted towards the north: sky above y = 300 px, ground further north the
    // nearer it is to the horizon.
    const pick = (x: number, y: number): GeoSample | null =>
      y < 300 ? null : { lat: 60 - y / 100, lon: 20 + x / 1000 };
    const samples = viewEdgeSamples(1280, 720, pick);
    // The edge loop first, sky as null, so ordered logic (pole circling) still sees it.
    expect(samples.slice(0, 32).filter((s) => s === null).length).toBeGreaterThan(8);
    const widened = bboxFromRectangle(widenToEdges(rect(20, 52.8, 21.28, 55), samples));
    expect(widened?.north).toBeCloseTo(57, 1); // the horizon, 300 px down
    // Where the whole line shows sky there is no horizon to find.
    expect(viewEdgeSamples(1280, 720, () => null).every((s) => s === null)).toBe(true);
  });

  it('leaves "no globe in view" and empty samples alone', () => {
    expect(widenToEdges(undefined, [{ lat: 1, lon: 1 }])).toBeUndefined();
    expect(widenToEdges(rect(0, 0, 10, 10), [])).toEqual(rect(0, 0, 10, 10));
    expect(viewEdgeSamples(0, 720, () => ({ lat: 0, lon: 0 }))).toEqual([]);
  });
});
