import { describe, expect, it } from 'vitest';
import type { FeedItems, FireCell } from '../contracts';
import { findItem, itemId } from './entities';
import { fireCellId, isValidFireCell } from './fireCells';

describe('the fire cell validity rule', () => {
  it('accepts cells with a usable position, detection count and FRP', () => {
    const valid = { lat: 45, lon: 10, count: 3, frp: 12.5, maxFrp: 6 };
    expect(isValidFireCell(valid)).toBe(true);
    for (const edge of [
      { lat: 90 },
      { lat: -90 },
      { lon: 180 },
      { lon: -180 },
      { frp: 0, maxFrp: 0 },
      { count: 1 },
      { count: 2 ** 31 },
      { frp: 1e300 },
    ]) {
      expect(isValidFireCell({ ...valid, ...edge }), JSON.stringify(edge)).toBe(true);
    }
  });

  it('rejects anything else from the wire instead of guessing', () => {
    const valid = { lat: 45, lon: 10, count: 3, frp: 12.5, maxFrp: 6 };
    const malformed: Record<string, unknown>[] = [
      { lat: Number.NaN },
      { lat: Number.POSITIVE_INFINITY },
      { lat: 90.0001 },
      { lat: -91 },
      { lat: null },
      { lat: '45' },
      { lat: undefined },
      { lon: Number.NaN },
      { lon: Number.NEGATIVE_INFINITY },
      { lon: 180.5 },
      { lon: -540 },
      { lon: null },
      { count: 0 },
      { count: -2 },
      { count: 1.5 },
      { count: Number.NaN },
      { count: Number.POSITIVE_INFINITY },
      { count: 2 ** 53 },
      { count: '3' },
      { count: null },
      { frp: Number.NaN },
      { frp: Number.POSITIVE_INFINITY },
      { frp: -0.5 },
      { frp: null },
      { frp: '12' },
      { maxFrp: Number.NaN },
      { maxFrp: -1 },
      { maxFrp: undefined },
    ];
    for (const fault of malformed) {
      const label = Object.entries(fault)
        .map(([k, v]) => `${k}=${String(v)}`)
        .join();
      expect(isValidFireCell({ ...valid, ...fault }), label).toBe(false);
    }
    for (const notACell of [null, undefined, 42, 'cell', [], {}]) {
      expect(isValidFireCell(notACell), String(notACell)).toBe(false);
    }
  });

  it('formats stable fire cell ids from lat/lon to 4 decimal places', () => {
    expect(fireCellId({ lat: 37.123456, lon: -122.987654 })).toBe('37.1235,-122.9877');
  });
});

describe('fire cell identity in snapshots', () => {
  const cell = (overrides: Record<string, unknown>): FireCell =>
    ({
      lat: 60.17,
      lon: 24.94,
      brightness: 320,
      frp: 80,
      confidence: 'nominal',
      satellite: 'N20',
      acquiredAt: '2026-10-15T12:00:00Z',
      dayNight: 'N',
      count: 1,
      maxFrp: 80,
      latestAcquiredAt: '2026-10-15T12:00:00Z',
      ...overrides,
    }) as FireCell;

  it('gives an invalid cell no id, so lookups skip it instead of throwing', () => {
    const good = cell({});
    const items: FeedItems = {
      kind: 'fires',
      items: [cell({ lat: null }), cell({ lon: Number.NaN }), cell({ count: 0 }), good],
    };
    expect(itemId(items, good)).toBe('60.1700,24.9400');
    expect(items.items.slice(0, 3).map((item) => itemId(items, item))).toEqual([null, null, null]);
    expect(findItem(items, '60.1700,24.9400')).toBe(good);
    expect(findItem(items, '0.0000,0.0000')).toBeUndefined();
  });
});
