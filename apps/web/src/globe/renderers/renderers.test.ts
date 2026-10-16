/**
 * Renderers against real Cesium collections (no GPU needed until a frame is drawn): keyed
 * diffing keeps primitives, symbology lands on them, stale feeds dim, labels are budgeted,
 * the satellite timer only runs while satellites are drawn, and the halo follows.
 */
import { Cartesian3, Color } from '@cesium/core';
import {
  type Billboard,
  type BillboardCollection,
  type PointPrimitive,
  type PointPrimitiveCollection,
  PrimitiveCollection,
} from '@cesium/engine';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type {
  Aircraft,
  Earthquake,
  FeedItems,
  FeedSnapshot,
  Freshness,
  SatelliteElements,
} from '../../contracts';
import { DEFAULT_PALETTE } from '../tokens';
import fixture from './__fixtures__/iss-omm.json';
import { AircraftRenderer } from './aircraft';
import { EarthquakeRenderer } from './earthquakes';
import { AIRCRAFT_GLYPH_PX, GLYPH_SUPERSAMPLE } from './glyphs';
import { pickWashGlyphId, quakeRingGlyphId } from './hazardGlyphs';
import { HAZARD_EDGE_PX } from './hazardShapes';
import { SatelliteRenderer } from './satellites';
import { SelectionHalo } from './selection';
import { STALE_ALPHA } from './symbology';
import { type RenderContext, toMapColors } from './types';

function context(heightM = 500_000) {
  const camera = { heightM };
  const requestRender = vi.fn();
  const ctx: RenderContext = {
    colors: toMapColors(DEFAULT_PALETTE),
    requestRender,
    cameraPosition: () => Cartesian3.fromDegrees(0, 51, camera.heightM),
    cameraHeightM: () => camera.heightM,
  };
  return { ctx, camera, requestRender };
}

function snapshot(items: FeedItems, freshness: Freshness = { state: 'live' }): FeedSnapshot {
  return {
    feed: items.kind,
    sources: ['test'],
    generatedAt: '2026-10-12T00:00:00Z',
    freshness,
    coverage: { kind: 'complete' },
    sourceCount: items.items.length,
    pendingRegions: 0,
    items,
    notices: [],
  };
}

const aircraft = (hex: string, overrides: Partial<Aircraft> = {}): Aircraft => ({
  hex,
  callsign: `CS${hex}`,
  position: { lat: 51, lon: 0 },
  altitudeBaroFt: 30_000,
  onGround: false,
  trackDeg: 90,
  isMilitary: false,
  seenPosS: 1,
  source: 'test',
  ...overrides,
});

const quake = (id: string, magnitude: number): Earthquake => ({
  id,
  magnitude,
  depthKm: 10,
  position: { lat: 35, lon: 139 },
  time: '2026-10-11T00:00:00Z',
  tsunami: false,
});

function billboardsOf(primitives: PrimitiveCollection): Billboard[] {
  const collection = primitives.get(0) as BillboardCollection;
  return Array.from({ length: collection.length }, (_, i) => collection.get(i));
}

/** The wash billboards of a renderer's pick assists (open centres). */
function washesOf(renderer: { readonly pickAssists?: unknown }): Billboard[] {
  const collection = renderer.pickAssists as BillboardCollection;
  return Array.from({ length: collection.length }, (_, i) => collection.get(i));
}

function pointsOf(primitives: PrimitiveCollection, index = 0): PointPrimitive[] {
  const collection = primitives.get(index) as PointPrimitiveCollection;
  return Array.from({ length: collection.length }, (_, i) => collection.get(i));
}

afterEach(() => {
  vi.useRealTimers();
});

describe('AircraftRenderer', () => {
  it('updates billboards in place by hex, adds new and removes missing ones', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, requestRender } = context();
    const renderer = new AircraftRenderer(primitives, ctx);
    renderer.apply(snapshot({ kind: 'aircraft', items: [aircraft('a1'), aircraft('a2')] }));
    const [first] = billboardsOf(primitives);
    expect(billboardsOf(primitives)).toHaveLength(2);
    expect(requestRender).toHaveBeenCalled();

    renderer.apply(
      snapshot({
        kind: 'aircraft',
        items: [aircraft('a1', { position: { lat: 52, lon: 1 }, trackDeg: 180 }), aircraft('a3')],
      }),
    );
    const after = billboardsOf(primitives);
    expect(after).toHaveLength(2);
    expect(after).toContain(first);
    expect(first?.id).toEqual({ feed: 'aircraft', id: 'a1' });
    expect(first?.rotation).toBeCloseTo(-Math.PI, 12);
    expect(renderer.locate('a1')?.position).toEqual(Cartesian3.fromDegrees(1, 52, 30_000 * 0.3048));
    expect(renderer.locate('a2')).toBeUndefined();
  });

  it('places aircraft at altitude, on-ground ones on the ellipsoid, rotated about local north', () => {
    const primitives = new PrimitiveCollection();
    const renderer = new AircraftRenderer(primitives, context().ctx);
    renderer.apply(
      snapshot({
        kind: 'aircraft',
        items: [aircraft('air'), aircraft('gnd', { onGround: true, position: { lat: 0, lon: 0 } })],
      }),
    );
    expect(renderer.locate('gnd')?.position).toEqual(Cartesian3.fromDegrees(0, 0, 0));
    const ground = billboardsOf(primitives).find((b) => b.id.id === 'gnd');
    // At the equator local north is the Earth's axis.
    expect(Cartesian3.equalsEpsilon(ground?.alignedAxis, Cartesian3.UNIT_Z, 1e-9)).toBe(true);
    expect(renderer.locate('air')?.symbolPx).toBe(AIRCRAFT_GLYPH_PX);
  });

  it('dims every symbol to 60 % when the snapshot is stale, and back when live', () => {
    const primitives = new PrimitiveCollection();
    const renderer = new AircraftRenderer(primitives, context().ctx);
    renderer.apply(
      snapshot({ kind: 'aircraft', items: [aircraft('a1')] }, { state: 'stale', ageS: 900 }),
    );
    const [billboard] = billboardsOf(primitives);
    expect(billboard?.color.alpha).toBeCloseTo(STALE_ALPHA, 9);
    expect(
      Color.equals(billboard?.color.withAlpha(1), Color.fromCssColorString(DEFAULT_PALETTE.air)),
    ).toBe(true);
    renderer.apply(snapshot({ kind: 'aircraft', items: [aircraft('a1')] }));
    expect(billboard?.color.alpha).toBe(1);
  });

  it('budgets labels by camera height and always labels the selection', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, camera } = context(500_000);
    const renderer = new AircraftRenderer(primitives, ctx);
    const many = Array.from({ length: 150 }, (_, i) =>
      aircraft(`h${i}`, { position: { lat: 51, lon: i * 0.01 } }),
    );
    renderer.apply(snapshot({ kind: 'aircraft', items: many }));
    expect(renderer.labelledIds()).toHaveLength(100);
    expect(renderer.labelledIds()).toContain('h0'); // nearest to the camera at lon 0

    renderer.setSelected('h149');
    expect(renderer.labelledIds()).toContain('h149');
    expect(renderer.labelledIds()).toHaveLength(101);

    camera.heightM = 5_000_000;
    renderer.viewChanged();
    expect(renderer.labelledIds()).toEqual(['h149']);

    renderer.setSelected(null);
    expect(renderer.labelledIds()).toEqual([]);
  });

  it('ignores items with unusable positions and other feeds', () => {
    const primitives = new PrimitiveCollection();
    const renderer = new AircraftRenderer(primitives, context().ctx);
    renderer.apply(
      snapshot({
        kind: 'aircraft',
        items: [aircraft('ok'), aircraft('bad', { position: { lat: Number.NaN, lon: 0 } })],
      }),
    );
    renderer.apply(snapshot({ kind: 'earthquakes', items: [quake('q', 5)] }));
    expect(renderer.size).toBe(1);
  });

  it('clears and destroys its collections', () => {
    const primitives = new PrimitiveCollection();
    const renderer = new AircraftRenderer(primitives, context().ctx);
    renderer.apply(snapshot({ kind: 'aircraft', items: [aircraft('a1')] }));
    renderer.clear();
    expect(renderer.size).toBe(0);
    expect(billboardsOf(primitives)).toHaveLength(0);
    renderer.destroy();
    expect(primitives.length).toBe(0);
  });
});

describe('EarthquakeRenderer', () => {
  it('draws batched rings sized and coloured by magnitude, inside the hazard edge', () => {
    const primitives = new PrimitiveCollection();
    const { ctx } = context();
    const renderer = new EarthquakeRenderer(primitives, ctx);
    renderer.apply(snapshot({ kind: 'earthquakes', items: [quake('small', 2), quake('big', 7)] }));
    expect(primitives.length).toBe(2);
    const [small, big] = billboardsOf(primitives);
    // Ring diameter 4 px + 2.5 px per unit of magnitude in whole pixels (M2 → 9, M7 → 22);
    // ramp step < 3 → 0, ≥ 6 → 4. The glyph carries its colours; the billboard is untinted.
    expect(small?.image).toBe(quakeRingGlyphId(9, 0));
    expect(big?.image).toBe(quakeRingGlyphId(22, 4));
    expect(Color.equals(big?.color, Color.WHITE)).toBe(true);
    expect(big?.scale).toBe(1 / GLYPH_SUPERSAMPLE);
    expect(big?.id).toEqual({ feed: 'earthquakes', id: 'big' });
    // Picking and the selection halo: the halo clears the ring and its edge.
    expect(renderer.locate('big')).toEqual({
      position: Cartesian3.fromDegrees(139, 35, 0),
      symbolPx: 22 + 2 * HAZARD_EDGE_PX,
    });
    // The open centres pick through one wash per ring over its whole glyph, kept apart from
    // the rings in the layer's pick assists so a wash never covers another drawn ring.
    expect(renderer.pickAssists).toBe(primitives.get(1));
    const [smallWash, bigWash] = washesOf(renderer);
    expect(smallWash?.id).toBe(small?.id);
    expect(bigWash?.id).toBe(big?.id);
    expect(smallWash?.image).toBe(pickWashGlyphId(9 + 2 * HAZARD_EDGE_PX));
    expect(bigWash?.image).toBe(pickWashGlyphId(22 + 2 * HAZARD_EDGE_PX));
    expect(bigWash?.position).toEqual(big?.position);
    expect(bigWash?.scale).toBe(1 / GLYPH_SUPERSAMPLE);
  });

  it('re-styles an updated magnitude in place and dims when stale', () => {
    const primitives = new PrimitiveCollection();
    const { ctx } = context();
    const renderer = new EarthquakeRenderer(primitives, ctx);
    renderer.apply(snapshot({ kind: 'earthquakes', items: [quake('q', 4.2)] }));
    const [ring] = billboardsOf(primitives);
    expect(ring?.image).toBe(quakeRingGlyphId(15, 2));
    renderer.apply(
      snapshot({ kind: 'earthquakes', items: [quake('q', 6.1)] }, { state: 'stale', ageS: 4000 }),
    );
    expect(billboardsOf(primitives)).toEqual([ring]);
    expect(ring?.image).toBe(quakeRingGlyphId(19, 4));
    expect(ring?.color.alpha).toBeCloseTo(STALE_ALPHA, 9);
    // The wash follows its ring in place: size and stale alpha.
    const [wash] = washesOf(renderer);
    expect(washesOf(renderer)).toHaveLength(1);
    expect(wash?.image).toBe(pickWashGlyphId(19 + 2 * HAZARD_EDGE_PX));
    expect(wash?.color.alpha).toBeCloseTo(STALE_ALPHA, 9);
    renderer.apply(snapshot({ kind: 'earthquakes', items: [quake('q', 6.1)] }));
    expect(ring?.color.alpha).toBe(1);
    expect(wash?.color.alpha).toBe(1);
  });

  it('skips unusable positions, clears and destroys its collection', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, requestRender } = context();
    const renderer = new EarthquakeRenderer(primitives, ctx);
    const bad = { ...quake('bad', 3), position: { lat: Number.NaN, lon: 0 } };
    renderer.apply(snapshot({ kind: 'earthquakes', items: [quake('ok', 3), bad] }));
    expect(renderer.size).toBe(1);
    expect(renderer.locate('bad')).toBeUndefined();
    expect(washesOf(renderer)).toHaveLength(1);
    renderer.clear();
    expect(billboardsOf(primitives)).toHaveLength(0);
    expect(washesOf(renderer)).toHaveLength(0);
    expect(requestRender).toHaveBeenCalled();
    renderer.destroy();
    expect(primitives.length).toBe(0);
  });
});

describe('SatelliteRenderer', () => {
  const iss = fixture.elements satisfies SatelliteElements;
  const at = new Date('2024-03-08T12:00:00Z');

  it('propagates on apply and on a ~1 s timer that stops when the feed is empty or cleared', () => {
    vi.useFakeTimers();
    const primitives = new PrimitiveCollection();
    const { ctx, requestRender } = context();
    let now = at;
    const onMoved = vi.fn();
    const renderer = new SatelliteRenderer(primitives, ctx, { now: () => now, onMoved });
    expect(renderer.ticking).toBe(false);

    renderer.apply(snapshot({ kind: 'satellites', items: [iss] }));
    expect(renderer.ticking).toBe(true);
    const first = renderer.locate('25544')?.position.clone();
    expect(first).toBeDefined();
    // ~6 795 km from the Earth's centre: ISS altitude.
    expect(Cartesian3.magnitude(first ?? Cartesian3.ZERO) / 1000).toBeGreaterThan(6_700);

    now = new Date(at.getTime() + 60_000);
    requestRender.mockClear();
    vi.advanceTimersByTime(1000);
    const moved = renderer.locate('25544')?.position;
    // One minute later the ISS has moved ~460 km.
    expect(
      Cartesian3.distance(first ?? Cartesian3.ZERO, moved ?? Cartesian3.ZERO) / 1000,
    ).toBeGreaterThan(400);
    expect(onMoved).toHaveBeenCalled();
    expect(requestRender).toHaveBeenCalled();

    renderer.apply(snapshot({ kind: 'satellites', items: [] }));
    expect(renderer.ticking).toBe(false);
    renderer.apply(snapshot({ kind: 'satellites', items: [iss] }));
    expect(renderer.ticking).toBe(true);
    renderer.clear();
    expect(renderer.ticking).toBe(false);
    requestRender.mockClear();
    vi.advanceTimersByTime(5000);
    expect(requestRender).not.toHaveBeenCalled(); // idle: nothing asks for frames
    renderer.destroy();
  });

  it('propagates large catalogues in slices without blocking', () => {
    vi.useFakeTimers();
    const primitives = new PrimitiveCollection();
    const { ctx } = context();
    const renderer = new SatelliteRenderer(primitives, ctx, { now: () => at, chunkSize: 2 });
    const items = Array.from({ length: 5 }, (_, i) => ({ ...iss, noradId: 90_000 + i }));
    renderer.apply(snapshot({ kind: 'satellites', items }));
    const shown = () => pointsOf(primitives).filter((p) => p.show).length;
    expect(shown()).toBe(2);
    vi.advanceTimersToNextTimer();
    expect(shown()).toBe(4);
    vi.advanceTimersToNextTimer();
    expect(shown()).toBe(5);
    renderer.destroy();
  });

  it('hides satellites whose elements cannot be propagated', () => {
    const primitives = new PrimitiveCollection();
    const renderer = new SatelliteRenderer(primitives, context().ctx, { now: () => at });
    renderer.apply(snapshot({ kind: 'satellites', items: [{ ...iss, eccentricity: 1.5 }] }));
    expect(renderer.locate('25544')).toBeUndefined();
    renderer.destroy();
  });

  it('draws the selected orbit in the selection colour and clears it', () => {
    const primitives = new PrimitiveCollection();
    const { ctx } = context();
    const renderer = new SatelliteRenderer(primitives, ctx, { now: () => at });
    const lines = primitives.get(1) as { length: number; get(i: number): { id: unknown } };
    renderer.showTrack({
      noradId: 25544,
      name: 'ISS',
      periodMin: 92.9,
      points: [
        { time: '2024-03-08T12:00:00Z', position: { lat: 0, lon: 0, altM: 420_000 } },
        { time: '2024-03-08T12:01:00Z', position: { lat: 3, lon: 3, altM: 420_000 } },
      ],
    });
    expect(lines.length).toBe(1);
    expect(lines.get(0).id).toEqual({ feed: 'satellites', id: '25544' });
    renderer.showTrack(null);
    expect(lines.length).toBe(0);
    renderer.destroy();
  });
});

describe('SelectionHalo', () => {
  it('follows the located entity and hides when it is gone or deselected', () => {
    const primitives = new PrimitiveCollection();
    const { ctx } = context();
    const halo = new SelectionHalo(primitives, ctx.colors);
    const position = Cartesian3.fromDegrees(10, 20, 1000);
    halo.follow({ position, symbolPx: 22 });
    expect(halo.visible).toBe(false); // nothing selected

    halo.set({ feed: 'aircraft', id: 'a1' });
    halo.follow({ position, symbolPx: 22 });
    expect(halo.visible).toBe(true);
    const billboard = billboardsOf(primitives)[0];
    expect(billboard?.position).toEqual(position);
    expect(billboard?.id).toEqual({ feed: 'aircraft', id: 'a1' });

    halo.follow(undefined);
    expect(halo.visible).toBe(false);
    halo.destroy();
    expect(primitives.length).toBe(0);
  });
});
