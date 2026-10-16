import { describe, expect, it } from 'vitest';
import type { WindManifest } from '../contracts';
import { PlatformError } from './types';
import { decodeWindGrid } from './windGrid';

const manifest = (overrides: Partial<WindManifest> = {}): WindManifest => ({
  schemaVersion: 1,
  model: 'gfs',
  grid: { nx: 3, ny: 2, lo1: 0, la1: 90, dx: 120, dy: 180 },
  stale: false,
  unavailable: false,
  reason: null,
  ...overrides,
});

/** Writes values the way lavender-core does: binary32, least-significant byte first. */
function littleEndian(values: number[]): ArrayBuffer {
  const view = new DataView(new ArrayBuffer(values.length * 4));
  for (const [i, value] of values.entries()) view.setFloat32(i * 4, value, true);
  return view.buffer;
}

describe('decodeWindGrid', () => {
  it('splits U and V blocks in row-major order from the north-west corner', () => {
    const u = [1, 2, 3, 4, 5, 6];
    const v = [-1, -2, -3, -4, -5, -6];
    const grid = decodeWindGrid(littleEndian([...u, ...v]), manifest());
    expect(grid.nx).toBe(3);
    expect(grid.ny).toBe(2);
    expect([...grid.u]).toEqual(u);
    expect([...grid.v]).toEqual(v);
    expect(grid.scalar).toBeUndefined();
    // Row 1 (90° S), column 2 (240° E) of U.
    expect(grid.u[1 * 3 + 2]).toBe(6);
  });

  it('reads the scalar block when the manifest has a scalar', () => {
    const withScalar = manifest({
      overlay: 'pressure',
      scalar: { kind: 'pressure', units: 'hPa', level: 'mean sea level' },
    });
    const scalar = [1013.25, 1000, 990.5, 1020, 1008.75, 1001];
    const wind = new Array<number>(12).fill(0);
    const grid = decodeWindGrid(littleEndian([...wind, ...scalar]), withScalar);
    expect([...(grid.scalar ?? [])]).toEqual(scalar);
  });

  it('reads little-endian bytes regardless of the host byte order', () => {
    const bytes = new Uint8Array(12 * 4);
    bytes.set([0x00, 0x00, 0xc0, 0x3f], 0); // 1.5, least-significant byte first
    const grid = decodeWindGrid(bytes.buffer, manifest());
    expect(grid.u[0]).toBe(1.5);
  });

  it('refuses a body whose size does not match the manifest', () => {
    const tooShort = () => decodeWindGrid(littleEndian([1, 2, 3]), manifest());
    expect(tooShort).toThrow(PlatformError);
    expect(tooShort).toThrow(/expects 48/);
    const noGrid = manifest();
    delete noGrid.grid;
    expect(() => decodeWindGrid(new ArrayBuffer(0), noGrid)).toThrow(/no grid/);
  });
});
