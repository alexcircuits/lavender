import { describe, expect, it } from 'vitest';
import {
  AT_REST_KNOTS,
  THIN_MIN_HEIGHT_M,
  type ThinCandidate,
  thinnedOut,
  thinningCellDeg,
  thinningKeepers,
  vesselLabel,
  vesselSymbol,
} from './vesselSymbology';

describe('vesselSymbol', () => {
  it('points a ship making way along its course over ground', () => {
    expect(vesselSymbol({ speedKnots: 11.5, courseDeg: 88, headingDeg: 89 })).toEqual({
      kind: 'underway',
      bearingDeg: 88,
    });
    expect(vesselSymbol({ speedKnots: 0.5, courseDeg: 0 })).toEqual({
      kind: 'underway',
      bearingDeg: 0,
    });
  });

  it('falls back to the true heading when no course is reported', () => {
    expect(vesselSymbol({ speedKnots: 4.2, headingDeg: 120 })).toEqual({
      kind: 'underway',
      bearingDeg: 120,
    });
    // AIS heading 360 is north; course 360 is "not available".
    expect(vesselSymbol({ speedKnots: 4.2, courseDeg: 360, headingDeg: 360 })).toEqual({
      kind: 'underway',
      bearingDeg: 0,
    });
  });

  it('draws a dot at rest, with unknown speed or with neither course nor heading', () => {
    const atRest = { kind: 'at-rest', bearingDeg: null };
    expect(vesselSymbol({ speedKnots: AT_REST_KNOTS - 0.01, courseDeg: 90 })).toEqual(atRest);
    expect(vesselSymbol({ speedKnots: 0, courseDeg: 0, headingDeg: 270 })).toEqual(atRest);
    expect(vesselSymbol({ courseDeg: 90, headingDeg: 90 })).toEqual(atRest);
    expect(vesselSymbol({ speedKnots: 9 })).toEqual(atRest);
  });

  it('treats AIS "not available" codes and malformed values as unknown', () => {
    const atRest = { kind: 'at-rest', bearingDeg: null };
    expect(vesselSymbol({ speedKnots: 102.3, courseDeg: 90 })).toEqual(atRest);
    expect(vesselSymbol({ speedKnots: -1, courseDeg: 90 })).toEqual(atRest);
    expect(vesselSymbol({ speedKnots: Number.NaN, courseDeg: 90 })).toEqual(atRest);
    expect(vesselSymbol({ speedKnots: 8, courseDeg: 360, headingDeg: 511 })).toEqual(atRest);
    expect(vesselSymbol({ speedKnots: 8, courseDeg: -5, headingDeg: Number.NaN })).toEqual(atRest);
    // A newer server's wire value of the wrong type must not become a bearing.
    expect(
      vesselSymbol({ speedKnots: '8', courseDeg: 90 } as unknown as { speedKnots: number }),
    ).toEqual(atRest);
  });
});

describe('vesselLabel', () => {
  it('labels with the trimmed name, or not at all', () => {
    expect(vesselLabel({ name: ' TEST TRADER ' })).toBe('TEST TRADER');
    expect(vesselLabel({ name: '   ' })).toBeNull();
    expect(vesselLabel({})).toBeNull();
    expect(vesselLabel({ name: 'A VERY LONG SHIP NAME INDEED' })).toHaveLength(20);
  });
});

describe('low-zoom thinning', () => {
  const ship = (lat: number, lon: number, underway = true): ThinCandidate => ({
    lat,
    lon,
    underway,
  });

  it('draws every ship at or below the threshold height', () => {
    expect(thinningCellDeg(THIN_MIN_HEIGHT_M)).toBeNull();
    expect(thinningCellDeg(500_000)).toBeNull();
    expect(thinningCellDeg(Number.NaN)).toBeNull();
    const pairs = [
      ['1', ship(55, 4)],
      ['2', ship(55, 4)],
    ] as const;
    expect(thinnedOut(pairs, null, null).size).toBe(0);
  });

  it('snaps cell sizes to zoom levels and grows them with height', () => {
    const near = thinningCellDeg(2_100_000);
    expect(near).not.toBeNull();
    expect(thinningCellDeg(2_400_000)).toBe(near);
    const world = thinningCellDeg(20_000_000) ?? 0;
    expect(world).toBeGreaterThan(near ?? 0);
    // About 24 px on a 1440 px wide globe: a world view (snapped to 16 000 km) has ~2.8° cells.
    expect(world).toBeGreaterThan(2.6);
    expect(world).toBeLessThan(3);
  });

  it('sizes cells in screen pixels of the actual canvas (its longer side spans the 60° view)', () => {
    const reference = thinningCellDeg(20_000_000, 1440) ?? 0;
    expect(thinningCellDeg(20_000_000)).toBe(reference);
    expect(thinningCellDeg(20_000_000, 2880)).toBeCloseTo(reference / 2);
    expect(thinningCellDeg(20_000_000, 720)).toBeCloseTo(reference * 2);
    // An unknown or empty canvas falls back to the 1440 px reference.
    expect(thinningCellDeg(20_000_000, 0)).toBe(reference);
    expect(thinningCellDeg(20_000_000, Number.NaN)).toBe(reference);
  });

  it('keeps one ship per cell: under way first, then the lowest MMSI, whatever the order', () => {
    const cell = 1;
    const pairs: (readonly [string, ThinCandidate])[] = [
      ['300000003', ship(55.1, 4.1, false)],
      ['300000002', ship(55.2, 4.2)],
      ['300000001', ship(55.3, 4.3, false)],
      ['300000004', ship(55.4, 4.4)],
      ['300000009', ship(10.5, 4.5)],
    ];
    const hidden = thinnedOut(pairs, cell, null);
    expect([...hidden].sort()).toEqual(['300000001', '300000003', '300000004']);
    expect(thinnedOut([...pairs].reverse(), cell, null)).toEqual(hidden);
  });

  it('returns the very candidates that represent their cells, for the renderer’s hot path', () => {
    const a = { id: '300000002', ...ship(55.2, 4.2) };
    const b = { id: '300000001', ...ship(55.3, 4.3, false) };
    const far = { id: '300000009', ...ship(10.5, 4.5, false) };
    const kept = thinningKeepers([b, a, far], 1, null);
    expect(kept).toEqual(new Set([a, far]));
    expect(kept.has(a)).toBe(true); // identity, not a copy
    expect(thinningKeepers([a, b, far], 1, '300000001')).toEqual(new Set([b, far]));
    expect(thinningKeepers([], 1, null).size).toBe(0);
  });

  it('never hides the selected ship', () => {
    const pairs: (readonly [string, ThinCandidate])[] = [
      ['300000001', ship(55.1, 4.1)],
      ['300000002', ship(55.2, 4.2, false)],
    ];
    expect([...thinnedOut(pairs, 1, '300000002')]).toEqual(['300000001']);
    expect([...thinnedOut([...pairs].reverse(), 1, '300000002')]).toEqual(['300000001']);
  });

  it('widens longitude cells towards the poles so cells stay roughly square', () => {
    // 2° of longitude apart: separate cells at the equator, one cell at 70° N.
    const equator = thinnedOut(
      [
        ['1', ship(0.2, 10.1)],
        ['2', ship(0.2, 12.1)],
      ],
      1,
      null,
    );
    const arctic = thinnedOut(
      [
        ['1', ship(70.2, 10.1)],
        ['2', ship(70.2, 10.9)],
      ],
      1,
      null,
    );
    expect(equator.size).toBe(0);
    expect(arctic.size).toBe(1);
  });
});
