/**
 * The geofence draw-mode state machine: a pure reducer the layers panel drives from globe
 * map-point and cursor events. Polygon mode collects clicked vertices (double-click or Enter
 * closes); box mode closes itself on the second click of a two-click bounding box.
 */
import type { GeoPoint } from '../../contracts';
import { MAX_VERTICES } from '../annotations/model';

export type GeofenceDrawMode = 'polygon' | 'box';

/** A draw in progress. `closed` freezes the geometry for the naming step. */
export interface GeofenceDrawState {
  readonly mode: GeofenceDrawMode;
  readonly points: readonly GeoPoint[];
  readonly cursor: GeoPoint | null;
  readonly closed: boolean;
  readonly startedAt: number;
  readonly lastPointAt: number | null;
}

export type GeofenceDrawEvent =
  | { readonly type: 'start'; readonly mode: GeofenceDrawMode }
  | { readonly type: 'point'; readonly point: GeoPoint; readonly at: number }
  | { readonly type: 'cursor'; readonly point: GeoPoint | null }
  | { readonly type: 'undo' }
  | { readonly type: 'close' }
  | { readonly type: 'cancel' };

/** A second click this close in time and space to the previous vertex closes the polygon. */
const DOUBLE_CLICK_MS = 400;
const DOUBLE_CLICK_M = 30;

/** Reduces one draw event; `null` means no draw is active (or the draw was cancelled). */
export function geofenceDrawReducer(
  state: GeofenceDrawState | null,
  event: GeofenceDrawEvent,
): GeofenceDrawState | null {
  if (event.type === 'start') {
    return {
      mode: event.mode,
      points: [],
      cursor: null,
      closed: false,
      startedAt: Date.now(),
      lastPointAt: null,
    };
  }
  if (!state) return null;
  switch (event.type) {
    case 'cursor':
      if (state.closed) return state;
      return { ...state, cursor: event.point };
    case 'point': {
      if (state.closed) return state;
      // A second click on (near) the last vertex is a double-click: close the polygon.
      const last = state.points.at(-1);
      if (
        last &&
        state.lastPointAt !== null &&
        event.at - state.lastPointAt <= DOUBLE_CLICK_MS &&
        greatCircleDeg(last, event.point) <= DOUBLE_CLICK_M
      ) {
        return closeFrom(state);
      }
      if (state.points.length >= MAX_VERTICES) return state;
      // Box mode: the second corner defines the rectangle and finishes the draw.
      if (state.mode === 'box' && state.points.length === 1) {
        const rectangle = rectanglePoints(state.points[0] as GeoPoint, event.point);
        return rectangle ? { ...state, points: rectangle, closed: true } : state;
      }
      return { ...state, points: [...state.points, event.point], lastPointAt: event.at };
    }
    case 'undo': {
      if (state.closed || state.points.length === 0) return state;
      return { ...state, points: state.points.slice(0, -1), lastPointAt: null };
    }
    case 'close':
      return closeFrom(state);
    case 'cancel':
      return null;
  }
}

/** Whether the current geometry can close into a named region. */
export function canCloseGeofence(state: GeofenceDrawState | null): boolean {
  if (!state || state.closed) return false;
  if (state.mode === 'box')
    return state.points.length >= 2 || (state.points.length === 1 && state.cursor !== null);
  return state.points.length >= 3;
}

/** The polygon a closed draw persists: the clicked vertices, or the box's four corners. */
export function geofencePoints(state: GeofenceDrawState): readonly GeoPoint[] | null {
  if (!state.closed) return null;
  if (state.mode === 'polygon') return state.points.length >= 3 ? state.points : null;
  // Box mode stores the resolved rectangle ring the moment it closes.
  return state.points.length === 4 ? state.points : null;
}

/** What the globe previews while drawing: the chain so far, or the growing rectangle. */
export function geofencePreviewPoints(state: GeofenceDrawState): readonly GeoPoint[] {
  if (state.mode === 'polygon') return state.points;
  if (state.closed) return state.points;
  const first = state.points[0];
  if (!first) return [];
  const opposite = state.points[1] ?? state.cursor;
  return opposite ? (rectanglePoints(first, opposite) ?? [first]) : [first];
}

/** Two opposite corners of an axis-aligned (lat/lon) rectangle as an ordered ring. */
export function rectanglePoints(corner: GeoPoint, opposite: GeoPoint): readonly GeoPoint[] | null {
  const west = Math.min(corner.lon, opposite.lon);
  const east = Math.max(corner.lon, opposite.lon);
  const south = Math.min(corner.lat, opposite.lat);
  const north = Math.max(corner.lat, opposite.lat);
  // A degenerate (line or point) rectangle is not a fence the analyst named on purpose.
  if (west === east || south === north) return null;
  return [
    { lat: south, lon: west },
    { lat: south, lon: east },
    { lat: north, lon: east },
    { lat: north, lon: west },
  ];
}

function closeFrom(state: GeofenceDrawState): GeofenceDrawState {
  if (!canCloseGeofence(state)) return state;
  const points =
    state.mode === 'box'
      ? rectanglePoints(state.points[0] as GeoPoint, (state.points[1] ?? state.cursor) as GeoPoint)
      : state.points;
  return points ? { ...state, points, closed: true } : state;
}

/** Rough degrees-based distance for double-click detection only (metres at the equator). */
function greatCircleDeg(a: GeoPoint, b: GeoPoint): number {
  return Math.hypot(a.lat - b.lat, a.lon - b.lon) * 111_195;
}
