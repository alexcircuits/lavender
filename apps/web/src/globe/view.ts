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

export function toViewState(
  rectangle: RadianRectangle | undefined,
  centre: GeoPoint | null,
  heightM: number,
): ViewState {
  return {
    bbox: bboxFromRectangle(rectangle),
    centre: centre ? { lat: round(centre.lat), lon: round(wrapLongitude(centre.lon)) } : null,
    heightM: Number.isFinite(heightM) ? Math.max(0, heightM) : 0,
  };
}
