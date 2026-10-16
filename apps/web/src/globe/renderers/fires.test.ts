import { Cartesian3, Color } from '@cesium/core';
import { type Billboard, BillboardCollection, PrimitiveCollection } from '@cesium/engine';
import { describe, expect, it, vi } from 'vitest';
import type { FeedSnapshot, FireCell, Freshness } from '../../contracts';
import { DEFAULT_PALETTE } from '../tokens';
import { FireRenderer, fireCellId, fireSizePx } from './fires';
import { GLYPH_SUPERSAMPLE } from './glyphs';
import { drawFireCell, fireCellGlyphId, fireCellGlyphPx, hazardInk } from './hazardGlyphs';
import { HAZARD_EDGE_PX } from './hazardShapes';
import { STALE_ALPHA } from './symbology';
import { type RenderContext, toMapColors } from './types';

function context() {
  const requestRender = vi.fn();
  const ctx: RenderContext = {
    colors: toMapColors(DEFAULT_PALETTE),
    requestRender,
    cameraPosition: () => Cartesian3.ZERO,
    cameraHeightM: () => 10_000,
  };
  return { ctx, requestRender };
}

function snapshot(items: FireCell[], freshness: Freshness = { state: 'live' }): FeedSnapshot {
  return {
    feed: 'fires',
    sources: ['nasa-firms'],
    generatedAt: '2026-10-15T12:00:00Z',
    items: { kind: 'fires', items },
    freshness,
    coverage: { kind: 'complete' },
    sourceCount: 1,
    pendingRegions: 0,
    notices: [],
  };
}

function cellsOf(collection: PrimitiveCollection): Billboard[] {
  for (let i = 0; i < collection.length; i++) {
    const candidate = collection.get(i);
    if (candidate instanceof BillboardCollection) {
      const billboards: Billboard[] = [];
      for (let j = 0; j < candidate.length; j++) billboards.push(candidate.get(j));
      return billboards;
    }
  }
  return [];
}

function cell(overrides: Partial<FireCell> = {}): FireCell {
  return {
    lat: 34.05,
    lon: -118.25,
    brightness: 320,
    frp: 25,
    confidence: 'nominal',
    satellite: 'N20',
    acquiredAt: '2026-10-15T12:00:00Z',
    dayNight: 'D',
    count: 1,
    maxFrp: 25,
    latestAcquiredAt: '2026-10-15T12:00:00Z',
    ...overrides,
  };
}

/** The whole-pixel side the renderer draws a cell at. */
const sideOf = (item: FireCell) => Math.round(fireSizePx(item.count, item.maxFrp));

describe('fire cell glyph', () => {
  it('is a square inside the hazard edge, one atlas entry per whole-pixel side and ramp step', () => {
    expect(fireCellGlyphId(9.4, 0)).toBe('lv-fire-cell-9-0');
    expect(fireCellGlyphId(9.6, 3)).toBe('lv-fire-cell-10-3');
    expect(fireCellGlyphPx(9)).toBe(9 + 2 * HAZARD_EDGE_PX);
    const ink = hazardInk(toMapColors(DEFAULT_PALETTE), toMapColors(DEFAULT_PALETTE).hazardRamp[0]);
    const canvas = drawFireCell(9, ink);
    expect(canvas.width).toBe((9 + 2 * HAZARD_EDGE_PX) * GLYPH_SUPERSAMPLE);
    expect(canvas.height).toBe(canvas.width);
  });
});

describe('FireRenderer', () => {
  it('batches cells as square billboards in the hazard ramp colour (fires are cells, quakes rings)', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, requestRender } = context();
    const renderer = new FireRenderer(primitives, ctx);
    const low = cell({ lat: 34.0, lon: -118.0, count: 1, frp: 5, maxFrp: 5 });
    const high = cell({ lat: 35.0, lon: -117.0, count: 1, frp: 300, maxFrp: 300 });

    renderer.apply(snapshot([low, high]));
    expect(renderer.size).toBe(2);
    expect(requestRender).toHaveBeenCalled();
    const [lowCell, highCell] = cellsOf(primitives);
    if (!lowCell || !highCell) throw new Error('Expected 2 cells');

    // Each glyph carries its ramp colour (step 0 for 5 MW, step 4 for 300 MW) and the light
    // keyline, so the billboard itself is not tinted.
    expect(lowCell.image).toBe(fireCellGlyphId(sideOf(low), 0));
    expect(highCell.image).toBe(fireCellGlyphId(sideOf(high), 4));
    expect(Color.equals(lowCell.color, Color.WHITE)).toBe(true);
    expect(lowCell.scale).toBe(1 / GLYPH_SUPERSAMPLE);
    expect(lowCell.id).toEqual({ feed: 'fires', id: fireCellId(low) });
    expect(highCell.id).toEqual({ feed: 'fires', id: fireCellId(high) });

    renderer.destroy();
  });

  it('sizes grouped cells (count > 1) above single detections with the same peak FRP', () => {
    const primitives = new PrimitiveCollection();
    const { ctx } = context();
    const renderer = new FireRenderer(primitives, ctx);
    const single = cell({ lat: 10, lon: 20, count: 1, frp: 40, maxFrp: 40 });
    const cluster = cell({ lat: 30, lon: 40, count: 25, frp: 200, maxFrp: 40 });

    renderer.apply(snapshot([single, cluster]));
    const [singleCell, clusterCell] = cellsOf(primitives);
    if (!singleCell || !clusterCell) throw new Error('Expected 2 cells');
    expect(sideOf(cluster)).toBeGreaterThan(sideOf(single));
    // Colour follows total FRP: step 1 for 40 MW, step 3 for 200 MW.
    expect(singleCell.image).toBe(fireCellGlyphId(sideOf(single), 1));
    expect(clusterCell.image).toBe(fireCellGlyphId(sideOf(cluster), 3));

    renderer.destroy();
  });

  it('updates existing cells in place and dims them when stale', () => {
    const primitives = new PrimitiveCollection();
    const { ctx } = context();
    const renderer = new FireRenderer(primitives, ctx);
    renderer.apply(snapshot([cell({ lat: 34.0, lon: -118.0, count: 1, frp: 20, maxFrp: 20 })]));
    const [before] = cellsOf(primitives);
    if (!before) throw new Error('Expected 1 cell');

    const grown = cell({ lat: 34.0, lon: -118.0, count: 10, frp: 260, maxFrp: 80 });
    renderer.apply(snapshot([grown], { state: 'stale', ageS: 3600 }));
    const after = cellsOf(primitives);
    expect(after).toHaveLength(1);
    expect(after[0]).toBe(before);
    expect(before.image).toBe(fireCellGlyphId(sideOf(grown), 4));
    expect(before.color.alpha).toBeCloseTo(STALE_ALPHA, 6);
    expect(Color.equals(before.color.withAlpha(1), Color.WHITE)).toBe(true);

    renderer.apply(snapshot([grown]));
    expect(before.color.alpha).toBe(1);

    renderer.destroy();
  });

  it('locates a cell for the selection halo, which must clear the square corners', () => {
    const primitives = new PrimitiveCollection();
    const { ctx } = context();
    const renderer = new FireRenderer(primitives, ctx);
    const item = cell({ lat: 34.1234, lon: -118.5678, count: 5, frp: 80, maxFrp: 50 });
    const id = fireCellId(item);
    renderer.apply(snapshot([item]));

    const located = renderer.locate(id);
    const glyphSide = sideOf(item) + 2 * HAZARD_EDGE_PX;
    expect(located?.position).toEqual(Cartesian3.fromDegrees(-118.5678, 34.1234, 0));
    // The halo is a ring of diameter symbolPx: it goes round the square's diagonal.
    expect(located?.symbolPx).toBeGreaterThanOrEqual(glyphSide * Math.SQRT2);
    expect(located?.symbolPx).toBeLessThan(glyphSide * Math.SQRT2 + 1);
    expect(renderer.locate('99.9999,99.9999')).toBeUndefined();
    renderer.setSelected(id);
    renderer.setSelected(null);

    renderer.destroy();
  });

  it('ignores invalid positions and snapshots from other feeds', () => {
    const primitives = new PrimitiveCollection();
    const { ctx } = context();
    const renderer = new FireRenderer(primitives, ctx);
    const valid = cell({ lat: 10, lon: 20 });
    renderer.apply(snapshot([valid, cell({ lat: 95, lon: 20 }), cell({ lat: Number.NaN })]));
    expect(renderer.size).toBe(1);

    renderer.apply({
      feed: 'earthquakes',
      sources: ['usgs'],
      generatedAt: '2026-10-15T12:00:00Z',
      items: { kind: 'earthquakes', items: [] },
      freshness: { state: 'live' },
      coverage: { kind: 'complete' },
      sourceCount: 0,
      pendingRegions: 0,
      notices: [],
    });
    expect(renderer.size).toBe(1);

    renderer.destroy();
  });

  it('skips malformed cells without throwing or dropping the rest of the batch', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, requestRender } = context();
    const renderer = new FireRenderer(primitives, ctx);
    const good = [
      cell({ lat: 10, lon: 20, frp: 5, maxFrp: 5 }),
      cell({ lat: -33.9, lon: 151.2, count: 40, frp: 400, maxFrp: 90 }),
      cell({ lat: 64.1, lon: -180, frp: 0, maxFrp: 0 }),
    ];
    // What a server bug or a newer contract could put on the wire (JSON has no NaN: null).
    const bad = [
      cell({ lat: 95 }),
      cell({ lat: Number.NaN, lon: 1 }),
      cell({ lon: 181 }),
      cell({ lat: 1, lon: Number.POSITIVE_INFINITY }),
      cell({ lat: 2, count: 0 }),
      cell({ lat: 3, count: -4 }),
      cell({ lat: 4, count: 2.5 }),
      cell({ lat: 5, frp: Number.NaN }),
      cell({ lat: 6, frp: -1 }),
      cell({ lat: 7, maxFrp: Number.POSITIVE_INFINITY }),
      { ...cell({ lat: 8 }), lat: null },
      { ...cell({ lat: 9 }), count: '3' },
      { ...cell({ lat: 11 }), frp: null },
      null,
      'cell',
      {},
    ] as unknown as FireCell[];

    expect(() => renderer.apply(snapshot([bad[0], ...good, ...bad] as FireCell[]))).not.toThrow();
    expect(renderer.size).toBe(good.length);
    expect(requestRender).toHaveBeenCalled();
    const cells = cellsOf(primitives);
    expect(cells).toHaveLength(good.length);
    for (const drawn of cells) {
      expect(drawn.image).toMatch(/^lv-fire-cell-\d+-[0-4]$/);
      expect(Number.isFinite(drawn.position.x)).toBe(true);
    }
    for (const item of good) expect(renderer.locate(fireCellId(item))).toBeDefined();

    renderer.destroy();
  });

  it('removes a drawn cell whose next update is malformed and keeps the others in place', () => {
    const primitives = new PrimitiveCollection();
    const { ctx } = context();
    const renderer = new FireRenderer(primitives, ctx);
    const kept = cell({ lat: 10, lon: 20 });
    const broken = cell({ lat: 30, lon: 40, frp: 60, maxFrp: 60 });
    renderer.apply(snapshot([kept, broken]));
    const [keptCell] = cellsOf(primitives);

    renderer.apply(snapshot([kept, { ...broken, frp: Number.NaN }]));
    expect(renderer.size).toBe(1);
    expect(renderer.locate(fireCellId(broken))).toBeUndefined();
    expect(cellsOf(primitives)[0]).toBe(keptCell);

    renderer.destroy();
  });

  it('clears and destroys its collection cleanly', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, requestRender } = context();
    const renderer = new FireRenderer(primitives, ctx);
    renderer.apply(snapshot([cell()]));
    expect(renderer.size).toBe(1);

    renderer.clear();
    expect(renderer.size).toBe(0);
    expect(cellsOf(primitives)).toHaveLength(0);
    expect(requestRender).toHaveBeenCalled();

    renderer.destroy();
    expect(primitives.length).toBe(0);
  });
});
