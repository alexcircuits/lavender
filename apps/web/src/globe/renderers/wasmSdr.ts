/**
 * WebAssembly Mode-S ADS-B Extended Squitter (DF17/DF18) Decoder & 2 Msps I/Q Demodulator (ADR 0009).
 *
 * Ports `legacy/src/sdr/adsbDecoder.js` to TypeScript with hardware-friendly WASM acceleration
 * via the freestanding Zig micro-kernel (`lavender_modes_checksum`, `lavender_decode_global_airborne_cpr`,
 * `lavender_adsb_decode_frame`, `lavender_iq_extract_adsb_frames`) and a bit-exact pure-TS fallback.
 */
import {
  getActiveLavenderWasm,
  type LavenderWasmContext,
  loadLavenderWasm,
} from './wasmPropagation';

export { loadLavenderWasm };

export const MODE_S_POLYNOMIAL = 0xfff409;
/** An aircraft with no CRC-valid message for this long is removed. */
export const LOCAL_ADSB_STALE_MS = 60_000;
const CPR_PAIR_MAX_AGE_MS = 10_000;
const CPR_SURFACE_PAIR_MAX_AGE_MS = 25_000;
const CPR_SCALE = 131_072;
const CPR_NZ = 15;
const CALLSIGN_CHARSET = '#ABCDEFGHIJKLMNOPQRSTUVWXYZ#####_###############0123456789######';
export const PREAMBLE_SAMPLES = 16;
export const FRAME_BITS = 112;
export const FRAME_SAMPLES = PREAMBLE_SAMPLES + FRAME_BITS * 2;
const STREAM_CARRY_BYTES = (FRAME_SAMPLES - 1) * 2;

/** Scratch memory offset in WASM linear memory (18 MB). */
const SDR_SCRATCH_OFFSET = 18 * 1024 * 1024;
const MAX_IQ_FRAMES = 4096;

export interface CprPositionPayload {
  readonly odd: boolean;
  readonly surface?: boolean | undefined;
  readonly latitude: number;
  readonly longitude: number;
  readonly receivedAt: number;
}

export interface DecodedAdsbMessage {
  readonly icao: string;
  readonly typeCode: number;
  readonly downlinkFormat: number;
  readonly receivedAt: number;
  readonly callsign?: string | undefined;
  readonly category?: string | undefined;
  readonly altitudeFt?: number | null | undefined;
  readonly onGround?: boolean | undefined;
  readonly speedKt?: number | undefined;
  readonly headingDeg?: number | undefined;
  readonly verticalRateFpm?: number | null | undefined;
  readonly cpr?: CprPositionPayload | undefined;
}

export interface CprFrameInput {
  readonly latitude: number;
  readonly longitude: number;
  readonly odd?: boolean | undefined;
  readonly surface?: boolean | undefined;
  readonly receivedAt?: number | undefined;
}

export interface DecodedGeoPosition {
  readonly latitude: number;
  readonly longitude: number;
}

interface SdrWasmExports {
  readonly lavender_cpr_nl?: (lat: number) => number;
  readonly lavender_modes_checksum?: (frame_ptr: number) => number;
  readonly lavender_decode_global_airborne_cpr?: (
    even_lat: number,
    even_lon: number,
    odd_lat: number,
    odd_lon: number,
    use_odd: boolean,
    out_lat_ptr: number,
    out_lon_ptr: number,
  ) => boolean;
  readonly lavender_adsb_decode_frame?: (
    frame_ptr: number,
    out_icao_ptr: number,
    out_type_code_ptr: number,
    out_msg_type_ptr: number,
    out_callsign_ptr: number,
    out_altitude_ft_ptr: number,
    out_has_altitude_ptr: number,
    out_on_ground_ptr: number,
    out_speed_kt_ptr: number,
    out_heading_deg_ptr: number,
    out_vr_fpm_ptr: number,
    out_has_velocity_ptr: number,
    out_cpr_odd_ptr: number,
    out_cpr_surface_ptr: number,
    out_cpr_lat_ptr: number,
    out_cpr_lon_ptr: number,
    out_has_cpr_ptr: number,
  ) => boolean;
  readonly lavender_iq_extract_adsb_frames?: (
    iq_ptr: number,
    iq_len: number,
    power_scratch_ptr: number,
    power_scratch_len: number,
    out_frames_ptr: number,
    max_frames: number,
  ) => number;
}

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

function bit(bytes: Uint8Array, index: number): number {
  return ((bytes[index >> 3] ?? 0) >> (7 - (index & 7))) & 1;
}

function bits(bytes: Uint8Array, start: number, length: number): number {
  let value = 0;
  for (let i = 0; i < length; i += 1) {
    value = value * 2 + bit(bytes, start + i);
  }
  return value;
}

function positiveModulo(value: number, divisor: number): number {
  return ((value % divisor) + divisor) % divisor;
}

/** Number of CPR longitude zones NL(latitude) per DO-260B. */
export function cprNlTs(latitude: number): number {
  const absolute = Math.abs(latitude);
  if (absolute >= 87) return absolute === 87 ? 2 : 1;
  const numerator = 1 - Math.cos(Math.PI / (2 * CPR_NZ));
  const denominator = Math.cos((Math.PI / 180) * absolute) ** 2;
  return Math.floor((2 * Math.PI) / Math.acos(1 - numerator / denominator));
}

function normalizeLongitude(longitude: number): number {
  let normalized = longitude;
  while (normalized > 180) normalized -= 360;
  while (normalized < -180) normalized += 360;
  return normalized;
}

/**
 * Pure-TypeScript 24-bit Mode-S CRC parity remainder.
 * Valid 112-bit extended squitter frames return `0`.
 */
export function modeSChecksumTs(bytes: Uint8Array): number {
  let remainder = 0;
  const totalBits = bytes.length * 8;
  for (let index = 0; index < totalBits; index += 1) {
    const feedback = bit(bytes, index) ^ ((remainder >> 23) & 1);
    remainder = (remainder << 1) & 0xffffff;
    if (feedback) remainder ^= MODE_S_POLYNOMIAL;
  }
  return remainder >>> 0;
}

/**
 * Computes the 24-bit Mode-S CRC remainder using `lavender_modes_checksum` when
 * a 14-byte frame is passed and the Zig WASM kernel is loaded, falling back to
 * `modeSChecksumTs`.
 */
export function modeSChecksum(bytes: Uint8Array): number {
  if (!(bytes instanceof Uint8Array)) return 0xffffff;
  if (bytes.length === 14) {
    const wasm = getActiveLavenderWasm();
    const fn = (wasm?.exports as unknown as SdrWasmExports | undefined)?.lavender_modes_checksum;
    if (wasm && fn) {
      const ptr = align8(wasm.heapBase + SDR_SCRATCH_OFFSET);
      ensureMemory(wasm, ptr + 16);
      new Uint8Array(wasm.memory.buffer, ptr, 14).set(bytes);
      return fn(ptr) >>> 0;
    }
  }
  return modeSChecksumTs(bytes);
}

/**
 * Ground speed in knots from a surface message's 7-bit movement field (DO-260B §2.2.3.2.4.2).
 */
export function decodeSurfaceMovement(movement: number): number | null {
  if (movement === 0 || movement > 124) return null;
  if (movement === 1) return 0;
  if (movement === 124) return 175;
  const bands: readonly (readonly [number, number, number])[] = [
    [2, 0.125, 0.125],
    [9, 1, 0.25],
    [13, 2, 0.5],
    [39, 15, 1],
    [94, 70, 2],
    [109, 100, 5],
  ];
  let band: readonly [number, number, number] = [2, 0.125, 0.125];
  for (const candidate of bands) {
    if (movement >= candidate[0]) band = candidate;
  }
  const [firstCode, firstKt, stepKt] = band;
  return firstKt + (movement - firstCode) * stepKt;
}

function decodeAltitudeTs(bytes: Uint8Array): number | null {
  if (!bit(bytes, 47)) return null;
  const encoded = (bits(bytes, 40, 7) << 4) | bits(bytes, 48, 4);
  return encoded * 25 - 1_000;
}

function decodeCallsignTs(bytes: Uint8Array): string {
  let callsign = '';
  for (let index = 0; index < 8; index += 1) {
    callsign += CALLSIGN_CHARSET[bits(bytes, 40 + index * 6, 6)] ?? ' ';
  }
  return callsign.replace(/[_#]/g, ' ').trim();
}

function decodeVelocityTs(bytes: Uint8Array): {
  speedKt: number;
  headingDeg: number;
  verticalRateFpm: number | null;
} | null {
  const subtype = bits(bytes, 37, 3);
  if (subtype !== 1 && subtype !== 2) return null;
  const multiplier = subtype === 2 ? 4 : 1;
  const eastWestRaw = bits(bytes, 46, 10);
  const northSouthRaw = bits(bytes, 57, 10);
  if (!eastWestRaw || !northSouthRaw) return null;
  const eastWest = (eastWestRaw - 1) * multiplier * (bit(bytes, 45) ? -1 : 1);
  const northSouth = (northSouthRaw - 1) * multiplier * (bit(bytes, 56) ? -1 : 1);
  const speedKt = Math.hypot(eastWest, northSouth);
  const headingDeg = positiveModulo((Math.atan2(eastWest, northSouth) * 180) / Math.PI, 360);
  const verticalRaw = bits(bytes, 69, 9);
  const verticalRateFpm = verticalRaw ? (verticalRaw - 1) * 64 * (bit(bytes, 68) ? -1 : 1) : null;
  return { speedKt, headingDeg, verticalRateFpm };
}

/**
 * Pure-TypeScript decoder for a 14-byte (112-bit) DF17/DF18 Mode-S Extended Squitter frame.
 */
export function decodeAdsbFrameTs(
  bytes: Uint8Array,
  { receivedAt = Date.now() }: { receivedAt?: number } = {},
): DecodedAdsbMessage | null {
  if (!(bytes instanceof Uint8Array) || bytes.length !== 14) return null;
  const downlinkFormat = bits(bytes, 0, 5);
  if ((downlinkFormat !== 17 && downlinkFormat !== 18) || modeSChecksumTs(bytes) !== 0) {
    return null;
  }
  const icao = Array.from(bytes.subarray(1, 4))
    .map((value) => value.toString(16).padStart(2, '0'))
    .join('')
    .toUpperCase();
  const typeCode = bits(bytes, 32, 5);

  if (typeCode >= 1 && typeCode <= 4) {
    const callsign = decodeCallsignTs(bytes);
    const category = `${String.fromCharCode(69 - typeCode)}${bits(bytes, 37, 3)}`;
    return { icao, typeCode, downlinkFormat, receivedAt, callsign, category };
  }

  if (typeCode >= 5 && typeCode <= 8) {
    const speedKt = decodeSurfaceMovement(bits(bytes, 37, 7));
    const hasHeading = Boolean(bit(bytes, 44));
    const headingDeg = hasHeading ? (bits(bytes, 45, 7) * 360) / 128 : undefined;
    return {
      icao,
      typeCode,
      downlinkFormat,
      receivedAt,
      onGround: true,
      altitudeFt: null,
      ...(speedKt !== null ? { speedKt } : {}),
      ...(headingDeg !== undefined ? { headingDeg } : {}),
      cpr: {
        surface: true,
        odd: Boolean(bit(bytes, 53)),
        latitude: bits(bytes, 54, 17),
        longitude: bits(bytes, 71, 17),
        receivedAt,
      },
    };
  }

  if (typeCode >= 9 && typeCode <= 18) {
    return {
      icao,
      typeCode,
      downlinkFormat,
      receivedAt,
      onGround: false,
      altitudeFt: decodeAltitudeTs(bytes),
      cpr: {
        odd: Boolean(bit(bytes, 53)),
        latitude: bits(bytes, 54, 17),
        longitude: bits(bytes, 71, 17),
        receivedAt,
      },
    };
  }

  if (typeCode === 19) {
    const vel = decodeVelocityTs(bytes);
    return {
      icao,
      typeCode,
      downlinkFormat,
      receivedAt,
      ...(vel
        ? {
            speedKt: vel.speedKt,
            headingDeg: vel.headingDeg,
            verticalRateFpm: vel.verticalRateFpm,
          }
        : {}),
    };
  }

  return { icao, typeCode, downlinkFormat, receivedAt };
}

/**
 * Decodes a 14-byte DF17/DF18 Extended Squitter frame using the Zig WASM export
 * `lavender_adsb_decode_frame` when loaded, falling back to `decodeAdsbFrameTs`.
 */
export function decodeAdsbMessage(
  bytes: Uint8Array,
  { receivedAt = Date.now() }: { receivedAt?: number } = {},
): DecodedAdsbMessage | null {
  if (!(bytes instanceof Uint8Array) || bytes.length !== 14) return null;

  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as SdrWasmExports | undefined)?.lavender_adsb_decode_frame;
  if (!wasm || !fn) {
    return decodeAdsbFrameTs(bytes, { receivedAt });
  }

  const basePtr = align8(wasm.heapBase + SDR_SCRATCH_OFFSET);
  ensureMemory(wasm, basePtr + 96);

  const outSpeedKtPtr = basePtr; // f64 (0..8)
  const outHeadingDegPtr = basePtr + 8; // f64 (8..16)
  const outAltitudeFtPtr = basePtr + 16; // i32 (16..20)
  const outVrFpmPtr = basePtr + 20; // i32 (20..24)
  const outCprLatPtr = basePtr + 24; // u32 (24..28)
  const outCprLonPtr = basePtr + 28; // u32 (28..32)
  const framePtr = basePtr + 32; // [14]u8 (32..46)
  const outIcaoPtr = basePtr + 48; // [6]u8 (48..54)
  const outCallsignPtr = basePtr + 56; // [8]u8 (56..64)
  const outTypeCodePtr = basePtr + 64; // u8
  const outMsgTypePtr = basePtr + 65; // u8
  const outHasAltitudePtr = basePtr + 66; // u8
  const outOnGroundPtr = basePtr + 67; // u8
  const outHasVelocityPtr = basePtr + 68; // u8
  const outCprOddPtr = basePtr + 69; // u8
  const outCprSurfacePtr = basePtr + 70; // u8
  const outHasCprPtr = basePtr + 71; // u8

  new Uint8Array(wasm.memory.buffer, framePtr, 14).set(bytes);

  const ok = fn(
    framePtr,
    outIcaoPtr,
    outTypeCodePtr,
    outMsgTypePtr,
    outCallsignPtr,
    outAltitudeFtPtr,
    outHasAltitudePtr,
    outOnGroundPtr,
    outSpeedKtPtr,
    outHeadingDegPtr,
    outVrFpmPtr,
    outHasVelocityPtr,
    outCprOddPtr,
    outCprSurfacePtr,
    outCprLatPtr,
    outCprLonPtr,
    outHasCprPtr,
  );

  if (!ok) return null;

  const buf = wasm.memory.buffer;
  const u8 = new Uint8Array(buf);
  const f64 = new Float64Array(buf, basePtr, 2);
  const i32 = new Int32Array(buf, basePtr + 16, 2);
  const u32 = new Uint32Array(buf, basePtr + 24, 2);

  const icao = String.fromCharCode(...u8.subarray(outIcaoPtr, outIcaoPtr + 6));
  const typeCode = u8[outTypeCodePtr] ?? 0;
  const msgType = u8[outMsgTypePtr] ?? 4;
  const downlinkFormat = bits(bytes, 0, 5);

  if (msgType === 0) {
    const callsign = String.fromCharCode(...u8.subarray(outCallsignPtr, outCallsignPtr + 8)).trim();
    const category = `${String.fromCharCode(69 - typeCode)}${bits(bytes, 37, 3)}`;
    return { icao, typeCode, downlinkFormat, receivedAt, callsign, category };
  }

  if (msgType === 1) {
    const speedKt = decodeSurfaceMovement(bits(bytes, 37, 7));
    const hasHeading = Boolean(bit(bytes, 44));
    const headingDeg = hasHeading ? (bits(bytes, 45, 7) * 360) / 128 : undefined;
    return {
      icao,
      typeCode,
      downlinkFormat,
      receivedAt,
      onGround: true,
      altitudeFt: null,
      ...(speedKt !== null ? { speedKt } : {}),
      ...(headingDeg !== undefined ? { headingDeg } : {}),
      cpr: {
        surface: true,
        odd: Boolean(u8[outCprOddPtr]),
        latitude: u32[0] ?? 0,
        longitude: u32[1] ?? 0,
        receivedAt,
      },
    };
  }

  if (msgType === 2) {
    const hasAltitude = Boolean(u8[outHasAltitudePtr]);
    const altitudeFt = hasAltitude ? (i32[0] ?? 0) : null;
    const hasCpr = Boolean(u8[outHasCprPtr]);
    return {
      icao,
      typeCode,
      downlinkFormat,
      receivedAt,
      onGround: Boolean(u8[outOnGroundPtr]),
      altitudeFt,
      ...(hasCpr
        ? {
            cpr: {
              odd: Boolean(u8[outCprOddPtr]),
              ...(u8[outCprSurfacePtr] ? { surface: true } : {}),
              latitude: u32[0] ?? 0,
              longitude: u32[1] ?? 0,
              receivedAt,
            },
          }
        : {}),
    };
  }

  if (msgType === 3) {
    const hasVelocity = Boolean(u8[outHasVelocityPtr]);
    if (!hasVelocity) {
      return { icao, typeCode, downlinkFormat, receivedAt };
    }
    const vrRaw = bits(bytes, 69, 9);
    return {
      icao,
      typeCode,
      downlinkFormat,
      receivedAt,
      speedKt: f64[0] ?? 0,
      headingDeg: f64[1] ?? 0,
      verticalRateFpm: vrRaw > 0 ? (i32[1] ?? 0) : null,
    };
  }

  return { icao, typeCode, downlinkFormat, receivedAt };
}

/**
 * Parses a 28-character hexadecimal Mode-S frame (with optional `*` prefix and `;` suffix)
 * into a 14-byte `Uint8Array`. Returns `null` if malformed.
 */
export function parseAdsbHex(hex: string): Uint8Array | null {
  const cleaned = hex
    .trim()
    .replace(/^\*|;$/g, '')
    .trim();
  if (!/^[0-9a-fA-F]{28}$/.test(cleaned)) return null;
  const out = new Uint8Array(14);
  for (let i = 0; i < 14; i++) {
    out[i] = Number.parseInt(cleaned.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/**
 * Decodes a 28-character hexadecimal DF17/DF18 Extended Squitter frame string.
 */
export function decodeAdsbFrameHex(
  hex: string,
  options?: { receivedAt?: number },
): DecodedAdsbMessage | null {
  const bytes = parseAdsbHex(hex);
  if (!bytes) return null;
  return decodeAdsbMessage(bytes, options);
}

/**
 * Pure-TypeScript global airborne CPR decoding from an even/odd frame pair.
 */
export function decodeCprAirbornePairTs(
  even: CprFrameInput,
  odd: CprFrameInput,
): DecodedGeoPosition | null {
  if (!even || !odd) return null;
  if (
    even.receivedAt !== undefined &&
    odd.receivedAt !== undefined &&
    Math.abs(even.receivedAt - odd.receivedAt) > CPR_PAIR_MAX_AGE_MS
  ) {
    return null;
  }

  const latitudeIndex = Math.floor((59 * even.latitude - 60 * odd.latitude) / CPR_SCALE + 0.5);
  let evenLatitude = 6 * (positiveModulo(latitudeIndex, 60) + even.latitude / CPR_SCALE);
  let oddLatitude = (360 / 59) * (positiveModulo(latitudeIndex, 59) + odd.latitude / CPR_SCALE);
  if (evenLatitude >= 270) evenLatitude -= 360;
  if (oddLatitude >= 270) oddLatitude -= 360;
  if (cprNlTs(evenLatitude) !== cprNlTs(oddLatitude)) return null;

  const useOdd =
    even.receivedAt !== undefined && odd.receivedAt !== undefined
      ? odd.receivedAt >= even.receivedAt
      : true;
  const latitude = useOdd ? oddLatitude : evenLatitude;
  if (latitude < -90 || latitude > 90) return null;

  const nl = cprNlTs(latitude);
  const longitudeZones = Math.max(nl - (useOdd ? 1 : 0), 1);
  const longitudeIndex = Math.floor(
    (even.longitude * (nl - 1) - odd.longitude * nl) / CPR_SCALE + 0.5,
  );
  const frame = useOdd ? odd : even;
  const longitude = normalizeLongitude(
    (360 / longitudeZones) *
      (positiveModulo(longitudeIndex, longitudeZones) + frame.longitude / CPR_SCALE),
  );
  return { latitude, longitude };
}

/**
 * Globally decodes an even/odd airborne CPR frame pair using the Zig WASM export
 * `lavender_decode_global_airborne_cpr` when loaded, falling back to `decodeCprAirbornePairTs`.
 */
export function decodeCprAirbornePair(
  even: CprFrameInput,
  odd: CprFrameInput,
): DecodedGeoPosition | null {
  if (!even || !odd) return null;
  if (
    even.receivedAt !== undefined &&
    odd.receivedAt !== undefined &&
    Math.abs(even.receivedAt - odd.receivedAt) > CPR_PAIR_MAX_AGE_MS
  ) {
    return null;
  }

  const useOdd =
    even.receivedAt !== undefined && odd.receivedAt !== undefined
      ? odd.receivedAt >= even.receivedAt
      : true;

  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as SdrWasmExports | undefined)
    ?.lavender_decode_global_airborne_cpr;
  if (wasm && fn) {
    const basePtr = align8(wasm.heapBase + SDR_SCRATCH_OFFSET);
    ensureMemory(wasm, basePtr + 16);
    const ok = fn(
      even.latitude >>> 0,
      even.longitude >>> 0,
      odd.latitude >>> 0,
      odd.longitude >>> 0,
      useOdd,
      basePtr,
      basePtr + 8,
    );
    if (!ok) return null;
    const f64 = new Float64Array(wasm.memory.buffer, basePtr, 2);
    return {
      latitude: f64[0] ?? 0,
      longitude: f64[1] ?? 0,
    };
  }

  return decodeCprAirbornePairTs(even, odd);
}

/**
 * Local CPR decoding of a single frame against a known reference position within half a zone.
 */
export function decodeLocalCpr(
  frame: CprFrameInput,
  reference: DecodedGeoPosition | null | undefined,
): DecodedGeoPosition | null {
  if (!reference || !Number.isFinite(reference.latitude) || !Number.isFinite(reference.longitude)) {
    return null;
  }
  const span = frame.surface ? 90 : 360;
  const parity = frame.odd ? 1 : 0;
  const dLat = span / (4 * CPR_NZ - parity);
  const latitudeIndex =
    Math.floor(reference.latitude / dLat) +
    Math.floor(0.5 + positiveModulo(reference.latitude, dLat) / dLat - frame.latitude / CPR_SCALE);
  let latitude = dLat * (latitudeIndex + frame.latitude / CPR_SCALE);
  if (latitude >= 270) latitude -= 360;
  if (latitude < -90 || latitude > 90) return null;

  const longitudeZones = Math.max(cprNlTs(latitude) - parity, 1);
  const dLon = span / longitudeZones;
  const longitudeIndex =
    Math.floor(reference.longitude / dLon) +
    Math.floor(
      0.5 + positiveModulo(reference.longitude, dLon) / dLon - frame.longitude / CPR_SCALE,
    );
  const longitude = normalizeLongitude(dLon * (longitudeIndex + frame.longitude / CPR_SCALE));
  return { latitude, longitude };
}

function nearestAlias(value: number, period: number, target: number): number {
  return value + period * Math.round((target - value) / period);
}

/**
 * Globally decodes an even/odd pair of surface position frames near `reference`.
 */
export function decodeGlobalSurfaceCpr(
  even: CprFrameInput | null | undefined,
  odd: CprFrameInput | null | undefined,
  reference: DecodedGeoPosition | null | undefined,
): DecodedGeoPosition | null {
  if (
    !even ||
    !odd ||
    !reference ||
    !Number.isFinite(reference.latitude) ||
    !Number.isFinite(reference.longitude)
  ) {
    return null;
  }
  if (
    even.receivedAt !== undefined &&
    odd.receivedAt !== undefined &&
    Math.abs(even.receivedAt - odd.receivedAt) > CPR_SURFACE_PAIR_MAX_AGE_MS
  ) {
    return null;
  }
  const latitudeIndex = Math.floor((59 * even.latitude - 60 * odd.latitude) / CPR_SCALE + 0.5);
  const evenLatitude = nearestAlias(
    (90 / 60) * (positiveModulo(latitudeIndex, 60) + even.latitude / CPR_SCALE),
    90,
    reference.latitude,
  );
  const oddLatitude = nearestAlias(
    (90 / 59) * (positiveModulo(latitudeIndex, 59) + odd.latitude / CPR_SCALE),
    90,
    reference.latitude,
  );
  if (Math.abs(evenLatitude) > 90 || Math.abs(oddLatitude) > 90) return null;
  if (cprNlTs(evenLatitude) !== cprNlTs(oddLatitude)) return null;

  const useOdd =
    even.receivedAt !== undefined && odd.receivedAt !== undefined
      ? odd.receivedAt >= even.receivedAt
      : true;
  const latitude = useOdd ? oddLatitude : evenLatitude;
  const nl = cprNlTs(latitude);
  const longitudeZones = Math.max(nl - (useOdd ? 1 : 0), 1);
  const longitudeIndex = Math.floor(
    (even.longitude * (nl - 1) - odd.longitude * nl) / CPR_SCALE + 0.5,
  );
  const frame = useOdd ? odd : even;
  const longitude = normalizeLongitude(
    nearestAlias(
      (90 / longitudeZones) *
        (positiveModulo(longitudeIndex, longitudeZones) + frame.longitude / CPR_SCALE),
      90,
      reference.longitude,
    ),
  );
  return { latitude, longitude };
}

function preambleContrast(power: Float32Array, offset: number): number {
  const pulseOffsets = [0, 2, 7, 9] as const;
  const quietOffsets = [1, 3, 4, 5, 6, 8] as const;
  let pulseMean = 0;
  let quietMean = 0;
  for (const index of pulseOffsets) pulseMean += power[offset + index] ?? 0;
  for (const index of quietOffsets) quietMean += power[offset + index] ?? 0;
  pulseMean /= pulseOffsets.length;
  quietMean /= quietOffsets.length;
  const contrast = pulseMean - quietMean;
  if (contrast < 120 || pulseMean < quietMean * 1.8) return 0;
  const decisionLevel = quietMean + contrast * 0.35;
  for (const index of pulseOffsets) {
    if ((power[offset + index] ?? 0) <= decisionLevel) return 0;
  }
  return contrast;
}

/**
 * Pure-TypeScript 2 Msps I/Q preamble detector and Manchester bit demodulator.
 * Extracts CRC-valid 14-byte Mode-S frames from unsigned 8-bit interleaved I/Q samples.
 */
export function demodulateIqBufferTs(
  buffer: ArrayBuffer | Uint8Array,
  sampleRate = 2_000_000,
): Uint8Array[] {
  if (Math.abs(sampleRate - 2_000_000) > 30_000) return [];
  const iq = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
  const sampleCount = Math.floor(iq.length / 2);
  if (sampleCount < FRAME_SAMPLES) return [];

  const power = new Float32Array(sampleCount);
  for (let index = 0; index < sampleCount; index += 1) {
    const i = (iq[index * 2] ?? 128) - 127.5;
    const q = (iq[index * 2 + 1] ?? 128) - 127.5;
    power[index] = i * i + q * q;
  }

  const messages: Uint8Array[] = [];
  for (let offset = 0; offset <= sampleCount - FRAME_SAMPLES; offset += 1) {
    const contrast = preambleContrast(power, offset);
    if (!contrast) continue;
    const bytes = new Uint8Array(14);
    let uncertainBits = 0;
    for (let bitIndex = 0; bitIndex < FRAME_BITS; bitIndex += 1) {
      const sampleOffset = offset + PREAMBLE_SAMPLES + bitIndex * 2;
      const first = power[sampleOffset] ?? 0;
      const second = power[sampleOffset + 1] ?? 0;
      const total = first + second;
      const difference = Math.abs(first - second);
      if (difference < Math.max(20, contrast * 0.02) || difference / Math.max(total, 1) < 0.04) {
        uncertainBits += 1;
      }
      if (first > second) {
        const byteIdx = bitIndex >> 3;
        bytes[byteIdx] = (bytes[byteIdx] ?? 0) | (1 << (7 - (bitIndex & 7)));
      }
    }
    if (uncertainBits <= 10 && modeSChecksumTs(bytes) === 0) {
      messages.push(bytes);
      offset += FRAME_SAMPLES - 1;
    }
  }
  return messages;
}

/**
 * Demodulates unsigned 8-bit interleaved 2 Msps I/Q samples into CRC-valid 14-byte
 * Mode-S ADS-B frames, delegating to `lavender_iq_extract_adsb_frames` when the Zig
 * WASM kernel is loaded and falling back to `demodulateIqBufferTs` otherwise.
 */
export function demodulateIqBuffer(
  buffer: ArrayBuffer | Uint8Array,
  sampleRate = 2_000_000,
): Uint8Array[] {
  if (Math.abs(sampleRate - 2_000_000) > 30_000) return [];
  const iq = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
  const sampleCount = Math.floor(iq.length / 2);
  if (sampleCount < FRAME_SAMPLES) return [];

  const wasm = getActiveLavenderWasm();
  const fn = (wasm?.exports as unknown as SdrWasmExports | undefined)
    ?.lavender_iq_extract_adsb_frames;
  if (!wasm || !fn) {
    return demodulateIqBufferTs(iq, sampleRate);
  }

  const maxFrames = Math.min(
    MAX_IQ_FRAMES,
    Math.max(1, Math.floor(sampleCount / FRAME_SAMPLES) + 1),
  );
  const basePtr = align8(wasm.heapBase + SDR_SCRATCH_OFFSET);
  const powerScratchPtr = basePtr;
  const powerBytes = sampleCount * 4;
  const iqPtr = powerScratchPtr + powerBytes;
  const outFramesPtr = iqPtr + iq.byteLength;
  const totalBytes = align8(outFramesPtr + maxFrames * 14);

  ensureMemory(wasm, totalBytes);
  new Uint8Array(wasm.memory.buffer, iqPtr, iq.byteLength).set(iq);

  const count = fn(iqPtr, iq.byteLength, powerScratchPtr, sampleCount, outFramesPtr, maxFrames);
  const frames: Uint8Array[] = [];
  for (let i = 0; i < count; i++) {
    const start = outFramesPtr + i * 14;
    frames.push(new Uint8Array(wasm.memory.buffer, start, 14).slice());
  }
  return frames;
}

/** Alias matching `legacy/src/sdr/adsbDecoder.js`. */
export const extractAdsbMessages = demodulateIqBuffer;

/**
 * Preserves enough trailing I/Q samples between consecutive USB transfers to recover
 * Mode-S frames split across block boundaries.
 */
export class AdsbStreamDecoder {
  #carry: Uint8Array = new Uint8Array(0);

  reset(): void {
    this.#carry = new Uint8Array(0);
  }

  extract(buffer: ArrayBuffer | Uint8Array, sampleRate = 2_000_000): Uint8Array[] {
    const incoming = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
    if (incoming.length === 0) return [];
    let combined = incoming;
    if (this.#carry.length > 0) {
      combined = new Uint8Array(this.#carry.length + incoming.length);
      combined.set(this.#carry);
      combined.set(incoming, this.#carry.length);
    }
    this.#carry = combined.slice(Math.max(0, combined.length - STREAM_CARRY_BYTES));
    return demodulateIqBuffer(combined, sampleRate);
  }
}
