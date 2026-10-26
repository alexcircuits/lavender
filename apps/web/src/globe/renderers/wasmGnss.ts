/**
 * WebAssembly GNSS Navigation Integrity & Interference Cell-Binning Micro-Kernel (ADR 0009).
 *
 * Classifies aircraft DO-260A/B navigation accuracy indicators (NIC, NACp, gpsLost) per
 * 14 CFR 91.227(c) minima, applies the gpsjam.org 1-aircraft false-positive discount, and
 * bins observations onto an equirectangular cell grid using the freestanding Zig WASM kernel
 * (`lavender_gnss_bin_cells`) with a matching pure-TypeScript fallback.
 */
import type { GnssCell, GnssLevel } from '../../contracts';
import {
  getActiveLavenderWasm,
  type LavenderWasmContext,
  loadLavenderWasm,
} from './wasmPropagation';

export type { GnssCell, GnssLevel, GnssSnapshot } from '../../contracts';
export { loadLavenderWasm };

/** Input aircraft observation for GNSS cell binning. */
export interface GnssObservation {
  readonly lat: number;
  readonly lon: number;
  readonly nic?: number | null | undefined;
  readonly nacP?: number | null | undefined;
  readonly gpsLost?: boolean | undefined;
  readonly degraded?: boolean | undefined;
}

/** Options controlling GNSS spatial cell binning. */
export interface GnssBinOptions {
  readonly cellDeg?: number | undefined;
  readonly minAircraft?: number | undefined;
}

interface GnssWasmExports {
  readonly lavender_gnss_classify_observation?: (
    nic: number,
    nac_p: number,
    gps_lost: boolean,
  ) => boolean;
  readonly lavender_gnss_discount_percent?: (aircraft: number, degraded: number) => number;
  readonly lavender_gnss_classify_level?: (percent_degraded: number) => number;
  readonly lavender_gnss_bin_cells?: (
    lons_ptr: number,
    lats_ptr: number,
    degraded_flags_ptr: number,
    num_obs: number,
    cell_deg: number,
    min_aircraft: number,
    cell_aircraft_ptr: number,
    cell_degraded_ptr: number,
    scratch_len: number,
    out_row_ptr: number,
    out_col_ptr: number,
    out_aircraft_ptr: number,
    out_degraded_ptr: number,
    out_percent_ptr: number,
    out_level_ptr: number,
    max_out_cells: number,
  ) => number;
}

/** DO-260A/B Navigation Integrity Category threshold (containment radius >= 0.2 NM below 7). */
export const GNSS_NIC_THRESHOLD = 7;
/** DO-260A/B Navigation Accuracy Category for Position threshold (95% accuracy >= 93 m below 8). */
export const GNSS_NACP_THRESHOLD = 8;
/** Default spatial grid resolution in degrees (0.5° × 0.5°). */
export const GNSS_DEFAULT_CELL_DEG = 0.5;
/** Default minimum distinct aircraft required in a cell. */
export const GNSS_DEFAULT_MIN_AIRCRAFT = 3;
/** Medium anomaly threshold in discounted percent. */
export const GNSS_MEDIUM_PERCENT_THRESHOLD = 2;
/** High anomaly threshold in discounted percent. */
export const GNSS_HIGH_PERCENT_THRESHOLD = 10;

export const GNSS_NOMINAL_STATE = 'nominal';
export const GNSS_ANOMALY_STATE = 'navigation-integrity-anomaly';
export const GNSS_INTERPRETATION = 'suspected-gnss-interference';

/** Scratch memory offset in WASM linear memory (16 MB). */
const GNSS_SCRATCH_OFFSET = 16 * 1024 * 1024;
const MAX_GNSS_OBSERVATIONS = 200_000;

function ensureMemory(wasm: LavenderWasmContext, requiredBytes: number): void {
  if (requiredBytes > wasm.memory.buffer.byteLength) {
    const bytesNeeded = requiredBytes - wasm.memory.buffer.byteLength;
    const pagesNeeded = Math.ceil(bytesNeeded / 65536);
    wasm.memory.grow(pagesNeeded);
  }
}

function align8(offset: number): number {
  return (offset + 7) & ~7;
}

/**
 * Pure-TypeScript DO-260A/B navigation integrity classifier.
 * Returns `true` if `gpsLost` is true, or `0 <= nic < 7`, or `0 <= nacP < 8`.
 */
export function classifyGnssObservationTs(
  nic: number | null | undefined,
  nacP: number | null | undefined,
  gpsLost: boolean,
): boolean {
  if (gpsLost) return true;
  if (nic != null && Number.isFinite(nic) && nic >= 0 && nic < GNSS_NIC_THRESHOLD) {
    return true;
  }
  if (nacP != null && Number.isFinite(nacP) && nacP >= 0 && nacP < GNSS_NACP_THRESHOLD) {
    return true;
  }
  return false;
}

/**
 * Classifies an aircraft observation using the Zig WASM kernel when loaded,
 * falling back to `classifyGnssObservationTs`.
 */
export function classifyGnssObservation(
  nic: number | null | undefined,
  nacP: number | null | undefined,
  gpsLost: boolean,
): boolean {
  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as GnssWasmExports | undefined)
    ?.lavender_gnss_classify_observation;
  if (fn) {
    const nicInt = nic != null && Number.isFinite(nic) ? Math.trunc(nic) : -1;
    const nacInt = nacP != null && Number.isFinite(nacP) ? Math.trunc(nacP) : -1;
    return Boolean(fn(nicInt, nacInt, gpsLost));
  }
  return classifyGnssObservationTs(nic, nacP, gpsLost);
}

/**
 * Pure-TypeScript gpsjam.org 1-aircraft false-positive discount:
 * `aircraft <= 0 || degraded <= 1 ? 0 : (100 * (degraded - 1)) / aircraft`.
 */
export function discountGnssDegradedPercentTs(aircraft: number, degraded: number): number {
  if (!Number.isFinite(aircraft) || !Number.isFinite(degraded)) return 0;
  if (aircraft <= 0 || degraded <= 1) return 0;
  return (100 * (degraded - 1)) / aircraft;
}

/**
 * Computes the discounted degraded percentage using WASM when available,
 * falling back to `discountGnssDegradedPercentTs`.
 */
export function discountGnssDegradedPercent(aircraft: number, degraded: number): number {
  if (!Number.isFinite(aircraft) || !Number.isFinite(degraded) || aircraft <= 0 || degraded <= 1) {
    return 0;
  }
  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as GnssWasmExports | undefined)
    ?.lavender_gnss_discount_percent;
  if (fn) {
    return fn(Math.trunc(aircraft), Math.trunc(degraded));
  }
  return discountGnssDegradedPercentTs(aircraft, degraded);
}

/**
 * Pure-TypeScript GNSS anomaly level classification:
 * `< 2` -> `'low'`, `<= 10` -> `'medium'`, `> 10` -> `'high'`.
 */
export function classifyGnssLevelTs(percentDegraded: number): GnssLevel {
  if (!Number.isFinite(percentDegraded) || percentDegraded < GNSS_MEDIUM_PERCENT_THRESHOLD) {
    return 'low';
  }
  if (percentDegraded <= GNSS_HIGH_PERCENT_THRESHOLD) {
    return 'medium';
  }
  return 'high';
}

function levelCodeToGnssLevel(code: number): GnssLevel {
  if (code === 2) return 'high';
  if (code === 1) return 'medium';
  return 'low';
}

/**
 * Classifies a discounted degraded percentage into `'low' | 'medium' | 'high'`,
 * delegating to the Zig WASM kernel when loaded.
 */
export function classifyGnssLevel(percentDegraded: number): GnssLevel {
  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as GnssWasmExports | undefined)
    ?.lavender_gnss_classify_level;
  if (fn) {
    return levelCodeToGnssLevel(fn(percentDegraded));
  }
  return classifyGnssLevelTs(percentDegraded);
}

function buildGnssCell(
  row: number,
  col: number,
  cellDeg: number,
  aircraft: number,
  degraded: number,
  rawPercent: number,
  level: GnssLevel,
): GnssCell {
  const south = row * cellDeg - 90;
  const west = col * cellDeg - 180;
  const north = Math.min(90, south + cellDeg);
  const east = Math.min(180, west + cellDeg);
  const percentDegraded = Math.round(rawPercent * 10) / 10;
  const isAnomaly = level !== 'low';
  return {
    id: `${row}:${col}`,
    bbox: { west, south, east, north },
    center: { lat: south + cellDeg / 2, lon: west + cellDeg / 2 },
    aircraft,
    degraded,
    percentDegraded,
    level,
    state: isAnomaly ? GNSS_ANOMALY_STATE : GNSS_NOMINAL_STATE,
    ...(isAnomaly ? { interpretation: GNSS_INTERPRETATION } : {}),
  };
}

/**
 * Pure-TypeScript reference implementation for binning GNSS aircraft observations
 * into equirectangular spatial cells.
 */
export function binGnssObservationsTs(
  observations: readonly GnssObservation[],
  options: GnssBinOptions = {},
): GnssCell[] {
  const cellDeg = options.cellDeg ?? GNSS_DEFAULT_CELL_DEG;
  const minAircraft = Math.max(1, Math.trunc(options.minAircraft ?? GNSS_DEFAULT_MIN_AIRCRAFT));
  if (!Number.isFinite(cellDeg) || cellDeg <= 0 || cellDeg > 180 || observations.length === 0) {
    return [];
  }

  const rows = Math.ceil(180 / cellDeg);
  const cols = Math.ceil(360 / cellDeg);
  const numObs = Math.min(observations.length, MAX_GNSS_OBSERVATIONS);
  const tallies = new Map<
    number,
    { row: number; col: number; aircraft: number; degraded: number }
  >();

  for (let i = 0; i < numObs; i++) {
    const obs = observations[i];
    if (!obs) continue;
    const { lon, lat } = obs;
    if (
      !Number.isFinite(lon) ||
      !Number.isFinite(lat) ||
      lon < -180 ||
      lon > 180 ||
      lat < -90 ||
      lat > 90
    ) {
      continue;
    }

    const clampedLat = Math.min(lat, 89.999999);
    const clampedLon = Math.min(lon, 179.999999);
    const row = Math.min(Math.max(0, Math.floor((clampedLat + 90) / cellDeg)), rows - 1);
    const col = Math.min(Math.max(0, Math.floor((clampedLon + 180) / cellDeg)), cols - 1);
    const cellIdx = row * cols + col;

    const isDegraded =
      typeof obs.degraded === 'boolean'
        ? obs.degraded
        : classifyGnssObservationTs(obs.nic, obs.nacP, Boolean(obs.gpsLost));

    const existing = tallies.get(cellIdx);
    if (existing) {
      existing.aircraft += 1;
      if (isDegraded) existing.degraded += 1;
    } else {
      tallies.set(cellIdx, {
        row,
        col,
        aircraft: 1,
        degraded: isDegraded ? 1 : 0,
      });
    }
  }

  const sortedIndices = [...tallies.keys()].sort((a, b) => a - b);
  const cells: GnssCell[] = [];
  for (const idx of sortedIndices) {
    const t = tallies.get(idx);
    if (!t || t.aircraft < minAircraft) continue;
    const rawPercent = discountGnssDegradedPercentTs(t.aircraft, t.degraded);
    const level = classifyGnssLevelTs(rawPercent);
    cells.push(buildGnssCell(t.row, t.col, cellDeg, t.aircraft, t.degraded, rawPercent, level));
  }

  return cells;
}

/**
 * Bins aircraft GNSS observations onto an equirectangular grid, delegating to the
 * freestanding Zig WebAssembly export `lavender_gnss_bin_cells` when loaded and
 * falling back to `binGnssObservationsTs` otherwise.
 */
export function binGnssObservations(
  observations: readonly GnssObservation[],
  options: GnssBinOptions = {},
): GnssCell[] {
  const cellDeg = options.cellDeg ?? GNSS_DEFAULT_CELL_DEG;
  const minAircraft = Math.max(1, Math.trunc(options.minAircraft ?? GNSS_DEFAULT_MIN_AIRCRAFT));
  if (!Number.isFinite(cellDeg) || cellDeg <= 0 || cellDeg > 180 || observations.length === 0) {
    return [];
  }

  const wasm = getActiveLavenderWasm();
  const binFn = (wasm?.exports as unknown as GnssWasmExports | undefined)?.lavender_gnss_bin_cells;
  if (!wasm || !binFn) {
    return binGnssObservationsTs(observations, options);
  }

  const rows = Math.ceil(180 / cellDeg);
  const cols = Math.ceil(360 / cellDeg);
  const scratchLen = rows * cols;
  if (scratchLen <= 0 || scratchLen > 4_000_000) {
    return binGnssObservationsTs(observations, options);
  }

  const numObs = Math.min(observations.length, MAX_GNSS_OBSERVATIONS);
  const maxOutCells = Math.min(scratchLen, numObs);

  const basePtr = align8(wasm.heapBase + GNSS_SCRATCH_OFFSET);
  const lonsPtr = basePtr;
  const latsPtr = lonsPtr + numObs * 8;
  const cellAircraftPtr = latsPtr + numObs * 8;
  const cellDegradedPtr = cellAircraftPtr + scratchLen * 4;
  const outAircraftPtr = cellDegradedPtr + scratchLen * 4;
  const outDegradedPtr = outAircraftPtr + maxOutCells * 4;
  const outPercentPtr = outDegradedPtr + maxOutCells * 4;
  const outRowPtr = outPercentPtr + maxOutCells * 4;
  const outColPtr = outRowPtr + maxOutCells * 2;
  const degradedFlagsPtr = outColPtr + maxOutCells * 2;
  const outLevelPtr = degradedFlagsPtr + numObs;
  const totalRequiredBytes = align8(outLevelPtr + maxOutCells);

  ensureMemory(wasm, totalRequiredBytes);

  const buf = wasm.memory.buffer;
  const lonsView = new Float64Array(buf, lonsPtr, numObs);
  const latsView = new Float64Array(buf, latsPtr, numObs);
  const degradedFlagsView = new Uint8Array(buf, degradedFlagsPtr, numObs);

  for (let i = 0; i < numObs; i++) {
    const obs = observations[i];
    if (!obs) {
      lonsView[i] = NaN;
      latsView[i] = NaN;
      degradedFlagsView[i] = 0;
      continue;
    }
    lonsView[i] = obs.lon;
    latsView[i] = obs.lat;
    const isDegraded =
      typeof obs.degraded === 'boolean'
        ? obs.degraded
        : classifyGnssObservationTs(obs.nic, obs.nacP, Boolean(obs.gpsLost));
    degradedFlagsView[i] = isDegraded ? 1 : 0;
  }

  const written = binFn(
    lonsPtr,
    latsPtr,
    degradedFlagsPtr,
    numObs,
    cellDeg,
    minAircraft,
    cellAircraftPtr,
    cellDegradedPtr,
    scratchLen,
    outRowPtr,
    outColPtr,
    outAircraftPtr,
    outDegradedPtr,
    outPercentPtr,
    outLevelPtr,
    maxOutCells,
  );

  const outRow = new Uint16Array(wasm.memory.buffer, outRowPtr, written);
  const outCol = new Uint16Array(wasm.memory.buffer, outColPtr, written);
  const outAircraft = new Uint32Array(wasm.memory.buffer, outAircraftPtr, written);
  const outDegraded = new Uint32Array(wasm.memory.buffer, outDegradedPtr, written);
  const outPercent = new Float32Array(wasm.memory.buffer, outPercentPtr, written);
  const outLevel = new Uint8Array(wasm.memory.buffer, outLevelPtr, written);

  const cells: GnssCell[] = [];
  for (let i = 0; i < written; i++) {
    const row = outRow[i] ?? 0;
    const col = outCol[i] ?? 0;
    const ac = outAircraft[i] ?? 0;
    const deg = outDegraded[i] ?? 0;
    const pct = outPercent[i] ?? 0;
    const lvl = levelCodeToGnssLevel(outLevel[i] ?? 0);
    cells.push(buildGnssCell(row, col, cellDeg, ac, deg, pct, lvl));
  }

  return cells;
}
