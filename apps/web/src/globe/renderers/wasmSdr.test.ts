import { beforeAll, describe, expect, it } from 'vitest';
import {
  AdsbStreamDecoder,
  decodeAdsbFrameHex,
  decodeAdsbFrameTs,
  decodeAdsbMessage,
  decodeCprAirbornePair,
  decodeCprAirbornePairTs,
  demodulateIqBuffer,
  demodulateIqBufferTs,
  loadLavenderWasm,
  modeSChecksum,
  modeSChecksumTs,
  parseAdsbHex,
} from './wasmSdr';

function mustParseHex(hex: string): Uint8Array {
  const parsed = parseAdsbHex(hex);
  if (!parsed) throw new Error(`Invalid test hex: ${hex}`);
  return parsed;
}

function synthesizeIq(
  bytes: Uint8Array,
  {
    floor = 128,
    high = 255,
    preambleTail = null,
  }: { floor?: number; high?: number; preambleTail?: number | null } = {},
): ArrayBuffer {
  const sampleCount = 16 + 112 * 2 + 8;
  const iq = new Uint8Array(sampleCount * 2).fill(128);
  for (let sample = 0; sample < sampleCount; sample += 1) {
    iq[sample * 2] = floor;
  }
  const pulse = (sample: number) => {
    iq[sample * 2] = high;
  };
  for (const offset of [0, 2, 7, 9]) pulse(offset);
  if (preambleTail !== null && Number.isFinite(preambleTail)) {
    for (let sample = 10; sample < 16; sample += 1) {
      iq[sample * 2] = preambleTail;
    }
  }
  for (let index = 0; index < 112; index += 1) {
    const value = ((bytes[index >> 3] ?? 0) >> (7 - (index & 7))) & 1;
    pulse(16 + index * 2 + (value ? 0 : 1));
  }
  return iq.buffer;
}

describe('wasmSdr (Zig WASM Mode-S ADS-B Decoder & 2 Msps I/Q Demodulator)', () => {
  beforeAll(async () => {
    await loadLavenderWasm();
  });

  it('verifies CRC-24 Mode-S parity check and rejects corrupted frames', () => {
    const valid = mustParseHex('8D4840D6202CC371C32CE0576098');
    expect(modeSChecksumTs(valid)).toBe(0);
    expect(modeSChecksum(valid)).toBe(0);

    const corrupted = valid.slice();
    corrupted[5] = (corrupted[5] ?? 0) ^ 0x01;
    expect(modeSChecksumTs(corrupted)).not.toBe(0);
    expect(modeSChecksum(corrupted)).not.toBe(0);
    expect(decodeAdsbMessage(corrupted)).toBeNull();
  });

  it('decodes DF17 aircraft identification (callsign and emitter category)', () => {
    const msg = decodeAdsbFrameHex('8D4840D6202CC371C32CE0576098', { receivedAt: 1000 });
    expect(msg).not.toBeNull();
    expect(msg?.icao).toBe('4840D6');
    expect(msg?.typeCode).toBe(4);
    expect(msg?.downlinkFormat).toBe(17);
    expect(msg?.callsign).toBe('KLM1023');
    expect(msg?.category).toBe('A0');

    const tsMsg = decodeAdsbFrameTs(mustParseHex('8D4840D6202CC371C32CE0576098'), {
      receivedAt: 1000,
    });
    expect(msg).toEqual(tsMsg);
  });

  it('decodes DF17 airborne position altitude (25-ft Q-bit) and CPR fields', () => {
    const msg = decodeAdsbFrameHex('8D40621D58C382D690C8AC2863A7', { receivedAt: 1000 });
    expect(msg).not.toBeNull();
    expect(msg?.icao).toBe('40621D');
    expect(msg?.typeCode).toBe(11);
    expect(msg?.onGround).toBe(false);
    expect(msg?.altitudeFt).toBe(38_000);
    expect(msg?.cpr).toEqual({
      odd: false,
      latitude: 93_000,
      longitude: 51_372,
      receivedAt: 1000,
    });
  });

  it('decodes DF17 airborne velocity (ground speed, heading, and vertical rate)', () => {
    const msg = decodeAdsbFrameHex('8D485020994409940838175B284F', { receivedAt: 2000 });
    expect(msg).not.toBeNull();
    expect(msg?.icao).toBe('485020');
    expect(msg?.typeCode).toBe(19);
    expect(msg?.speedKt).toBeCloseTo(159.2, 1);
    expect(msg?.headingDeg).toBeCloseTo(182.88, 2);
    expect(msg?.verticalRateFpm).toBe(-832);
  });

  it('decodes global airborne CPR even/odd pair to exact WGS84 coordinates', () => {
    const even = decodeAdsbFrameHex('8D40621D58C382D690C8AC2863A7', { receivedAt: 1000 });
    const odd = decodeAdsbFrameHex('8D40621D58C386435CC412692AD6', { receivedAt: 1500 });
    expect(even?.cpr).toBeDefined();
    expect(odd?.cpr).toBeDefined();
    if (!even?.cpr || !odd?.cpr) return;

    const posWasm = decodeCprAirbornePair(even.cpr, odd.cpr);
    const posTs = decodeCprAirbornePairTs(even.cpr, odd.cpr);

    expect(posWasm).not.toBeNull();
    expect(posTs).not.toBeNull();
    if (!posWasm || !posTs) return;

    expect(posWasm.latitude).toBeCloseTo(52.26578, 4);
    expect(posWasm.longitude).toBeCloseTo(3.93891, 4);
    expect(posWasm.latitude).toBeCloseTo(posTs.latitude, 6);
    expect(posWasm.longitude).toBeCloseTo(posTs.longitude, 6);

    // Rejects pair older than 10 seconds
    const expiredOdd = { ...odd.cpr, receivedAt: 15_000 };
    expect(decodeCprAirbornePair(even.cpr, expiredOdd)).toBeNull();
  });

  it('demodulates 2 Msps I/Q buffers and recovers split frames across USB blocks', () => {
    const bytes = mustParseHex('8D4840D6202CC371C32CE0576098');
    const iqBuffer = synthesizeIq(bytes, { floor: 138, high: 148, preambleTail: 145 });

    const tsFrames = demodulateIqBufferTs(iqBuffer, 2_000_000);
    const wasmFrames = demodulateIqBuffer(iqBuffer, 2_000_000);

    expect(tsFrames).toHaveLength(1);
    expect(wasmFrames).toHaveLength(1);
    expect(wasmFrames[0]).toEqual(bytes);
    expect(tsFrames[0]).toEqual(bytes);

    // Rejects unsupported sample rates
    expect(demodulateIqBuffer(iqBuffer, 1_024_000)).toEqual([]);

    // Recovers frame split across two USB transfers via AdsbStreamDecoder
    const iqBytes = new Uint8Array(synthesizeIq(bytes));
    const stream = new AdsbStreamDecoder();
    expect(stream.extract(iqBytes.slice(0, 300).buffer, 2_000_000)).toEqual([]);
    expect(stream.extract(iqBytes.slice(300).buffer, 2_000_000)).toEqual([bytes]);
  });
});
