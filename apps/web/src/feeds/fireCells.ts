/**
 * The one fire cell validity rule and the stable id of a cell (NASA FIRMS), shared by the globe
 * renderer (which cells it draws), the inspector (which cells it describes) and snapshot lookups
 * (which cell a selection finds). Cells are untrusted wire data: a cell that fails the rule is
 * skipped everywhere rather than guessed at.
 */
import type { FireCell } from '../contracts';

const isNumberIn = (value: unknown, min: number, max: number): value is number =>
  typeof value === 'number' && Number.isFinite(value) && value >= min && value <= max;

/**
 * Whether a fire cell from the wire is usable: latitude within ±90°, longitude within ±180°, a
 * whole detection count of at least 1, and finite, non-negative FRP and peak FRP. Anything
 * else (null, strings, NaN, a missing field, not an object) is rejected.
 */
export function isValidFireCell(value: unknown): value is FireCell {
  if (typeof value !== 'object' || value === null) return false;
  const cell = value as Partial<Record<keyof FireCell, unknown>>;
  return (
    isNumberIn(cell.lat, -90, 90) &&
    isNumberIn(cell.lon, -180, 180) &&
    isNumberIn(cell.count, 1, Number.MAX_SAFE_INTEGER) &&
    Number.isInteger(cell.count) &&
    isNumberIn(cell.frp, 0, Number.MAX_VALUE) &&
    isNumberIn(cell.maxFrp, 0, Number.MAX_VALUE)
  );
}

/** The stable id of a (valid) fire cell: lat,lon rounded to 4 decimals. */
export function fireCellId(cell: { readonly lat: number; readonly lon: number }): string {
  return `${cell.lat.toFixed(4)},${cell.lon.toFixed(4)}`;
}
