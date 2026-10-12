/**
 * Latitude/longitude ⇄ UTM ⇄ MGRS on the WGS84 ellipsoid. A port of
 * crates/lavender-geo/src/mgrs.rs: the same zones (Norway and Svalbard exceptions), latitude
 * bands and set-based 100 km square letters, so the browser and the core always agree.
 *
 * The transverse Mercator projection uses Krüger's series in the third flattening n, to third
 * order (the formulation on Wikipedia's UTM page). Truncation error is O(n⁴): well under a
 * millimetre inside a zone. Polar regions (UPS) are not supported: MGRS bands cover −80° to 84°.
 */
import type { GeoPoint } from '../contracts';

const A = 6_378_137;
const F = 1 / 298.257_223_563;
const K0 = 0.9996;
const FALSE_EASTING = 500_000;
const FALSE_NORTHING_SOUTH = 10_000_000;
const SQUARE_M = 100_000;
const MIN_LAT = -80;
const MAX_LAT = 84;

const BANDS = 'CDEFGHJKLMNPQRSTUVWX';
const ROW_LETTERS = 'ABCDEFGHJKLMNPQRSTUV';
const COLUMN_SETS = ['ABCDEFGH', 'JKLMNPQR', 'STUVWXYZ'] as const;

// Krüger series coefficients (third order in n).
const N = F / (2 - F);
const N2 = N * N;
const N3 = N2 * N;
/** Rectifying radius: the meridian arc length per radian. */
const RECT = (A / (1 + N)) * (1 + N2 / 4 + (N2 * N2) / 64);
const ALPHA = [
  N / 2 - (2 * N2) / 3 + (5 * N3) / 16,
  (13 * N2) / 48 - (3 * N3) / 5,
  (61 * N3) / 240,
];
const BETA = [N / 2 - (2 * N2) / 3 + (37 * N3) / 96, N2 / 48 + N3 / 15, (17 * N3) / 480];
const DELTA = [2 * N - (2 * N2) / 3 - 2 * N3, (7 * N2) / 3 - (8 * N3) / 5, (56 * N3) / 15];
/** First eccentricity, 2√n / (1 + n). */
const E = (2 * Math.sqrt(N)) / (1 + N);

const rad = (deg: number) => (deg * Math.PI) / 180;
const deg = (radians: number) => (radians * 180) / Math.PI;

/** A UTM coordinate in metres. */
export interface Utm {
  readonly zone: number;
  readonly northern: boolean;
  readonly easting: number;
  readonly northing: number;
}

function isValidPosition(lat: number, lon: number): boolean {
  return (
    Number.isFinite(lat) && Number.isFinite(lon) && Math.abs(lat) <= 90 && Math.abs(lon) <= 180
  );
}

/** The UTM zone (1–60) for a position, including the Norway and Svalbard exceptions. */
export function utmZone(lat: number, lon: number): number {
  let zone = Math.min(60, Math.floor((lon + 180) / 6) + 1);
  if (lat >= 56 && lat < 64 && lon >= 3 && lon < 12) zone = 32;
  if (lat >= 72 && lat <= 84) {
    if (lon >= 0 && lon < 9) zone = 31;
    else if (lon >= 9 && lon < 21) zone = 33;
    else if (lon >= 21 && lon < 33) zone = 35;
    else if (lon >= 33 && lon < 42) zone = 37;
  }
  return zone;
}

const centralMeridian = (zone: number) => zone * 6 - 183;

/** Projects a position into a given UTM zone. */
export function toUtmInZone(lat: number, lon: number, zone: number): Utm {
  const phi = rad(lat);
  const dLambda = rad(lon - centralMeridian(zone));
  const sinPhi = Math.sin(phi);
  // Conformal latitude, as tan.
  const t = Math.sinh(Math.atanh(sinPhi) - E * Math.atanh(E * sinPhi));
  const xiP = Math.atan2(t, Math.cos(dLambda));
  const etaP = Math.atanh(Math.sin(dLambda) / Math.sqrt(1 + t * t));
  let eta = etaP;
  let xi = xiP;
  ALPHA.forEach((alpha, i) => {
    const j2 = 2 * (i + 1);
    eta += alpha * Math.cos(j2 * xiP) * Math.sinh(j2 * etaP);
    xi += alpha * Math.sin(j2 * xiP) * Math.cosh(j2 * etaP);
  });
  const northern = lat >= 0;
  const northing = K0 * RECT * xi + (northern ? 0 : FALSE_NORTHING_SOUTH);
  return { zone, northern, easting: FALSE_EASTING + K0 * RECT * eta, northing };
}

/** Projects a position into its own UTM zone, or null for an invalid position. */
export function toUtm(lat: number, lon: number): Utm | null {
  if (!isValidPosition(lat, lon)) return null;
  return toUtmInZone(lat, lon, utmZone(lat, lon));
}

/** Converts a UTM coordinate back to a position, or null when the result is not valid. */
export function fromUtm(utm: Utm): GeoPoint | null {
  const xi = (utm.northern ? utm.northing : utm.northing - FALSE_NORTHING_SOUTH) / (K0 * RECT);
  const eta = (utm.easting - FALSE_EASTING) / (K0 * RECT);
  let xiP = xi;
  let etaP = eta;
  BETA.forEach((beta, i) => {
    const j2 = 2 * (i + 1);
    xiP -= beta * Math.sin(j2 * xi) * Math.cosh(j2 * eta);
    etaP -= beta * Math.cos(j2 * xi) * Math.sinh(j2 * eta);
  });
  const chi = Math.asin(Math.sin(xiP) / Math.cosh(etaP));
  let phi = chi;
  DELTA.forEach((delta, i) => {
    phi += delta * Math.sin(2 * (i + 1) * chi);
  });
  const lat = deg(phi);
  let lon = centralMeridian(utm.zone) + deg(Math.atan2(Math.sinh(etaP), Math.cos(xiP)));
  if (lon > 180) lon -= 360;
  if (lon < -180) lon += 360;
  return isValidPosition(lat, lon) ? { lat, lon } : null;
}

function bandIndex(lat: number): number {
  // X (index 19) is 12° tall, so clamp.
  return Math.min(19, Math.floor((lat - MIN_LAT) / 8));
}

const columnSet = (zone: number) => COLUMN_SETS[(zone - 1) % 3] ?? COLUMN_SETS[0];
const rowOffset = (zone: number) => (zone % 2 === 0 ? 5 : 0);

/**
 * Formats a position as MGRS with `precision` digits per axis (0 = the bare 100 km square,
 * 1 = 10 km … 5 = 1 m), without spaces: `18SUJ2339007393`. Null outside the MGRS latitude
 * bands (−80° to 84°). A precision that is not a number means 1 m.
 */
export function toMgrs(lat: number, lon: number, precision = 5): string | null {
  if (!isValidPosition(lat, lon) || lat < MIN_LAT || lat > MAX_LAT) return null;
  const digits = Number.isFinite(precision) ? Math.min(5, Math.max(0, Math.round(precision))) : 5;
  const utm = toUtmInZone(lat, lon, utmZone(lat, lon));
  // Easting is 100 000…900 000 inside a zone, so the 100 km column is 1…8.
  const col = Math.min(8, Math.max(1, Math.floor(utm.easting / SQUARE_M)));
  const row = Math.floor(utm.northing / SQUARE_M);
  const colLetter = columnSet(utm.zone)[col - 1];
  const rowLetter = ROW_LETTERS[(row + rowOffset(utm.zone)) % 20];
  const square = `${utm.zone}${BANDS[bandIndex(lat)]}${colLetter}${rowLetter}`;
  // A bare square: "0" per axis would read back as a 10 km square in its corner.
  if (digits === 0) return square;
  const divisor = 10 ** (5 - digits);
  const e = Math.floor((utm.easting % SQUARE_M) / divisor);
  const n = Math.floor((utm.northing % SQUARE_M) / divisor);
  const pad = (v: number) => String(v).padStart(digits, '0');
  return `${square}${pad(e)}${pad(n)}`;
}

/** `18SUJ2339007393` → `18SUJ 23390 07393`, the grouping people read and say aloud. */
export function spaceMgrs(mgrs: string): string {
  const match = /^(\d{1,2}[A-Z]{3})(\d*)$/.exec(mgrs);
  if (!match) return mgrs;
  const [, square = '', digits = ''] = match;
  if (digits === '') return square;
  const half = digits.length / 2;
  return `${square} ${digits.slice(0, half)} ${digits.slice(half)}`;
}

const MGRS_PATTERN = /^(\d{1,2})([A-Z])([A-Z])([A-Z])(\d*)$/;
/** Absorbs floating-point error in the projection round trip (as in mgrs.rs). */
const BAND_EPSILON_DEG = 1e-6;

/**
 * The latitude extent of a UTM square. Lines of constant northing curve, so besides the
 * corners the extremes can lie where the north and south edges are closest to the central
 * meridian. Null when part of the square is not a valid position.
 */
function squareLatitudes(
  zone: number,
  northern: boolean,
  easting: number,
  northing: number,
  size: number,
): { south: number; north: number } | null {
  const nearMeridian = Math.min(easting + size, Math.max(easting, FALSE_EASTING));
  const lats: number[] = [];
  for (const e of [easting, easting + size, nearMeridian]) {
    for (const n of [northing, northing + size]) {
      const point = fromUtm({ zone, northern, easting: e, northing: n });
      if (!point) return null;
      lats.push(point.lat);
    }
  }
  return { south: Math.min(...lats), north: Math.max(...lats) };
}

/**
 * Parses an MGRS reference (spaces allowed, any case) and returns the centre of the referenced
 * square, or null when it is malformed: unknown letters, an odd digit count, a column letter
 * that zone does not use, or a square outside its stated latitude band.
 *
 * Matches mgrs.rs exactly: a coarse square (a bare 100 km square such as `18SUJ`) may straddle
 * a band edge and is accepted when any part of it lies in the band; a 1 m square effectively
 * has to match the band.
 */
export function fromMgrs(input: string): GeoPoint | null {
  const compact = input.replace(/\s+/g, '').toUpperCase();
  const match = MGRS_PATTERN.exec(compact);
  if (!match) return null;
  const [, zoneText = '', band = '', colLetter = '', rowLetter = '', digits = ''] = match;
  const zone = Number(zoneText);
  if (zone < 1 || zone > 60) return null;
  const bandIdx = BANDS.indexOf(band);
  const col = columnSet(zone).indexOf(colLetter);
  const rowPos = ROW_LETTERS.indexOf(rowLetter);
  if (bandIdx < 0 || col < 0 || rowPos < 0) return null;
  if (digits.length % 2 !== 0 || digits.length > 10) return null;

  const half = digits.length / 2;
  const precisionM = 10 ** (5 - half);
  const row = (rowPos + 20 - rowOffset(zone)) % 20;
  const easting = (col + 1) * SQUARE_M + Number(digits.slice(0, half) || '0') * precisionM;
  let northing = row * SQUARE_M + Number(digits.slice(half) || '0') * precisionM;

  const northern = bandIdx >= 10;
  const bandSouth = MIN_LAT + 8 * bandIdx;
  const bandNorth = bandIdx === 19 ? MAX_LAT : bandSouth + 8;
  const minNorthing = toUtmInZone(bandSouth, centralMeridian(zone), zone).northing;
  // Row letters repeat every 2 000 km; bring the northing up to the band's southern edge.
  while (northing < minNorthing - SQUARE_M) northing += 2_000_000;

  const extent = squareLatitudes(zone, northern, easting, northing, precisionM);
  if (!extent) return null;
  if (extent.north < bandSouth - BAND_EPSILON_DEG || extent.south > bandNorth + BAND_EPSILON_DEG) {
    return null;
  }
  const centre = precisionM / 2;
  return fromUtm({ zone, northern, easting: easting + centre, northing: northing + centre });
}
