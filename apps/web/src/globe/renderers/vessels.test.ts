/**
 * VesselRenderer against real Cesium collections (no GPU needed until a frame is drawn):
 * keyed in-place updates by MMSI, chevron/dot symbology and rotation, stale dimming, label
 * budget, low-zoom thinning that never hides the selection, picking ids and the track trail.
 */
import { Cartesian3, Color } from '@cesium/core';
import {
  type Billboard,
  type BillboardCollection,
  type Label,
  type LabelCollection,
  type Polyline,
  type PolylineCollection,
  PrimitiveCollection,
} from '@cesium/engine';
import { describe, expect, it, vi } from 'vitest';
import type { FeedSnapshot, Freshness, Vessel, VesselTrackSample } from '../../contracts';
import { vessel } from '../../test/vessel';
import { DEFAULT_PALETTE } from '../tokens';
import { localNorth } from './aircraft';
import { STALE_ALPHA } from './symbology';
import { type RenderContext, toMapColors } from './types';
import { VESSEL_CHEVRON_PX, VESSEL_DOT_PX, vesselGlyphId } from './vesselGlyphs';
import { VESSEL_LABEL_BUDGET } from './vesselSymbology';
import { isDrawableVessel, VesselRenderer } from './vessels';

function context(heightM = 100_000) {
  const camera = { heightM, canvas: { width: 1440, height: 900 } };
  const requestRender = vi.fn();
  const ctx: RenderContext = {
    colors: toMapColors(DEFAULT_PALETTE),
    requestRender,
    cameraPosition: () => Cartesian3.fromDegrees(24.9, 60.1, camera.heightM),
    cameraHeightM: () => camera.heightM,
    canvasSizePx: () => camera.canvas,
  };
  return { ctx, camera, requestRender };
}

function snapshot(items: Vessel[], freshness: Freshness = { state: 'live' }): FeedSnapshot {
  return {
    feed: 'vessels',
    sources: ['aisstream'],
    generatedAt: '2026-10-15T12:00:00Z',
    freshness,
    coverage: { kind: 'complete' },
    sourceCount: items.length,
    pendingRegions: 0,
    items: { kind: 'vessels', items },
    notices: [],
  };
}

const underway = (mmsi: string, overrides: Partial<Vessel> = {}): Vessel =>
  vessel({ mmsi, name: `SHIP ${mmsi}`, speedKnots: 11.5, courseDeg: 90, ...overrides });

/** The renderer adds its trail lines, billboards and labels in that order. */
function collections(primitives: PrimitiveCollection) {
  const lines = primitives.get(0) as PolylineCollection;
  const billboards = primitives.get(1) as BillboardCollection;
  const labels = primitives.get(2) as LabelCollection;
  return {
    billboards: (): Billboard[] =>
      Array.from({ length: billboards.length }, (_, i) => billboards.get(i)),
    labels: (): Label[] => Array.from({ length: labels.length }, (_, i) => labels.get(i)),
    lines: (): Polyline[] => Array.from({ length: lines.length }, (_, i) => lines.get(i)),
  };
}

const byId = (billboards: Billboard[], id: string) =>
  billboards.find((b) => (b.id as { id: string }).id === id);

describe('VesselRenderer', () => {
  it('updates billboards in place by MMSI, adds new ships and removes missing ones', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, requestRender } = context();
    const renderer = new VesselRenderer(primitives, ctx);
    const drawn = collections(primitives);
    renderer.apply(snapshot([underway('230990001'), underway('230990002')]));
    const first = byId(drawn.billboards(), '230990001');
    expect(drawn.billboards()).toHaveLength(2);
    expect(requestRender).toHaveBeenCalled();

    renderer.apply(
      snapshot([
        underway('230990001', { position: { lat: 60.2, lon: 25 }, courseDeg: 180 }),
        underway('230990003'),
      ]),
    );
    const after = drawn.billboards();
    expect(after).toHaveLength(2);
    expect(after).toContain(first);
    expect(first?.id).toEqual({ feed: 'vessels', id: '230990001' });
    expect(first?.rotation).toBeCloseTo(-Math.PI, 12);
    expect(renderer.locate('230990001')?.position).toEqual(Cartesian3.fromDegrees(25, 60.2, 0));
    expect(renderer.locate('230990002')).toBeUndefined();
  });

  it('draws a chevron rotated to course over ground, a dot at rest, and switches in place', () => {
    const primitives = new PrimitiveCollection();
    const renderer = new VesselRenderer(primitives, context().ctx);
    const drawn = collections(primitives);
    renderer.apply(
      snapshot([
        underway('1', { courseDeg: 88, headingDeg: 89 }),
        vessel({ mmsi: '2', speedKnots: 4.2, headingDeg: 120 }),
        vessel({ mmsi: '3', speedKnots: 0, courseDeg: 0, headingDeg: 270 }),
        vessel({ mmsi: '4', speedKnots: 102.3, courseDeg: 360, headingDeg: 511 }),
      ]),
    );
    expect(renderer.glyphOf('1')).toBe('underway');
    expect(byId(drawn.billboards(), '1')?.rotation).toBeCloseTo((-88 * Math.PI) / 180, 12);
    expect(byId(drawn.billboards(), '2')?.rotation).toBeCloseTo((-120 * Math.PI) / 180, 12);
    expect(renderer.glyphOf('3')).toBe('at-rest');
    expect(renderer.glyphOf('4')).toBe('at-rest');
    expect(renderer.locate('1')?.symbolPx).toBe(VESSEL_CHEVRON_PX);
    expect(renderer.locate('3')?.symbolPx).toBe(VESSEL_DOT_PX);

    const dot = byId(drawn.billboards(), '3');
    renderer.apply(snapshot([underway('3', { courseDeg: 45 })]));
    expect(byId(drawn.billboards(), '3')).toBe(dot);
    expect(renderer.glyphOf('3')).toBe('underway');
    // Rotated about local north at the ship, on the ellipsoid.
    const north = dot?.alignedAxis ?? Cartesian3.ZERO;
    expect(Cartesian3.magnitude(north)).toBeCloseTo(1, 9);
  });

  it('turns chevrons from local north at the ship, refreshing that axis once it moves ~1 km', () => {
    const primitives = new PrimitiveCollection();
    const renderer = new VesselRenderer(primitives, context().ctx);
    const drawn = collections(primitives);
    const northAt = (lat: number, lon: number) =>
      localNorth(Cartesian3.fromDegrees(lon, lat, 0), new Cartesian3());
    renderer.apply(snapshot([underway('1', { position: { lat: 60, lon: 24 } })]));
    const billboard = byId(drawn.billboards(), '1');
    const axis = () => billboard?.alignedAxis ?? Cartesian3.ZERO;
    expect(Cartesian3.equalsEpsilon(axis(), northAt(60, 24), 1e-12)).toBe(true);

    // A 100 m move keeps the axis: it is off by far less than a pixel can show.
    renderer.apply(snapshot([underway('1', { position: { lat: 60.0009, lon: 24.0009 } })]));
    expect(Cartesian3.equalsEpsilon(axis(), northAt(60, 24), 1e-12)).toBe(true);
    expect(Cartesian3.angleBetween(axis(), northAt(60.0009, 24.0009))).toBeLessThan(1e-4);
    expect(billboard?.position).toEqual(Cartesian3.fromDegrees(24.0009, 60.0009, 0));

    renderer.apply(snapshot([underway('1', { position: { lat: 61, lon: 25 } })]));
    expect(Cartesian3.equalsEpsilon(axis(), northAt(61, 25), 1e-12)).toBe(true);
  });

  it('tints every glyph in the maritime colour and dims to 60 % when stale', () => {
    const primitives = new PrimitiveCollection();
    const { ctx } = context();
    const renderer = new VesselRenderer(primitives, ctx);
    const drawn = collections(primitives);
    renderer.apply(snapshot([underway('1')], { state: 'stale', ageS: 900 }));
    const [billboard] = drawn.billboards();
    expect(billboard?.color.alpha).toBeCloseTo(STALE_ALPHA, 9);
    expect(
      Color.equals(
        billboard?.color.withAlpha(1),
        Color.fromCssColorString(DEFAULT_PALETTE.maritime),
      ),
    ).toBe(true);
    renderer.apply(snapshot([underway('1')], { state: 'unavailable', reason: 'down' }));
    expect(billboard?.color.alpha).toBeCloseTo(STALE_ALPHA, 9);
    renderer.apply(snapshot([underway('1')]));
    expect(billboard?.color.alpha).toBe(1);
  });

  it('ignores ships it cannot place and other feeds', () => {
    const primitives = new PrimitiveCollection();
    const renderer = new VesselRenderer(primitives, context().ctx);
    const items = [
      underway('ok'),
      underway('nan', { position: { lat: Number.NaN, lon: 0 } }),
      underway('lat', { position: { lat: 91, lon: 0 } }),
      underway('lon', { position: { lat: 0, lon: 181 } }),
      { ...underway(''), mmsi: '' },
      { ...underway('null'), position: null } as unknown as Vessel,
    ];
    expect(items.filter(isDrawableVessel).map((v) => v.mmsi)).toEqual(['ok']);
    renderer.apply(snapshot(items));
    renderer.apply({
      ...snapshot([]),
      feed: 'earthquakes',
      items: { kind: 'earthquakes', items: [] },
    });
    expect(renderer.size).toBe(1);
  });

  it('budgets name labels at close zoom and always labels the selection', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, camera } = context(100_000);
    const renderer = new VesselRenderer(primitives, ctx);
    const many = Array.from({ length: 80 }, (_, i) =>
      underway(`2309${String(i).padStart(5, '0')}`, {
        position: { lat: 60.1, lon: 24.9 + i * 0.01 },
      }),
    );
    renderer.apply(snapshot(many));
    expect(renderer.labelledIds()).toHaveLength(VESSEL_LABEL_BUDGET);
    expect(renderer.labelledIds()).toContain('230900000'); // nearest the camera

    renderer.setSelected('230900079');
    expect(renderer.labelledIds()).toContain('230900079');
    expect(renderer.labelledIds()).toHaveLength(VESSEL_LABEL_BUDGET + 1);

    camera.heightM = 1_000_000;
    renderer.viewChanged();
    expect(renderer.labelledIds()).toEqual(['230900079']);
    expect(collections(primitives).labels()[0]?.text).toBe('SHIP 230900079');

    renderer.setSelected(null);
    expect(renderer.labelledIds()).toEqual([]);
  });

  it('thins crowded ships at low zoom, keeps the selection and restores all when zoomed in', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, camera, requestRender } = context(20_000_000);
    const renderer = new VesselRenderer(primitives, ctx);
    const drawn = collections(primitives);
    // Twenty ships within ~2 km of each other, one at rest, and one far away.
    const crowd = Array.from({ length: 20 }, (_, i) =>
      underway(`3000000${String(i).padStart(2, '0')}`, {
        position: { lat: 55 + i * 0.001, lon: 4 },
      }),
    );
    renderer.apply(
      snapshot([...crowd, underway('399999999', { position: { lat: -30, lon: 60 } })]),
    );
    expect(renderer.size).toBe(21);
    expect(renderer.drawn).toBe(2);
    expect(drawn.billboards().filter((b) => b.show)).toHaveLength(2);
    expect(byId(drawn.billboards(), '300000000')?.show).toBe(true);

    renderer.setSelected('300000013');
    expect(byId(drawn.billboards(), '300000013')?.show).toBe(true);
    expect(byId(drawn.billboards(), '300000000')?.show).toBe(false);
    expect(renderer.locate('300000013')).toBeDefined();
    expect(renderer.locate('300000000')).toBeUndefined();

    camera.heightM = 900_000;
    requestRender.mockClear();
    renderer.viewChanged();
    expect(renderer.drawn).toBe(21);
    expect(requestRender).toHaveBeenCalledTimes(1);
    requestRender.mockClear();
    renderer.viewChanged(); // nothing changed: no new frame
    expect(requestRender).not.toHaveBeenCalled();
  });

  it('keeps a hidden ship’s latest report and draws it when the ship is shown again', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, camera } = context(20_000_000);
    const renderer = new VesselRenderer(primitives, ctx);
    const drawn = collections(primitives);
    // Two ships in one thinning cell: the lower MMSI is drawn, the other is hidden.
    renderer.apply(
      snapshot([
        underway('300000000', { position: { lat: 55, lon: 4 } }),
        underway('300000001', { position: { lat: 55.001, lon: 4 } }),
      ]),
    );
    const hidden = byId(drawn.billboards(), '300000001');
    expect(hidden?.show).toBe(false);
    const before = Cartesian3.clone(hidden?.position ?? Cartesian3.ZERO);

    // While hidden it moves and comes to rest: its billboard is not rewritten (no buffer
    // writes for ships nobody sees), but the renderer remembers the report.
    renderer.apply(
      snapshot([
        underway('300000000', { position: { lat: 55, lon: 4 } }),
        underway('300000001', { position: { lat: 55.002, lon: 4.001 }, speedKnots: 0 }),
      ]),
    );
    expect(hidden?.position).toEqual(before);
    expect(renderer.glyphOf('300000001')).toBe('at-rest');

    camera.heightM = 900_000;
    renderer.viewChanged();
    expect(hidden?.show).toBe(true);
    expect(hidden?.position).toEqual(Cartesian3.fromDegrees(4.001, 55.002, 0));
    expect(hidden?.image).toBe(vesselGlyphId('at-rest'));
    expect(hidden?.rotation).toBe(0);
    expect(renderer.locate('300000001')?.position).toEqual(hidden?.position);
  });

  it('draws no new frame for a snapshot that changes nothing', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, requestRender } = context();
    const renderer = new VesselRenderer(primitives, ctx);
    const ships = [underway('1'), vessel({ mmsi: '2', speedKnots: 0 })];
    renderer.apply(snapshot(ships));
    requestRender.mockClear();

    // Fixture mode and quiet seas resend the same picture: nothing to draw.
    renderer.apply(snapshot(ships.map((s) => ({ ...s }))));
    expect(requestRender).not.toHaveBeenCalled();

    renderer.apply(snapshot([underway('1', { courseDeg: 91 }), ships[1] as Vessel]));
    expect(requestRender).toHaveBeenCalledTimes(1);
    requestRender.mockClear();
    renderer.apply(snapshot([underway('1', { courseDeg: 91 })]));
    expect(requestRender).toHaveBeenCalledTimes(1); // a ship left
    requestRender.mockClear();
    renderer.apply(snapshot([underway('1', { courseDeg: 91 })], { state: 'stale', ageS: 900 }));
    expect(requestRender).toHaveBeenCalledTimes(1); // dimmed
  });

  it('thins in screen space: a wider canvas keeps more of the same ships apart', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, camera } = context(20_000_000);
    const renderer = new VesselRenderer(primitives, ctx);
    // A row of ships 1° apart along the equator: ~2.8° cells at 1440 px draw one per ~3.
    const row = Array.from({ length: 30 }, (_, i) =>
      underway(`40000${String(i).padStart(4, '0')}`, { position: { lat: 0.5, lon: i } }),
    );
    renderer.apply(snapshot(row));
    const narrow = renderer.drawn;
    camera.canvas = { width: 2880, height: 1620 };
    renderer.viewChanged();
    expect(renderer.drawn).toBeGreaterThan(narrow);
    expect(renderer.size).toBe(30);
  });

  it('draws the selected track as a trail in the selection colour and clears it', () => {
    const primitives = new PrimitiveCollection();
    const { ctx, requestRender } = context();
    const renderer = new VesselRenderer(primitives, ctx);
    const drawn = collections(primitives);
    renderer.showTrack({
      mmsi: '230990001',
      source: 'aisstream',
      retainedSec: 1_800,
      samples: [
        { lat: 60.1, lon: 24.92, epochSec: 120 },
        { lat: 60.1, lon: 24.9, epochSec: 0 },
        { lat: Number.NaN, lon: 24.95, epochSec: 180 },
        { lat: 60.1, lon: 24.91, epochSec: 60 },
      ],
    });
    const [line] = drawn.lines();
    expect(drawn.lines()).toHaveLength(1);
    expect(line?.id).toEqual({ feed: 'vessels', id: '230990001' });
    expect(Color.equals(line?.material.uniforms.color, ctx.colors.selection)).toBe(true);
    // Oldest first, lifted just above the ellipsoid.
    const positions = line?.positions ?? [];
    expect(positions.length).toBeGreaterThanOrEqual(3);
    const west = Cartesian3.fromDegrees(24.9, 60.1, 50);
    expect(Cartesian3.distance(positions[0] ?? Cartesian3.ZERO, west)).toBeLessThan(1);

    requestRender.mockClear();
    renderer.showTrack(null);
    expect(drawn.lines()).toHaveLength(0);
    expect(requestRender).toHaveBeenCalledTimes(1);

    // A ship at anchor (one place) or a single sample has no trail to draw.
    renderer.showTrack({
      mmsi: '276990003',
      source: 'aisstream',
      retainedSec: 1_800,
      samples: [{ lat: 59.5, lon: 24.7, epochSec: 0 }],
    });
    expect(drawn.lines()).toHaveLength(0);
  });

  it('draws the trail around unusable samples and an impossible jump instead of dropping it', () => {
    const primitives = new PrimitiveCollection();
    const renderer = new VesselRenderer(primitives, context().ctx);
    const drawn = collections(primitives);
    const samples = [
      { lat: 60.1, lon: 24.9, epochSec: 0 },
      null,
      'not a sample',
      { lat: 60.1, lon: 24.91, epochSec: 60 },
      // A glitch on the far side of the Earth: no surface arc can join it to its neighbours.
      { lat: -60.1, lon: -155.09, epochSec: 120 },
      { lat: 60.1, lon: 24.93, epochSec: 180 },
      { lat: 60.1, lon: 24.94, epochSec: 240 },
    ] as unknown as VesselTrackSample[];
    expect(() =>
      renderer.showTrack({ mmsi: '230990001', source: 'aisstream', retainedSec: 1_800, samples }),
    ).not.toThrow();
    // The two good runs either side of the glitch are drawn; the glitch is not joined up.
    const lines = drawn.lines();
    expect(lines).toHaveLength(2);
    for (const line of lines) expect(line.id).toEqual({ feed: 'vessels', id: '230990001' });
    const start = Cartesian3.fromDegrees(24.9, 60.1, 50);
    expect(Cartesian3.distance(lines[0]?.positions[0] ?? Cartesian3.ZERO, start)).toBeLessThan(1);

    renderer.showTrack(null);
    expect(drawn.lines()).toHaveLength(0);
  });

  it('clears ships and the trail, and destroys its collections', () => {
    const primitives = new PrimitiveCollection();
    const renderer = new VesselRenderer(primitives, context().ctx);
    renderer.apply(snapshot([underway('1')]));
    renderer.showTrack({
      mmsi: '1',
      source: 'aisstream',
      retainedSec: 60,
      samples: [
        { lat: 60, lon: 24, epochSec: 0 },
        { lat: 60.01, lon: 24, epochSec: 60 },
      ],
    });
    renderer.clear();
    expect(renderer.size).toBe(0);
    expect(collections(primitives).billboards()).toHaveLength(0);
    expect(collections(primitives).lines()).toHaveLength(0);
    renderer.destroy();
    expect(primitives.length).toBe(0);
  });
});
