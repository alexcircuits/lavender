/**
 * Map symbology rules (docs/design/design-system.md §4, §11) as pure functions: altitude and
 * rotation of aircraft, magnitude → size and ramp step of earthquakes, FRP → size and ramp
 * step of fires, stale opacity and the label budget. Which fire cells are drawable is the
 * shared rule in feeds/fireCells.ts.
 */
import type { Aircraft, Freshness } from '../../contracts';

export const FEET_TO_METRES = 0.3048;
/** Stale symbols draw at 60 % opacity (§11). */
export const STALE_ALPHA = 0.6;

/** Whether an item's position can be drawn (finite, latitude within ±90°). */
export function hasValidPosition(item: {
  readonly position: { readonly lat: number; readonly lon: number };
}): boolean {
  const { lat, lon } = item.position;
  return Number.isFinite(lat) && Number.isFinite(lon) && Math.abs(lat) <= 90;
}

/** Aircraft height above the ellipsoid in metres: geometric, else barometric; 0 on ground. */
export function aircraftAltitudeM(
  aircraft: Pick<Aircraft, 'onGround' | 'altitudeGeomFt' | 'altitudeBaroFt'>,
): number {
  if (aircraft.onGround) return 0;
  const feet = aircraft.altitudeGeomFt ?? aircraft.altitudeBaroFt;
  if (feet === undefined || !Number.isFinite(feet)) return 0;
  // Below the Dead Sea shore is a data error, not a flight level.
  return Math.max(-500, feet * FEET_TO_METRES);
}

/**
 * Billboard rotation for a track. Cesium rotates counter-clockwise from the aligned axis
 * (local north); tracks run clockwise from north. Unknown tracks point north.
 */
export function trackRotationRad(trackDeg: number | undefined): number {
  if (trackDeg === undefined || !Number.isFinite(trackDeg)) return 0;
  const normalised = ((trackDeg % 360) + 360) % 360;
  return normalised === 0 ? 0 : -(normalised * Math.PI) / 180;
}

export const QUAKE_MIN_PX = 6;
export const QUAKE_MAX_PX = 24;

/** Point diameter for a magnitude: 4 px + 2.5 px per unit, clamped to 6–24 px. */
export function quakeSizePx(magnitude: number): number {
  if (!Number.isFinite(magnitude)) return QUAKE_MIN_PX;
  return Math.min(QUAKE_MAX_PX, Math.max(QUAKE_MIN_PX, 4 + 2.5 * magnitude));
}

/** Step of the five-colour hazard ramp: < 3, 3–4, 4–5, 5–6, ≥ 6 → 0..4. */
export function quakeRampIndex(magnitude: number): 0 | 1 | 2 | 3 | 4 {
  if (!Number.isFinite(magnitude) || magnitude < 3) return 0;
  if (magnitude < 4) return 1;
  if (magnitude < 5) return 2;
  if (magnitude < 6) return 3;
  return 4;
}

export const FIRE_MIN_PX = 6;
export const FIRE_MAX_PX = 24;

/**
 * Side of a fire's square cell in CSS pixels: a base from peak FRP (6–18 px) plus cluster
 * scaling (0–6 px) for count > 1, clamped to 6–24 px.
 */
export function fireSizePx(count: number, maxFrp: number): number {
  const safeCount = Number.isFinite(count) && count > 0 ? count : 1;
  const safeFrp = Number.isFinite(maxFrp) && maxFrp > 0 ? maxFrp : 0;
  const base = 6 + Math.min(12, Math.sqrt(safeFrp) * 0.8);
  const clusterBonus = safeCount > 1 ? Math.min(6, Math.log2(safeCount) * 1.2) : 0;
  return Math.min(FIRE_MAX_PX, Math.max(FIRE_MIN_PX, base + clusterBonus));
}

/** Step of the five-colour hazard ramp: < 10, 10–50, 50–100, 100–250, ≥ 250 MW → 0..4. */
export function fireRampIndex(frp: number): 0 | 1 | 2 | 3 | 4 {
  if (!Number.isFinite(frp) || frp < 10) return 0;
  if (frp < 50) return 1;
  if (frp < 100) return 2;
  if (frp < 250) return 3;
  return 4;
}

/** Whether a feed's symbols get the stale treatment (stale, or unavailable with last data). */
export function isStale(freshness: Freshness): boolean {
  return freshness.state === 'stale' || freshness.state === 'unavailable';
}

/** The label text for an aircraft (callsign, else registration), or null. */
export function aircraftLabel(
  aircraft: Pick<Aircraft, 'callsign' | 'registration'>,
): string | null {
  const text = (aircraft.callsign ?? '').trim() || (aircraft.registration ?? '').trim();
  return text.length > 0 ? text.slice(0, 16) : null;
}

/** At most this many unselected aircraft carry a callsign label. */
export const LABEL_BUDGET = 100;
/** Unselected labels only appear when the camera is below this height. */
export const LABEL_MAX_HEIGHT_M = 1_000_000;

export interface LabelCandidate {
  readonly id: string;
  /** Squared distance from the camera; nearer wins. */
  readonly distanceSq: number;
}

/**
 * Which entities carry a label: the selected one always; otherwise the nearest `budget`
 * candidates, and none at all above `LABEL_MAX_HEIGHT_M`.
 */
export function chooseLabelled(
  candidates: readonly LabelCandidate[],
  selectedId: string | null,
  cameraHeightM: number,
  budget = LABEL_BUDGET,
): Set<string> {
  const chosen = new Set<string>();
  if (selectedId !== null && candidates.some((c) => c.id === selectedId)) chosen.add(selectedId);
  if (!(cameraHeightM <= LABEL_MAX_HEIGHT_M) || budget <= 0) return chosen;
  const nearest = candidates
    .filter((c) => c.id !== selectedId && Number.isFinite(c.distanceSq))
    .sort((a, b) => a.distanceSq - b.distanceSq);
  for (const candidate of nearest.slice(0, budget)) chosen.add(candidate.id);
  return chosen;
}
