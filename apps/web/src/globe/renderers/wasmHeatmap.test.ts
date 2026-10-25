import { beforeAll, describe, expect, it } from 'vitest';
import { heatmapBin, heatmapBinTs, loadLavenderWasm } from './wasmHeatmap';

describe('wasmHeatmap (Zig WASM Heatmap Binning Kernel)', () => {
  // The path under test (`heatmapBin`) uses the Zig kernel when the loaded
  // artifact exports it and the identical TypeScript fallback otherwise, so
  // these expectations hold on every build.
  beforeAll(async () => {
    await loadLavenderWasm();
  });

  it('bins weighted points into the correct cells with north at row 0', () => {
    // 2×2 grid over lon [-10, 10], lat [-10, 10]: row 0 = north, col 0 = west.
    const points = [
      -5,
      5,
      2, // north-west cell: (row 0, col 0)
      5,
      5,
      3, // north-east cell: (row 0, col 1)
      -5,
      -5,
      4, // south-west cell: (row 1, col 0)
    ];
    const grid = heatmapBin(points, 2, 2, { lonMin: -10, lonMax: 10, latMin: -10, latMax: 10 });
    expect(grid.binned).toBe(3);
    expect(grid.weightSum[0]).toBeCloseTo(2, 5); // NW
    expect(grid.count[0]).toBe(1);
    expect(grid.weightSum[1]).toBeCloseTo(3, 5); // NE
    expect(grid.weightSum[2]).toBeCloseTo(4, 5); // SW
    expect(grid.count[3]).toBe(0); // SE stays empty
  });

  it('accumulates batches additively into the same cell', () => {
    const grid = heatmapBin([0, 0, 1.5, 0.25, -0.25, 2.5], 1, 1, {
      lonMin: -1,
      lonMax: 1,
      latMin: -1,
      latMax: 1,
    });
    expect(grid.binned).toBe(2);
    expect(grid.count[0]).toBe(2);
    expect(grid.weightSum[0]).toBeCloseTo(4, 5);
  });

  it('skips out-of-bounds points', () => {
    const grid = heatmapBin([0, 0, 1, 20, 0, 5, 0, 20, 5], 1, 1, {
      lonMin: -1,
      lonMax: 1,
      latMin: -1,
      latMax: 1,
    });
    expect(grid.binned).toBe(1);
    expect(grid.count[0]).toBe(1);
    expect(grid.weightSum[0]).toBeCloseTo(1, 5);
  });

  it('returns an empty grid for empty input without touching the kernel', () => {
    const grid = heatmapBin([], 3, 3, { lonMin: 0, lonMax: 1, latMin: 0, latMax: 1 });
    expect(grid.binned).toBe(0);
    expect(grid.weightSum).toHaveLength(9);
    expect(grid.weightSum.every((v) => v === 0)).toBe(true);
  });

  it('rejects nonsense dimensions', () => {
    expect(() =>
      heatmapBin([0, 0, 1], 0, 4, { lonMin: 0, lonMax: 1, latMin: 0, latMax: 1 }),
    ).toThrow(/positive dimensions/);
  });
});

describe('heatmapBinTs (reference fallback)', () => {
  it('matches the shared layout: row 0 north, additive, skips out-of-bounds', () => {
    const grid = heatmapBinTs([-5, 5, 2, -5, 5, 1, 99, 0, 5], 2, 2, {
      lonMin: -10,
      lonMax: 10,
      latMin: -10,
      latMax: 10,
    });
    expect(grid.binned).toBe(2);
    expect(grid.weightSum[0]).toBeCloseTo(3, 5);
    expect(grid.count[0]).toBe(2);
  });
});
