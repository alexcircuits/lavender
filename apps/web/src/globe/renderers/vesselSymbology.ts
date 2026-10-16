/**
 * Vessel symbology (docs/design/design-system.md §4: maritime hull chevron rotated to course,
 * a dot at rest) and low-zoom thinning, as pure functions with no Cesium dependency so the
 * rules are testable on their own and the performance spec can reproduce what is drawn.
 */
import type { Vessel } from '../../contracts';

/** Below this speed over ground (knots) a ship is drawn at rest, as a dot. */
export const AT_REST_KNOTS = 0.5;
/** AIS speed over ground runs 0–102.2 kn; 102.3 means "not available" (ITU-R M.1371). */
const MAX_SPEED_KNOTS = 102.3;

export type VesselGlyphKind = 'underway' | 'at-rest';

export interface VesselSymbol {
  readonly kind: VesselGlyphKind;
  /** Degrees clockwise from true north the chevron points to; null for a dot. */
  readonly bearingDeg: number | null;
}

const AT_REST: VesselSymbol = Object.freeze({ kind: 'at-rest', bearingDeg: null });

const isIn = (value: number | undefined, min: number, max: number): value is number =>
  typeof value === 'number' && Number.isFinite(value) && value >= min && value <= max;

/**
 * The symbol for a ship: a chevron along its course over ground (its true heading when no
 * course is reported) while it makes way; a dot when it is at rest, its speed is unknown or
 * it reports neither course nor heading. Out-of-range AIS values ("not available" codes from
 * a newer or misbehaving server) count as unknown.
 */
export function vesselSymbol(
  vessel: Pick<Vessel, 'speedKnots' | 'courseDeg' | 'headingDeg'>,
): VesselSymbol {
  const speed = vessel.speedKnots;
  if (!isIn(speed, 0, MAX_SPEED_KNOTS) || speed === MAX_SPEED_KNOTS || speed < AT_REST_KNOTS) {
    return AT_REST;
  }
  const course = vessel.courseDeg;
  if (isIn(course, 0, 360) && course < 360) return { kind: 'underway', bearingDeg: course };
  const heading = vessel.headingDeg;
  if (isIn(heading, 0, 360)) return { kind: 'underway', bearingDeg: heading % 360 };
  return AT_REST;
}

/** The label text for a ship (its name), or null when it has none. */
export function vesselLabel(vessel: Pick<Vessel, 'name'>): string | null {
  const name = typeof vessel.name === 'string' ? vessel.name.trim() : '';
  return name.length > 0 ? name.slice(0, 20) : null;
}

/** At most this many unselected ships carry a name label. */
export const VESSEL_LABEL_BUDGET = 60;
/** Unselected names only appear when the camera is below this height (ships crowd ports). */
export const VESSEL_LABEL_MAX_HEIGHT_M = 300_000;

// ── Low-zoom thinning ─────────────────────────────────────────────────────────────────────
//
// The vessels snapshot carries counts (ships in view, ships known before filtering) but no
// density grid, so clutter is reduced on the client. Above THIN_MIN_HEIGHT_M the globe is
// divided into cells about THIN_CELL_PX screen pixels wide at the centre of a straight-down
// view of the actual canvas, and each cell draws one ship: the selected one, else one making
// way, else the lowest MMSI, so the choice is stable from snapshot to snapshot. The feed's
// count stays the number of ships in view (thinning never changes what the layer reports),
// and the layer legend says when wide views draw one ship per crowded spot. Below the
// threshold every ship is drawn.

/** At or below this camera height (a view about 1 150 km wide) every ship is drawn. */
export const THIN_MIN_HEIGHT_M = 1_000_000;
/**
 * Cell width in CSS pixels at the centre of a straight-down view: a hull chevron's length
 * (vesselGlyphs.ts) plus a gap, so the ships that remain stand apart instead of touching.
 */
export const THIN_CELL_PX = 24;
/** The canvas side assumed when the renderer is not told its size (the 1440×900 reference). */
export const REFERENCE_CANVAS_PX = 1440;
/**
 * Cesium's perspective frustum: a 60° field of view across the canvas's longer side, so one
 * CSS pixel at the centre of a straight-down view covers 2·tan(30°)·height / side metres.
 */
const VIEW_SPAN_PER_HEIGHT = 2 * Math.tan(Math.PI / 6);
const METRES_PER_DEGREE_LAT = 111_320;

/**
 * The thinning cell size in degrees of latitude for a camera height and the canvas's longer
 * side in CSS pixels, or null when every ship is drawn. Heights snap to powers of two above
 * the threshold, so the cells only change when the zoom roughly doubles or halves, not on
 * every small camera move. It is exact at the centre of a straight-down view; towards the
 * horizon cells cover fewer pixels.
 */
export function thinningCellDeg(
  cameraHeightM: number,
  canvasLongSidePx: number = REFERENCE_CANVAS_PX,
): number | null {
  if (!Number.isFinite(cameraHeightM) || cameraHeightM <= THIN_MIN_HEIGHT_M) return null;
  const side =
    Number.isFinite(canvasLongSidePx) && canvasLongSidePx > 0
      ? canvasLongSidePx
      : REFERENCE_CANVAS_PX;
  const level = Math.round(Math.log2(cameraHeightM / THIN_MIN_HEIGHT_M));
  const snappedHeight = THIN_MIN_HEIGHT_M * 2 ** level;
  const cellM = (THIN_CELL_PX * VIEW_SPAN_PER_HEIGHT * snappedHeight) / side;
  return cellM / METRES_PER_DEGREE_LAT;
}

export interface ThinCandidate {
  readonly lat: number;
  readonly lon: number;
  readonly underway: boolean;
}

/**
 * Numeric cell key: a latitude row, then a longitude column widened towards the poles so
 * cells stay roughly square on the ground. `columns` bounds the column index of any row.
 */
function cellKey(lat: number, lon: number, cellDeg: number, columns: number): number {
  const row = Math.floor((lat + 90) / cellDeg);
  const rowCentre = Math.min(89.9, Math.abs(-90 + (row + 0.5) * cellDeg));
  const lonCell = cellDeg / Math.max(Math.cos((rowCentre * Math.PI) / 180), 0.01);
  return row * columns + Math.floor((lon + 180) / lonCell);
}

/** A ship that thinning can keep or hide, with its id (the MMSI) to break ties. */
export interface ThinShip extends ThinCandidate {
  readonly id: string;
}

/** Whether `a` represents its cell rather than `b` (selection, then under way, then MMSI). */
function outranks(a: ThinShip, b: ThinShip, selected: string | null): boolean {
  if (a.id === selected) return true;
  if (b.id === selected) return false;
  if (a.underway !== b.underway) return a.underway;
  return a.id < b.id;
}

/**
 * The ships drawn at `cellDeg`: one per cell, the very objects passed in. It allocates one
 * map entry per occupied cell and nothing per hidden ship, since the renderer runs it over
 * every ship on each snapshot of a wide view.
 */
export function thinningKeepers<S extends ThinShip>(
  ships: Iterable<S>,
  cellDeg: number,
  selectedId: string | null,
): Set<S> {
  if (!(cellDeg > 0)) return new Set(ships);
  const columns = Math.ceil(360 / cellDeg) + 1;
  const kept = new Map<number, S>();
  for (const ship of ships) {
    const key = cellKey(ship.lat, ship.lon, cellDeg, columns);
    const current = kept.get(key);
    if (!current || outranks(ship, current, selectedId)) kept.set(key, ship);
  }
  return new Set(kept.values());
}

/**
 * The ids to hide at `cellDeg` (null cell size hides nothing): every ship except one per
 * cell. The selected ship is never hidden.
 */
export function thinnedOut(
  candidates: Iterable<readonly [string, ThinCandidate]>,
  cellDeg: number | null,
  selectedId: string | null,
): Set<string> {
  if (cellDeg === null || !(cellDeg > 0)) return new Set();
  const ships = Array.from(candidates, ([id, candidate]) => ({ ...candidate, id }));
  const kept = thinningKeepers(ships, cellDeg, selectedId);
  return new Set(ships.filter((ship) => !kept.has(ship)).map((ship) => ship.id));
}
