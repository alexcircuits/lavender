/**
 * Coordinate recognition for search: decimal degrees, degrees-minutes-seconds with
 * hemispheres, and MGRS. A port of crates/lavender-geo/src/coords.rs. Parsing never touches
 * the network, so coordinates resolve instantly even when no geocoder is configured.
 */
import type { GeoPoint } from '../contracts';
import { fromMgrs } from './mgrs';

export type CoordinateFormat = 'decimal' | 'dms' | 'mgrs';

export interface ParsedCoordinate {
  readonly position: GeoPoint;
  readonly format: CoordinateFormat;
}

type Token =
  | { readonly kind: 'number'; readonly value: number }
  | { readonly kind: 'hemisphere'; readonly value: 'N' | 'S' | 'E' | 'W' };

const MAX_INPUT_LENGTH = 64;
const SEPARATORS = new Set([',', ';', ':', '°', 'º', "'", '"', '′', '″', '’', '”']);
const HEMISPHERES = new Set(['N', 'S', 'E', 'W']);

const isDigit = (c: string) => c >= '0' && c <= '9';

function tokenize(input: string): Token[] | null {
  const tokens: Token[] = [];
  const chars = [...input];
  let i = 0;
  while (i < chars.length) {
    const c = chars[i] ?? '';
    if (isDigit(c) || c === '.' || c === '-' || c === '+') {
      let text = c;
      i += 1;
      while (i < chars.length && (isDigit(chars[i] ?? '') || chars[i] === '.')) {
        text += chars[i];
        i += 1;
      }
      const value = Number(text);
      if (!Number.isFinite(value) || text === '-' || text === '+' || text === '.') return null;
      tokens.push({ kind: 'number', value });
    } else if (HEMISPHERES.has(c.toUpperCase())) {
      tokens.push({ kind: 'hemisphere', value: c.toUpperCase() as 'N' | 'S' | 'E' | 'W' });
      i += 1;
    } else if (/\s/.test(c) || SEPARATORS.has(c)) {
      i += 1;
    } else {
      return null;
    }
  }
  return tokens;
}

const isNegative = (v: number) => v < 0 || Object.is(v, -0);

/**
 * Degrees from 1–3 numbers (degrees, minutes, seconds). Minutes and seconds must be
 * non-negative and below 60, and only the last component may carry a fraction.
 */
function dmsValue(parts: readonly number[]): number | null {
  const [degrees, ...rest] = parts;
  if (degrees === undefined || rest.length > 2) return null;
  if (parts.slice(0, -1).some((v) => !Number.isInteger(v))) return null;
  let value = Math.abs(degrees);
  for (const [i, v] of rest.entries()) {
    if (!(v >= 0 && v < 60)) return null;
    value += v / (i === 0 ? 60 : 3600);
  }
  return isNegative(degrees) ? -value : value;
}

function validPoint(lat: number, lon: number): GeoPoint | null {
  if (!Number.isFinite(lat) || !Number.isFinite(lon)) return null;
  if (Math.abs(lat) > 90 || Math.abs(lon) > 180) return null;
  return { lat, lon };
}

const numbersIn = (tokens: readonly Token[]) =>
  tokens.flatMap((t) => (t.kind === 'number' ? [t.value] : []));

function fromHemispheres(tokens: readonly Token[]): ParsedCoordinate | null {
  const letters = tokens.flatMap((t, i) => (t.kind === 'hemisphere' ? [{ i, h: t.value }] : []));
  const [first, second] = letters;
  if (letters.length !== 2 || !first || !second) return null;
  const prefix = first.i === 0;
  if (!prefix && second.i !== tokens.length - 1) return null;
  const g1 = prefix ? numbersIn(tokens.slice(1, second.i)) : numbersIn(tokens.slice(0, first.i));
  const g2 = prefix
    ? numbersIn(tokens.slice(second.i + 1))
    : numbersIn(tokens.slice(first.i + 1, second.i));
  if (g1.length === 0 || g2.length === 0 || [...g1, ...g2].some(isNegative)) return null;
  const format: CoordinateFormat = g1.length > 1 || g2.length > 1 ? 'dms' : 'decimal';
  const v1 = dmsValue(g1);
  const v2 = dmsValue(g2);
  if (v1 === null || v2 === null) return null;
  const signed = (v: number, h: string) => (h === 'S' || h === 'W' ? -v : v);
  const latFirst = (first.h === 'N' || first.h === 'S') && (second.h === 'E' || second.h === 'W');
  const lonFirst = (first.h === 'E' || first.h === 'W') && (second.h === 'N' || second.h === 'S');
  let position: GeoPoint | null = null;
  if (latFirst) position = validPoint(signed(v1, first.h), signed(v2, second.h));
  else if (lonFirst) position = validPoint(signed(v2, second.h), signed(v1, first.h));
  return position ? { position, format } : null;
}

/**
 * Recognises coordinates in `input`: `60.17, 24.94`, `60.17N 24.94E`, `60°10'12"N 24°56'18"E`,
 * `N 60 10 12 E 24 56 18`, `18S UJ 23390 07393`. Unmarked decimal pairs are read as latitude
 * then longitude, the convention of every mainstream map product. Null when it is not one.
 */
export function parseCoordinates(input: string): ParsedCoordinate | null {
  const trimmed = input.trim();
  if (trimmed === '' || trimmed.length > MAX_INPUT_LENGTH) return null;
  const compact = trimmed.replace(/\s+/g, '');
  if (isDigit(compact[0] ?? '') && /[a-z]/i.test(compact)) {
    const point = fromMgrs(compact);
    if (point) return { position: point, format: 'mgrs' };
  }
  const tokens = tokenize(trimmed);
  if (!tokens) return null;
  if (tokens.some((t) => t.kind === 'hemisphere')) return fromHemispheres(tokens);
  const numbers = numbersIn(tokens);
  if (numbers.length === 2) {
    const position = validPoint(numbers[0] ?? Number.NaN, numbers[1] ?? Number.NaN);
    return position ? { position, format: 'decimal' } : null;
  }
  if (numbers.length === 4 || numbers.length === 6) {
    const half = numbers.length / 2;
    const lat = dmsValue(numbers.slice(0, half));
    const lon = dmsValue(numbers.slice(half));
    if (lat === null || lon === null) return null;
    const position = validPoint(lat, lon);
    return position ? { position, format: 'dms' } : null;
  }
  return null;
}

/** `60.1699° N, 24.9384° E` */
export function formatDecimal(point: GeoPoint): string {
  const ns = point.lat < 0 ? 'S' : 'N';
  const ew = point.lon < 0 ? 'W' : 'E';
  return `${Math.abs(point.lat).toFixed(4)}° ${ns}, ${Math.abs(point.lon).toFixed(4)}° ${ew}`;
}

/** `60°10′12″ N 24°56′18″ E` */
export function formatDms(point: GeoPoint): string {
  const part = (v: number) => {
    const total = Math.round(Math.abs(v) * 3600);
    const d = Math.floor(total / 3600);
    const m = Math.floor((total - d * 3600) / 60);
    const s = total - d * 3600 - m * 60;
    return `${d}°${String(m).padStart(2, '0')}′${String(s).padStart(2, '0')}″`;
  };
  const ns = point.lat < 0 ? 'S' : 'N';
  const ew = point.lon < 0 ? 'W' : 'E';
  return `${part(point.lat)} ${ns} ${part(point.lon)} ${ew}`;
}
