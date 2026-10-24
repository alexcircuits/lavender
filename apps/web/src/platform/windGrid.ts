/**
 * Reads the binary wind grid both transports return (`windGrid`). The format is fixed by
 * lavender-core (see `WindGridDef`): IEEE-754 binary32, little-endian, no header; the U
 * block, the V block, then the scalar block when the manifest has `scalar`. Each block is
 * `ny` rows of `nx` values, row 0 at `la1` (90° N) stepping south, column 0 at `lo1` (0° E)
 * stepping east. Values are read explicitly little-endian, whatever the host's byte order.
 */
import type { WindManifest } from '../contracts';
import { PlatformError } from './types';

export interface WindGridData {
  readonly nx: number;
  readonly ny: number;
  readonly lo1?: number;
  readonly la1?: number;
  readonly dx?: number;
  readonly dy?: number;
  readonly di?: number;
  readonly dj?: number;
  /** Eastward wind, m/s, row-major from the north-west corner. */
  readonly u: Float32Array;
  /** Northward wind, m/s, same layout. */
  readonly v: Float32Array;
  /** 2 m temperature (°C) or mean sea level pressure (hPa) when the manifest has `scalar`. */
  readonly scalar?: Float32Array;
}

const mismatch = (message: string): PlatformError =>
  new PlatformError({ code: 'internal', message, retryable: true });

/** Splits a grid into its blocks, checking its size against the manifest's geometry. */
export function decodeWindGrid(buffer: ArrayBuffer, manifest: WindManifest): WindGridData {
  const grid = manifest.grid;
  if (!grid) throw mismatch('The wind manifest describes no grid.');
  const points = grid.nx * grid.ny;
  const blocks = manifest.scalar ? 3 : 2;
  if (buffer.byteLength !== points * 4 * blocks) {
    throw mismatch(
      `The wind grid has ${buffer.byteLength} bytes; its manifest expects ${points * 4 * blocks}.`,
    );
  }
  const view = new DataView(buffer);
  const block = (index: number): Float32Array => {
    const values = new Float32Array(points);
    const start = index * points * 4;
    for (let i = 0; i < points; i += 1) values[i] = view.getFloat32(start + i * 4, true);
    return values;
  };
  const u = block(0);
  const v = block(1);
  return blocks === 3
    ? {
        nx: grid.nx,
        ny: grid.ny,
        lo1: grid.lo1,
        la1: grid.la1,
        dx: grid.dx,
        dy: grid.dy,
        di: grid.dx,
        dj: grid.dy,
        u,
        v,
        scalar: block(2),
      }
    : {
        nx: grid.nx,
        ny: grid.ny,
        lo1: grid.lo1,
        la1: grid.la1,
        dx: grid.dx,
        dy: grid.dy,
        di: grid.dx,
        dj: grid.dy,
        u,
        v,
      };
}
