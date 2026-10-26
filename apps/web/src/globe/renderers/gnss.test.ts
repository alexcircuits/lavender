import { Cartesian3, type Color } from '@cesium/core';
import type { BillboardCollection, PrimitiveCollection } from '@cesium/engine';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DEFAULT_PALETTE } from '../tokens';
import { GLYPH_SUPERSAMPLE } from './glyphs';
import {
  GNSS_BADGE_PX,
  GNSS_SELECTED_BADGE_PX,
  type GnssCell,
  GnssRenderer,
  type GnssSnapshot,
  gnssBadgeLabel,
} from './gnss';
import { type RenderContext, toMapColors } from './types';

interface MockBillboard {
  position: Cartesian3;
  image: HTMLCanvasElement;
  color: Color;
  scale: number;
  id: unknown;
}

class MockBillboardCollection {
  readonly billboards: MockBillboard[] = [];
  add(options: MockBillboard): MockBillboard {
    const bb = { ...options };
    this.billboards.push(bb);
    return bb;
  }
  remove(bb: unknown): void {
    const idx = this.billboards.indexOf(bb as MockBillboard);
    if (idx !== -1) this.billboards.splice(idx, 1);
  }
  destroy(): void {
    this.billboards.length = 0;
  }
}

const SAMPLE_CELLS: GnssCell[] = [
  {
    id: '280:400',
    bbox: { west: 20, south: 50, east: 20.5, north: 50.5 },
    center: { lat: 50.25, lon: 20.25 },
    aircraft: 12,
    degraded: 1,
    percentDegraded: 0,
    level: 'low',
    state: 'nominal',
  },
  {
    id: '281:401',
    bbox: { west: 20.5, south: 50.5, east: 21, north: 51 },
    center: { lat: 50.75, lon: 20.75 },
    aircraft: 20,
    degraded: 3,
    percentDegraded: 10,
    level: 'medium',
    state: 'navigation-integrity-anomaly',
    interpretation: 'suspected-gnss-interference',
  },
  {
    id: '282:402',
    bbox: { west: 21, south: 51, east: 21.5, north: 51.5 },
    center: { lat: 51.25, lon: 21.25 },
    aircraft: 10,
    degraded: 5,
    percentDegraded: 40,
    level: 'high',
    state: 'navigation-integrity-anomaly',
    interpretation: 'suspected-gnss-interference',
  },
];

const SAMPLE_SNAPSHOT: GnssSnapshot = {
  asOf: '2026-10-26T10:00:00Z',
  windowMinutes: 30,
  cellDeg: 0.5,
  minAircraft: 3,
  totalCells: 3,
  anomalyCells: 2,
  cells: SAMPLE_CELLS,
};

describe('GnssRenderer', () => {
  let primitives: {
    add: <T>(c: T) => T;
    remove: (c: unknown) => unknown;
    collections: unknown[];
  };
  let ctx: RenderContext;
  let billboards: MockBillboardCollection;
  let renderer: GnssRenderer;

  beforeEach(() => {
    billboards = new MockBillboardCollection();
    const collections: unknown[] = [billboards];
    primitives = {
      collections,
      add: <T>(c: T) => {
        collections.push(c);
        return c;
      },
      remove: (c: unknown) => {
        const idx = collections.indexOf(c);
        if (idx !== -1) collections.splice(idx, 1);
        return true;
      },
    };

    ctx = {
      colors: toMapColors(DEFAULT_PALETTE),
      requestRender: vi.fn(),
      cameraPosition: () => Cartesian3.ZERO,
      cameraHeightM: () => 500_000,
      canvasSizePx: () => ({ width: 800, height: 600 }),
    };

    renderer = new GnssRenderer(
      primitives as unknown as PrimitiveCollection,
      ctx,
      billboards as unknown as BillboardCollection,
    );
  });

  it('formats badge labels as OK for low and rounded percentage for medium/high', () => {
    expect(gnssBadgeLabel({ level: 'low', percentDegraded: 0 })).toBe('OK');
    expect(gnssBadgeLabel({ level: 'medium', percentDegraded: 6.4 })).toBe('6%');
    expect(gnssBadgeLabel({ level: 'high', percentDegraded: 39.7 })).toBe('40%');
  });

  it('renders center badge billboards for active cells on setSnapshot and setCells', () => {
    renderer.setSnapshot(SAMPLE_SNAPSHOT);
    expect(billboards.billboards).toHaveLength(3);
    expect(ctx.requestRender).toHaveBeenCalled();

    expect(renderer.getCell('280:400')).toEqual(SAMPLE_CELLS[0]);
    expect(renderer.getCell('282:402')).toEqual(SAMPLE_CELLS[2]);
    expect(billboards.billboards[0]?.id).toEqual({ feed: 'gnss', id: '280:400' });

    const located = renderer.locate('281:401');
    expect(located).toBeDefined();
    expect(located?.symbolPx).toBe(GNSS_SELECTED_BADGE_PX);
  });

  it('updates selection styling and scale on setSelected', () => {
    renderer.setCells(SAMPLE_CELLS);
    const normalScale = 1 / GLYPH_SUPERSAMPLE;
    const selectedScale = GNSS_SELECTED_BADGE_PX / GNSS_BADGE_PX / GLYPH_SUPERSAMPLE;

    expect(billboards.billboards[1]?.scale).toBeCloseTo(normalScale, 5);

    renderer.setSelected('281:401');
    expect(billboards.billboards[1]?.scale).toBeCloseTo(selectedScale, 5);
    expect(billboards.billboards[1]?.color).toBe(ctx.colors.selection);
    expect(billboards.billboards[0]?.scale).toBeCloseTo(normalScale, 5);
  });

  it('clears all cell billboards on clear and destroy', () => {
    renderer.setCells(SAMPLE_CELLS);
    expect(billboards.billboards).toHaveLength(3);
    renderer.clear();
    expect(billboards.billboards).toHaveLength(0);
    expect(renderer.getCell('280:400')).toBeUndefined();

    renderer.setCells(SAMPLE_CELLS);
    renderer.destroy();
    expect(billboards.billboards).toHaveLength(0);
    expect(primitives.collections).not.toContain(billboards);
  });
});
