/**
 * WebAssembly-accelerated SGP4 orbit propagation kernel.
 *
 * Replaces pure JS SGP4 propagation loops with the freestanding Zig WebAssembly kernel
 * (`lavender_kernels.wasm`). Provides single-satellite ECF propagation and high-throughput
 * batch propagation into flat Float32 coordinate buffers for Cesium.
 */
import { jday, type SatRec } from 'satellite.js';
import type { SatelliteElements } from '../../contracts';
import { fetchWasmResponse } from '../../platform';
import { type EcfMetres, epochMillis } from './propagation';

const DEG_TO_RAD = Math.PI / 180.0;
const TWO_PI = 2.0 * Math.PI;
const MINUTES_PER_DAY = 1440.0;

/** Plausible geocentric distances (km): 6,300 km to 500,000 km in metres squared. */
const MIN_RADIUS_M_SQ = 6_300_000 * 6_300_000;
const MAX_RADIUS_M_SQ = 500_000_000 * 500_000_000;

export interface LavenderWasmExports extends WebAssembly.Exports {
  readonly memory: WebAssembly.Memory;
  readonly lavender_sgp4_propagate_ecf_m: (
    bstar: number,
    inclo_rad: number,
    nodeo_rad: number,
    ecco: number,
    argpo_rad: number,
    mo_rad: number,
    no_kozai_rad_min: number,
    tsince_min: number,
    gmst_rad: number,
    out_ecf_m: number,
  ) => number;
  readonly lavender_sgp4_batch_propagate: (
    elements_ptr: number,
    tsince_array_ptr: number,
    count: number,
    gmst_rad: number,
    out_coords_m: number,
  ) => number;
  readonly lavender_wind_sample: (
    ni: number,
    nj: number,
    lo1: number,
    la1: number,
    di: number,
    dj: number,
    u_ptr: number,
    v_ptr: number,
    lon: number,
    lat: number,
    out_uv: number,
  ) => void;
  readonly lavender_wind_bake_streamline: (
    ni: number,
    nj: number,
    lo1: number,
    la1: number,
    di: number,
    dj: number,
    u_ptr: number,
    v_ptr: number,
    seed_lon: number,
    seed_lat: number,
    steps: number,
    step_seconds: number,
    out_coords: number,
    max_points: number,
  ) => number;
  readonly lavender_arbitrate_labels: (
    labels_ptr: number,
    count: number,
    screen_w: number,
    screen_h: number,
    max_labels: number,
    out_mask_ptr: number,
  ) => number;
  readonly lavender_delta_decode_coords: (
    deltas_ptr: number,
    deltas_len: number,
    scale: number,
    out_coords_ptr: number,
    max_coords: number,
  ) => number;
  readonly lavender_simplify_rdp: (
    coords_ptr: number,
    num_points: number,
    epsilon: number,
    out_coords_ptr: number,
    max_out_points: number,
  ) => number;
  readonly lavender_point_in_polygon: (
    test_lon: number,
    test_lat: number,
    poly_coords_ptr: number,
    num_points: number,
  ) => boolean;
  readonly lavender_heatmap_bin?: (
    points_ptr: number,
    num_points: number,
    width: number,
    height: number,
    lon_min: number,
    lon_max: number,
    lat_min: number,
    lat_max: number,
    out_weight_sum_ptr: number,
    out_count_ptr: number,
  ) => number;
  readonly lavender_cctv_compute_frustum?: (
    lat: number,
    lon: number,
    ground_alt_m: number,
    mount_height_m: number,
    heading_deg: number,
    pitch_deg: number,
    fov_deg: number,
    range_m: number,
    out_coords_ptr: number,
  ) => boolean;
  readonly lavender_cctv_batch_wireframe?: (
    cameras_ptr: number,
    camera_count: number,
    out_wireframe_ptr: number,
    max_cameras: number,
  ) => number;
}

export interface LavenderWasmContext {
  readonly instance: WebAssembly.Instance;
  readonly module: WebAssembly.Module;
  readonly exports: LavenderWasmExports;
  readonly memory: WebAssembly.Memory;
  readonly heapBase: number;
  ensureCapacity: (bytesNeeded: number) => void;
  propagateWasmEcf: (satrec: SatRec, date: Date, gmst: number, out: EcfMetres) => boolean;
  propagateBatchWasm: (
    satellites: readonly (SatRec | SatelliteElements)[],
    date: Date,
    gmst: number,
    outCoords: Float32Array,
  ) => number;
}

let activeWasm: LavenderWasmContext | null = null;
let activeWasmPromise: Promise<LavenderWasmContext> | null = null;

export function getActiveLavenderWasm(): LavenderWasmContext | null {
  return activeWasm;
}

export function isLavenderWasmLoaded(): boolean {
  return activeWasm !== null;
}

/**
 * Creates the WASM context with scratch memory management and propagation methods.
 */
function createContext(
  instance: WebAssembly.Instance,
  module: WebAssembly.Module,
): LavenderWasmContext {
  const exports = instance.exports as unknown as LavenderWasmExports;
  const memory = exports.memory;
  const heapBase = memory.buffer.byteLength;

  const ensureCapacity = (bytesNeeded: number) => {
    const required = heapBase + bytesNeeded;
    if (memory.buffer.byteLength < required) {
      const missing = required - memory.buffer.byteLength;
      const pages = Math.ceil(missing / 65536);
      memory.grow(pages);
    }
  };

  // Grow initial memory by 2 pages (128 KB) for immediate scratch availability
  ensureCapacity(128 * 1024);

  const propagateWasmEcfMethod = (
    satrec: SatRec,
    date: Date,
    gmst: number,
    out: EcfMetres,
  ): boolean => {
    if (satrec.error !== undefined && satrec.error !== 0) return false;

    const bstar = satrec.bstar;
    const inclo = satrec.inclo;
    const nodeo = satrec.nodeo;
    const ecco = satrec.ecco;
    const argpo = satrec.argpo;
    const mo = satrec.mo;
    const nokozai =
      (satrec as { nokozai?: number; no_kozai?: number }).nokozai ??
      (satrec as { nokozai?: number; no_kozai?: number }).no_kozai ??
      satrec.no;

    if (
      !Number.isFinite(bstar) ||
      !Number.isFinite(inclo) ||
      !Number.isFinite(nodeo) ||
      !Number.isFinite(ecco) ||
      !Number.isFinite(argpo) ||
      !Number.isFinite(mo) ||
      !Number.isFinite(nokozai) ||
      nokozai <= 0 ||
      ecco < 0 ||
      ecco >= 0.999
    ) {
      return false;
    }

    const j = jday(date);
    const tsince = (j - satrec.jdsatepoch) * MINUTES_PER_DAY;
    if (!Number.isFinite(tsince)) {
      return false;
    }

    ensureCapacity(24);
    const outPtr = heapBase;
    const ok = exports.lavender_sgp4_propagate_ecf_m(
      bstar,
      inclo,
      nodeo,
      ecco,
      argpo,
      mo,
      nokozai,
      tsince,
      gmst,
      outPtr,
    );
    if (!ok) return false;

    const view = new Float64Array(memory.buffer, outPtr, 3);
    const x = view[0] ?? 0;
    const y = view[1] ?? 0;
    const z = view[2] ?? 0;
    const radiusSq = x * x + y * y + z * z;
    if (radiusSq < MIN_RADIUS_M_SQ || radiusSq > MAX_RADIUS_M_SQ || !Number.isFinite(radiusSq)) {
      return false;
    }

    out.x = x;
    out.y = y;
    out.z = z;
    return true;
  };

  const propagateBatchWasmMethod = (
    satellites: readonly (SatRec | SatelliteElements)[],
    date: Date,
    gmst: number,
    outCoords: Float32Array,
  ): number => {
    const count = satellites.length;
    if (count === 0) return 0;
    if (outCoords.length < count * 3) {
      throw new Error(
        `outCoords buffer too small: expected at least ${count * 3}, got ${outCoords.length}`,
      );
    }

    const elementsBytes = count * 7 * 8;
    const tsinceBytes = count * 8;
    const coordsBytes = count * 3 * 4;
    const totalBytes = elementsBytes + tsinceBytes + coordsBytes;

    ensureCapacity(totalBytes);

    const elementsPtr = heapBase;
    const tsincePtr = elementsPtr + elementsBytes;
    const coordsPtr = tsincePtr + tsinceBytes;

    const elementsView = new Float64Array(memory.buffer, elementsPtr, count * 7);
    const tsinceView = new Float64Array(memory.buffer, tsincePtr, count);

    const julianDate = jday(date);
    const dateMs = date.getTime();

    for (let i = 0; i < count; i++) {
      const sat = satellites[i];
      if (!sat) continue;
      let bstar = 0;
      let inclo = 0;
      let nodeo = 0;
      let ecco = 0;
      let argpo = 0;
      let mo = 0;
      let nokozai = 0;
      let tsince = 0;

      if ('inclo' in sat) {
        bstar = sat.bstar;
        inclo = sat.inclo;
        nodeo = sat.nodeo;
        ecco = sat.ecco;
        argpo = sat.argpo;
        mo = sat.mo;
        nokozai =
          (sat as { nokozai?: number; no_kozai?: number }).nokozai ??
          (sat as { nokozai?: number; no_kozai?: number }).no_kozai ??
          sat.no;
        tsince = (julianDate - sat.jdsatepoch) * MINUTES_PER_DAY;
      } else {
        bstar = sat.bstar;
        inclo = sat.inclinationDeg * DEG_TO_RAD;
        nodeo = sat.raanDeg * DEG_TO_RAD;
        ecco = sat.eccentricity;
        argpo = sat.argOfPericenterDeg * DEG_TO_RAD;
        mo = sat.meanAnomalyDeg * DEG_TO_RAD;
        nokozai = (sat.meanMotion * TWO_PI) / MINUTES_PER_DAY;
        const epochMs = epochMillis(sat.epoch);
        tsince = (dateMs - epochMs) / 60000.0;
      }

      const base = i * 7;
      elementsView[base + 0] = bstar;
      elementsView[base + 1] = inclo;
      elementsView[base + 2] = nodeo;
      elementsView[base + 3] = ecco;
      elementsView[base + 4] = argpo;
      elementsView[base + 5] = mo;
      elementsView[base + 6] = nokozai;
      tsinceView[i] = tsince;
    }

    const successCount = exports.lavender_sgp4_batch_propagate(
      elementsPtr,
      tsincePtr,
      count,
      gmst,
      coordsPtr,
    );

    const coordsView = new Float32Array(memory.buffer, coordsPtr, count * 3);
    outCoords.set(coordsView);
    return successCount;
  };

  return {
    instance,
    module,
    exports,
    memory,
    heapBase,
    ensureCapacity,
    propagateWasmEcf: propagateWasmEcfMethod,
    propagateBatchWasm: propagateBatchWasmMethod,
  };
}

export type WasmSource = string | ArrayBuffer | Response | Promise<Response>;

/**
 * Asynchronously loads and instantiates the Lavender WebAssembly kernel.
 *
 * Uses `WebAssembly.instantiateStreaming` via `fetchWasmResponse('/wasm/lavender_kernels.wasm')`
 * where available, falling back to `ArrayBuffer` in tests / Node environments.
 */
export async function loadLavenderWasm(source?: WasmSource): Promise<LavenderWasmContext> {
  if (activeWasm && !source) {
    return activeWasm;
  }
  if (activeWasmPromise && !source) {
    return activeWasmPromise;
  }

  const loadPromise = (async () => {
    // 1. If explicit ArrayBuffer is passed:
    if (source instanceof ArrayBuffer) {
      const res = await WebAssembly.instantiate(source);
      const ctx = createContext(res.instance, res.module);
      activeWasm = ctx;
      return ctx;
    }

    // 2. If Response or Promise<Response> is passed:
    if (
      source instanceof Response ||
      (source && typeof (source as Promise<unknown>).then === 'function')
    ) {
      const resp = await source;
      if (resp instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
          try {
            const res = await WebAssembly.instantiateStreaming(resp.clone());
            const ctx = createContext(res.instance, res.module);
            activeWasm = ctx;
            return ctx;
          } catch {
            // fall back to arrayBuffer
          }
        }
        const bytes = await resp.arrayBuffer();
        const res = await WebAssembly.instantiate(bytes);
        const ctx = createContext(res.instance, res.module);
        activeWasm = ctx;
        return ctx;
      }
    }

    const wasmUrl = typeof source === 'string' ? source : '/wasm/lavender_kernels.wasm';

    // 3. Try browser fetch / instantiateStreaming via LavenderPlatform helper (ADR 0005)
    // In Node / happy-dom test environments without a running server, skip fetch
    const isBrowserWithoutNode =
      typeof window !== 'undefined' && (typeof process === 'undefined' || !process.versions?.node);

    if (isBrowserWithoutNode) {
      try {
        const response = await fetchWasmResponse(wasmUrl);
        if (response.ok) {
          if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
              const res = await WebAssembly.instantiateStreaming(response.clone());
              const ctx = createContext(res.instance, res.module);
              activeWasm = ctx;
              return ctx;
            } catch {
              // MIME type or streaming issue; fallback to arrayBuffer
            }
          }
          const bytes = await response.arrayBuffer();
          const res = await WebAssembly.instantiate(bytes);
          const ctx = createContext(res.instance, res.module);
          activeWasm = ctx;
          return ctx;
        }
      } catch {
        // Fetch failed; proceed to Node fallback
      }
    }

    // 4. Fallback for Node / Vitest test environments
    if (typeof process !== 'undefined' && process.versions?.node) {
      try {
        const fsMod = 'node:fs/promises';
        const pathMod = 'node:path';
        const fs = await import(/* @vite-ignore */ fsMod);
        const path = await import(/* @vite-ignore */ pathMod);

        const candidates: string[] = [];
        if (typeof source === 'string' && path.isAbsolute(source)) {
          candidates.push(source);
        }

        // Search relative to cwd and ancestor directories
        let dir = process.cwd();
        for (let i = 0; i < 6; i++) {
          candidates.push(
            path.join(dir, 'public/wasm/lavender_kernels.wasm'),
            path.join(dir, 'apps/web/public/wasm/lavender_kernels.wasm'),
            path.join(dir, 'products/lavender/apps/web/public/wasm/lavender_kernels.wasm'),
            path.join(dir, 'kernels/zig/lavender_kernels.wasm'),
            path.join(dir, 'products/lavender/kernels/zig/lavender_kernels.wasm'),
          );
          const parent = path.dirname(dir);
          if (parent === dir) break;
          dir = parent;
        }

        for (const candidate of candidates) {
          try {
            const fileBuf = await fs.readFile(candidate);
            const arrayBuffer = fileBuf.buffer.slice(
              fileBuf.byteOffset,
              fileBuf.byteOffset + fileBuf.byteLength,
            );
            const res = await WebAssembly.instantiate(arrayBuffer);
            const ctx = createContext(res.instance, res.module);
            activeWasm = ctx;
            return ctx;
          } catch {
            // try next candidate
          }
        }
      } catch {
        // Node fallback failed
      }
    }

    throw new Error(
      `Failed to load Lavender WebAssembly kernel from ${wasmUrl}. ` +
        'Ensure /wasm/lavender_kernels.wasm is available in public/wasm or provide an ArrayBuffer.',
    );
  })();

  if (!source) {
    activeWasmPromise = loadPromise;
  }
  return loadPromise;
}

/**
 * Propagates a satellite to Earth-Centered Earth-Fixed (ECF) coordinates in metres
 * using the Lavender WebAssembly kernel.
 */
export function propagateWasmEcf(
  satrec: SatRec,
  date: Date,
  gmst: number,
  out: EcfMetres,
): boolean {
  if (!activeWasm) {
    throw new Error('Lavender WebAssembly module is not loaded. Call loadLavenderWasm() first.');
  }
  return activeWasm.propagateWasmEcf(satrec, date, gmst, out);
}

/**
 * Batch propagates an array of satellites into a flat Float32Array coordinate buffer.
 *
 * Each satellite produces 3 consecutive coordinates [x, y, z] in metres.
 * Returns the number of successfully propagated satellites.
 */
export function propagateBatchWasm(
  satellites: readonly (SatRec | SatelliteElements)[],
  date: Date,
  gmst: number,
  outCoords: Float32Array,
): number {
  if (!activeWasm) {
    throw new Error('Lavender WebAssembly module is not loaded. Call loadLavenderWasm() first.');
  }
  return activeWasm.propagateBatchWasm(satellites, date, gmst, outCoords);
}
