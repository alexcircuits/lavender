/**
 * Camera commands (fly-to, home, zoom, north-up) as engine-free logic over a narrow
 * `CameraPort`; `cesiumCamera.ts` adapts the port to Cesium. Flights run 1.2 s with the
 * standard easing and cut (duration 0) under `prefers-reduced-motion: reduce`
 * (design-system §9, §14).
 */
import type { BBox } from '../contracts';
import type { FlyTarget } from './index';
import { lonSpan, wrapLongitude } from './view';

export const FLIGHT_DURATION_S = 1.2;
export const NORTH_UP_DURATION_S = 0.6;
/** Default camera height for point targets (search results, coordinates). */
export const DEFAULT_POINT_HEIGHT_M = 400_000;
/** Home: the whole Earth, centred on the Atlantic/Europe/Africa longitudes. */
export const HOME_VIEW = { lon: 10, lat: 20, heightM: 20_000_000 } as const;
export const MIN_HEIGHT_M = 100;
export const MAX_HEIGHT_M = 40_000_000;
/** Zoom in halves the height; zoom out doubles it. */
export const ZOOM_IN_FRACTION = 0.5;
export const ZOOM_OUT_FRACTION = 1;
/** Boxes get this much margin and at least this span (degrees) so the camera never dives. */
const BBOX_MARGIN = 0.1;
const MIN_BBOX_SPAN_DEG = 0.02;

/** Where a flight ends, in degrees. A rectangle with `west > east` crosses the antimeridian. */
export type Destination =
  | { readonly kind: 'point'; readonly lon: number; readonly lat: number; readonly heightM: number }
  | {
      readonly kind: 'rectangle';
      readonly west: number;
      readonly south: number;
      readonly east: number;
      readonly north: number;
    };

/** The few camera operations the service needs; implemented over Cesium in cesiumCamera.ts. */
export interface CameraPort {
  heightM(): number;
  pitchRad(): number;
  /** Flies (or cuts, for 0 s) to a destination looking straight down, north up. */
  flyTo(destination: Destination, durationS: number): void;
  /** Turns to a heading/pitch in place, keeping the camera position. */
  orient(headingRad: number, pitchRad: number, durationS: number): void;
  /** Moves along the view direction by `amountM` (negative moves away). */
  zoom(amountM: number): void;
}

export function flightDuration(reducedMotion: boolean, durationS = FLIGHT_DURATION_S): number {
  return reducedMotion ? 0 : durationS;
}

const clampHeight = (heightM: number): number =>
  Math.min(MAX_HEIGHT_M, Math.max(MIN_HEIGHT_M, heightM));

const clampLat = (degrees: number): number => Math.min(90, Math.max(-90, degrees));

/** A bbox with margin and a minimum span, latitudes clamped, antimeridian preserved. */
export function paddedRectangle(bbox: BBox): Destination {
  const span = lonSpan(bbox.west, bbox.east);
  const lowLat = clampLat(Math.min(bbox.south, bbox.north));
  const highLat = clampLat(Math.max(bbox.south, bbox.north));
  const height = highLat - lowLat;
  const padLon = Math.max(span * BBOX_MARGIN, (MIN_BBOX_SPAN_DEG - span) / 2, 0);
  const padLat = Math.max(height * BBOX_MARGIN, (MIN_BBOX_SPAN_DEG - height) / 2, 0);
  const south = clampLat(lowLat - padLat);
  const north = clampLat(highLat + padLat);
  if (span + 2 * padLon >= 360) return { kind: 'rectangle', west: -180, south, east: 180, north };
  return {
    kind: 'rectangle',
    west: wrapLongitude(bbox.west - padLon),
    south,
    east: wrapLongitude(bbox.east + padLon),
    north,
  };
}

/** The flight destination for a target, or null when its coordinates are not usable. */
export function destinationFor(target: FlyTarget): Destination | null {
  if (target.kind === 'point') {
    const { lat, lon } = target.position;
    if (!Number.isFinite(lat) || !Number.isFinite(lon) || Math.abs(lat) > 90) return null;
    const heightM = target.heightM ?? DEFAULT_POINT_HEIGHT_M;
    return {
      kind: 'point',
      lon: wrapLongitude(lon),
      lat,
      heightM: clampHeight(Number.isFinite(heightM) ? heightM : DEFAULT_POINT_HEIGHT_M),
    };
  }
  const { west, south, east, north } = target.bbox;
  if (![west, south, east, north].every(Number.isFinite)) return null;
  if (Math.abs(south) > 90 || Math.abs(north) > 90) return null;
  const rectangle = paddedRectangle(target.bbox);
  return rectangle.kind === 'rectangle' && rectangle.north > rectangle.south ? rectangle : null;
}

/** Zoom distance proportional to the current height, bounded by the height limits. */
export function zoomAmount(heightM: number, direction: 'in' | 'out'): number {
  const height = Number.isFinite(heightM) ? Math.max(0, heightM) : 0;
  if (direction === 'in') {
    return Math.max(0, Math.min(height * ZOOM_IN_FRACTION, height - MIN_HEIGHT_M));
  }
  return -Math.max(0, Math.min(height * ZOOM_OUT_FRACTION, MAX_HEIGHT_M - height));
}

/**
 * The design system's standard easing, `cubic-bezier(.2, 0, 0, 1)` (§14), as a function of
 * normalised time. Solves the bezier's x(t) = time by bisection (monotonic, 24 steps).
 */
export function standardEasing(time: number): number {
  if (time <= 0) return 0;
  if (time >= 1) return 1;
  const [x1, y1, x2, y2] = [0.2, 0, 0, 1];
  const bezier = (t: number, p1: number, p2: number) =>
    3 * (1 - t) ** 2 * t * p1 + 3 * (1 - t) * t ** 2 * p2 + t ** 3;
  let low = 0;
  let high = 1;
  let t = time;
  for (let i = 0; i < 24; i += 1) {
    t = (low + high) / 2;
    if (bezier(t, x1, x2) < time) low = t;
    else high = t;
  }
  return bezier(t, y1, y2);
}

export class CameraService {
  readonly #port: CameraPort;
  readonly #reducedMotion: () => boolean;

  constructor(port: CameraPort, prefersReducedMotion: () => boolean) {
    this.#port = port;
    this.#reducedMotion = prefersReducedMotion;
  }

  flyTo(target: FlyTarget): void {
    const destination = destinationFor(target);
    if (!destination) {
      console.warn('Globe: ignored a fly-to target with invalid coordinates', target);
      return;
    }
    this.#port.flyTo(destination, flightDuration(this.#reducedMotion()));
  }

  home(): void {
    this.#port.flyTo({ kind: 'point', ...HOME_VIEW }, flightDuration(this.#reducedMotion()));
  }

  zoomIn(): void {
    const amount = zoomAmount(this.#port.heightM(), 'in');
    if (amount > 0) this.#port.zoom(amount);
  }

  zoomOut(): void {
    const amount = zoomAmount(this.#port.heightM(), 'out');
    if (amount < 0) this.#port.zoom(amount);
  }

  resetNorth(): void {
    this.#port.orient(
      0,
      this.#port.pitchRad(),
      flightDuration(this.#reducedMotion(), NORTH_UP_DURATION_S),
    );
  }
}

/** Live `prefers-reduced-motion: reduce`, evaluated per command so a change applies at once. */
export function prefersReducedMotion(): boolean {
  return typeof window.matchMedia === 'function'
    ? window.matchMedia('(prefers-reduced-motion: reduce)').matches
    : false;
}
