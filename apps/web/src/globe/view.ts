/**
 * Camera view → `ViewState` normalisation (pure; the Cesium side lives in GlobeEngine). The
 * bbox drives server-side spatial filtering, so it is null whenever the view is roughly a
 * hemisphere or more: feeds then query worldwide instead of an ever-growing box.
 */
import type { BBox, GeoPoint } from '../contracts';
import type { ViewState } from './index';

/** A rectangle in radians as Cesium returns it; `west > east` crosses the antimeridian. */
export interface RadianRectangle {
  readonly west: number;
  readonly south: number;
  readonly east: number;
  readonly north: number;
}

/** Views at least this wide in longitude are treated as global. */
export const MAX_BBOX_LON_SPAN_DEG = 180;
/** Views at least this tall in latitude are treated as global. */
export const MAX_BBOX_LAT_SPAN_DEG = 90;
/** Degrees are rounded to 1e-4 (~11 m), so tiny camera jitter does not churn queries. */
const DECIMALS = 1e4;

const toDegrees = (radians: number): number => (radians * 180) / Math.PI;
const round = (degrees: number): number => Math.round(degrees * DECIMALS) / DECIMALS;

/** Wraps a longitude into [-180, 180]. */
export function wrapLongitude(degrees: number): number {
  if (degrees >= -180 && degrees <= 180) return degrees;
  const wrapped = ((((degrees + 180) % 360) + 360) % 360) - 180;
  return wrapped === -180 && degrees > 0 ? 180 : wrapped;
}

const clampLatitude = (degrees: number): number => Math.min(90, Math.max(-90, degrees));

/** Query views already contain rounded coordinates; repeated SDK events publish once. */
export function sameView(previous: ViewState | null, next: ViewState): boolean {
  return (
    previous !== null &&
    previous.heightM === next.heightM &&
    previous.centre?.lat === next.centre?.lat &&
    previous.centre?.lon === next.centre?.lon &&
    previous.bbox?.west === next.bbox?.west &&
    previous.bbox?.south === next.bbox?.south &&
    previous.bbox?.east === next.bbox?.east &&
    previous.bbox?.north === next.bbox?.north
  );
}

/** Longitude span of a box in degrees, honouring antimeridian crossing. */
export function lonSpan(west: number, east: number): number {
  return east >= west ? east - west : east + 360 - west;
}

/** The query bbox for a visible rectangle, or null for "worldwide". */
export function bboxFromRectangle(rectangle: RadianRectangle | undefined): BBox | null {
  if (!rectangle) return null;
  const values = [rectangle.west, rectangle.south, rectangle.east, rectangle.north];
  if (!values.every(Number.isFinite)) return null;
  const rawWest = toDegrees(rectangle.west);
  const rawEast = toDegrees(rectangle.east);
  const rawSpan = rectangle.east >= rectangle.west ? rawEast - rawWest : rawEast + 360 - rawWest;
  if (rawSpan >= MAX_BBOX_LON_SPAN_DEG || rawSpan <= 0) return null;
  const south = clampLatitude(round(toDegrees(rectangle.south)));
  const north = clampLatitude(round(toDegrees(rectangle.north)));
  if (north - south >= MAX_BBOX_LAT_SPAN_DEG || north <= south) return null;
  return {
    west: round(wrapLongitude(rawWest)),
    south,
    east: round(wrapLongitude(rawEast)),
    north,
  };
}

/** A point of the globe in degrees, as a screen pick returns it. */
export interface GeoSample {
  readonly lat: number;
  readonly lon: number;
}

/**
 * Screen-edge samples per side, corners included (cheap: one ray-ellipsoid test each). Odd,
 * so the middle of each edge, where a north-up view reaches furthest north or south, is one.
 */
const EDGE_SAMPLES_PER_SIDE = 9;
/** Halvings in the search for where a line from a sky sample meets the globe (~1 px). */
const HORIZON_SEARCH_STEPS = 11;
const TWO_PI = 2 * Math.PI;
const wrapRadians = (r: number) => (r > Math.PI ? r - TWO_PI : r < -Math.PI ? r + TWO_PI : r);

type ScreenPick = (x: number, y: number) => GeoSample | null;

/**
 * Picks along the four screen edges, in order around the screen (top left → right, right
 * edge down, bottom right → left, left edge up); null where an edge shows sky.
 *
 * Where an edge shows sky, the visible globe ends at the horizon inside the screen, and a
 * tilted view's furthest ground (sometimes even its centre) lies there, beyond every edge
 * that hits the globe. So for each sky sample the line from it through the screen centre is
 * searched for the horizon, and those points follow the edge loop.
 */
export function viewEdgeSamples(
  width: number,
  height: number,
  pick: ScreenPick,
  perSide = EDGE_SAMPLES_PER_SIDE,
): (GeoSample | null)[] {
  if (!(width > 0) || !(height > 0) || perSide < 2) return [];
  const steps = perSide - 1;
  const points: (readonly [number, number])[] = [];
  for (let i = 0; i < steps; i += 1) points.push([(width * i) / steps, 0]);
  for (let i = 0; i < steps; i += 1) points.push([width, (height * i) / steps]);
  for (let i = 0; i < steps; i += 1) points.push([width * (1 - i / steps), height]);
  for (let i = 0; i < steps; i += 1) points.push([0, height * (1 - i / steps)]);
  const samples = points.map(([x, y]) => pick(x, y));
  const horizon: GeoSample[] = [];
  points.forEach(([x, y], i) => {
    if (samples[i] !== null) return;
    const hit = horizonAlong(pick, x, y, width - x, height - y);
    if (hit) horizon.push(hit);
  });
  return [...samples, ...horizon];
}

/**
 * Where the screen line from (x0, y0), which shows sky, to (x1, y1) first meets the globe, or
 * null when (x1, y1) shows sky too. The globe's outline is convex, so the line crosses it
 * once and a bisection finds the crossing.
 */
function horizonAlong(
  pick: ScreenPick,
  x0: number,
  y0: number,
  x1: number,
  y1: number,
): GeoSample | null {
  let ground = pick(x1, y1);
  if (!ground) return null;
  let sky = 0;
  let hit = 1;
  for (let i = 0; i < HORIZON_SEARCH_STEPS; i += 1) {
    const t = (sky + hit) / 2;
    const point = pick(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
    if (point) {
      hit = t;
      ground = point;
    } else {
      sky = t;
    }
  }
  return ground;
}

/**
 * Cesium's `computeViewRectangle` bounds only the four screen corners (or the horizon where
 * a corner shows sky). In a wide view the globe's curvature puts the northern- or southern-
 * most visible point in the middle of an edge, far beyond the corners: at 5 000 km over 60° N
 * the corner box ended near 51° N, so the view's own centre fell outside the feed query.
 * This widens the rectangle to every edge sample that hits the globe (longitude the short
 * way, across the antimeridian if need be), and opens it to the pole when the edges circle
 * one. It never narrows the rectangle; `bboxFromRectangle` still turns a hemisphere-sized
 * result into "worldwide".
 */
export function widenToEdges(
  rectangle: RadianRectangle | undefined,
  samples: readonly (GeoSample | null)[],
): RadianRectangle | undefined {
  if (!rectangle) return undefined;
  let { west, south, east, north } = rectangle;
  const hits = samples.filter(
    (s): s is GeoSample => s !== null && Number.isFinite(s.lat) && Number.isFinite(s.lon),
  );
  for (const sample of hits) {
    const lat = (sample.lat * Math.PI) / 180;
    const lon = wrapRadians((sample.lon * Math.PI) / 180);
    south = Math.min(south, lat);
    north = Math.max(north, lat);
    const inside = west <= east ? lon >= west && lon <= east : lon >= west || lon <= east;
    if (inside) continue;
    const towardsWest = (((west - lon) % TWO_PI) + TWO_PI) % TWO_PI;
    const towardsEast = (((lon - east) % TWO_PI) + TWO_PI) % TWO_PI;
    if (towardsWest <= towardsEast) west = wrapRadians(west - towardsWest);
    else east = wrapRadians(east + towardsEast);
  }
  if (hits.length === samples.length && hits.length > 2 && circlesPole(hits)) {
    west = -Math.PI;
    east = Math.PI;
    const meanLat = hits.reduce((sum, s) => sum + s.lat, 0) / hits.length;
    if (meanLat >= 0) north = Math.PI / 2;
    else south = -Math.PI / 2;
  }
  return { west, south, east, north };
}

/** Whether a closed loop of points winds once around a pole (longitude turns through 360°). */
function circlesPole(loop: readonly GeoSample[]): boolean {
  let turned = 0;
  for (let i = 0; i < loop.length; i += 1) {
    const from = loop[i]?.lon ?? 0;
    const to = loop[(i + 1) % loop.length]?.lon ?? 0;
    turned += wrapLongitude(to - from);
  }
  return Math.abs(turned) > 180;
}

export function toViewState(
  rectangle: RadianRectangle | undefined,
  centre: GeoPoint | null,
  heightM: number,
  edgeSamples: readonly (GeoSample | null)[] = [],
): ViewState {
  return {
    bbox: bboxFromRectangle(widenToEdges(rectangle, edgeSamples)),
    centre: centre ? { lat: round(centre.lat), lon: round(wrapLongitude(centre.lon)) } : null,
    heightM: Number.isFinite(heightM) ? Math.max(0, heightM) : 0,
  };
}
