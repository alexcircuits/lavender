import { Cartesian3, type GeometryInstance, PolygonGeometry, PolygonHierarchy } from '@cesium/core';
import {
  type BillboardCollection,
  type GroundPrimitive,
  type PolylineCollection,
  PrimitiveCollection,
} from '@cesium/engine';
import { describe, expect, it, vi } from 'vitest';
import type { Cyclone, FeedSnapshot } from '../../contracts';
import { cyclone } from '../../test/cyclone';
import { DEFAULT_PALETTE } from '../tokens';
import { CycloneRenderer, cycloneRampIndex } from './cyclones';
import { cycloneGlyphId, cycloneGlyphPx, pickWashGlyphId } from './hazardGlyphs';
import { cycloneSpiralPx } from './hazardShapes';
import { keylineLineWidth } from './keylineLine';
import { toMapColors } from './types';

function setup() {
  const root = new PrimitiveCollection();
  const renderer = new CycloneRenderer(root, {
    colors: toMapColors(DEFAULT_PALETTE),
    requestRender: vi.fn(),
    cameraPosition: () => Cartesian3.ZERO,
    cameraHeightM: () => 1e6,
  });
  return {
    root,
    renderer,
    points: root.get(0) as BillboardCollection,
    lines: root.get(1) as PolylineCollection,
    washes: root.get(2) as BillboardCollection,
  };
}
function snapshot(storms: Cyclone[], stale = false): FeedSnapshot {
  return {
    feed: 'cyclones',
    sources: ['noaa-nhc-cphc'],
    generatedAt: '2026-10-14T00:00:00Z',
    items: { kind: 'cyclones', items: storms },
    freshness: stale ? { state: 'stale', ageS: 3600 } : { state: 'live' },
    coverage: { kind: 'complete' },
    sourceCount: storms.length,
    pendingRegions: 0,
    notices: [],
  };
}
describe('cyclone geometry', () => {
  it('keeps points and unchanged advisory primitives, with all parts and holes', () => {
    const { root, renderer, points, lines } = setup();
    renderer.apply(snapshot([cyclone()]));
    const point = points.get(0);
    const firstLine = lines.get(0);
    expect(lines.length).toBe(5);
    expect(root.length).toBe(5);
    const fill = root.get(3) as GroundPrimitive;
    const instances = fill.geometryInstances;
    expect(Array.isArray(instances)).toBe(false);
    const geometry = (instances as GeometryInstance).geometry;
    if (!(geometry instanceof PolygonGeometry)) throw new Error('Expected polygon geometry');
    const exteriorOnly = new PolygonGeometry({
      polygonHierarchy: new PolygonHierarchy(
        [
          [-62, 19],
          [-59, 19],
          [-59, 22],
          [-62, 22],
          [-62, 19],
        ].map(([lon, lat]) => Cartesian3.fromDegrees(lon ?? 0, lat ?? 0, 50)),
      ),
    });
    expect(geometry.packedLength).toBeGreaterThan(exteriorOnly.packedLength);
    renderer.apply(snapshot([cyclone({ position: { lat: 21, lon: -61 } })]));
    expect(points.get(0)).toBe(point);
    expect(lines.get(0)).toBe(firstLine);
    expect(point.id).toEqual({ feed: 'cyclones', id: 'al062026' });
    expect(renderer.locate('al062026')?.position).toBe(point.position);
    renderer.clear();
    expect(points.length).toBe(0);
    expect(lines.length).toBe(0);
    expect(root.length).toBe(3);
    renderer.destroy();
    expect(root.length).toBe(0);
  });
  it('never draws pending or unavailable forecast geometry, and dims stale data', () => {
    const { renderer, points, lines, washes } = setup();
    renderer.apply(snapshot([cyclone()]));
    renderer.apply(snapshot([cyclone({ geometryStatus: 'pending' })], true));
    expect(lines.length).toBe(0);
    expect(points.get(0).color.alpha).toBe(0.6);
    expect(washes.get(0).color.alpha).toBe(0.6);
    renderer.apply(snapshot([cyclone({ geometryStatus: 'unavailable' })]));
    expect(lines.length).toBe(0);
    expect(points.get(0).color.alpha).toBe(1);
    expect(washes.get(0).color.alpha).toBe(1);
    renderer.apply(snapshot([]));
    expect(renderer.size).toBe(0);
    expect(washes.length).toBe(0);
    renderer.destroy();
  });
  it('maps the legend boundaries to the actual five colour steps', () => {
    expect([undefined, 33, 34, 63, 64, 95, 96, 136, 137].map(cycloneRampIndex)).toEqual([
      null,
      0,
      1,
      1,
      2,
      2,
      3,
      3,
      4,
    ]);
  });
  it('retains keyed positions and other storms when an arc is unsupported', () => {
    const { renderer, points, lines } = setup();
    const unsupported = cyclone({
      id: 'bad',
      track: {
        type: 'LineString',
        coordinates: [
          [0, 0],
          [179.5, 0],
        ],
      },
    });
    delete unsupported.cone;
    renderer.apply(snapshot([unsupported, cyclone()]));
    expect(points.length).toBe(2);
    const first = points.get(0);
    expect(renderer.size).toBe(2);
    expect(lines.length).toBe(5);
    renderer.apply(snapshot([unsupported, cyclone()]));
    expect(points.length).toBe(2);
    expect(points.get(0)).toBe(first);
    expect(lines.length).toBe(5);
    renderer.destroy();
  });
  it('uses a neutral spiral when sustained wind was not reported', () => {
    const { renderer, points } = setup();
    const unknown = cyclone();
    delete unknown.intensityKt;
    renderer.apply(snapshot([unknown]));
    expect(points.get(0).image).toBe(cycloneGlyphId(cycloneSpiralPx(null), 'unknown', 'north'));
    renderer.destroy();
  });
  it('marks the position with a spiral sized and coloured by wind, turning with its hemisphere', () => {
    const { renderer, points, lines, washes } = setup();
    // The synthetic storm: 70 kn (ramp step 2) at 20° N.
    renderer.apply(snapshot([cyclone()]));
    const spiral = points.get(0);
    expect(spiral.image).toBe(cycloneGlyphId(cycloneSpiralPx(2), 2, 'north'));
    expect(spiral.color.alpha).toBe(1);
    expect(renderer.locate('al062026')?.symbolPx).toBe(cycloneGlyphPx(cycloneSpiralPx(2)));
    // Between the arms the storm picks through its wash, kept in the layer's pick assists.
    expect(renderer.pickAssists).toBe(washes);
    const wash = washes.get(0);
    expect(wash.id).toBe(spiral.id);
    expect(wash.position).toEqual(spiral.position);
    expect(wash.eyeOffset).toEqual(spiral.eyeOffset);
    expect(wash.image).toBe(pickWashGlyphId(cycloneGlyphPx(cycloneSpiralPx(2))));
    // Track (2 px core) and cone outlines (1 px core) carry the hazard edge on both sides.
    const widths = new Set(Array.from({ length: lines.length }, (_, i) => lines.get(i).width));
    expect(widths).toEqual(new Set([keylineLineWidth(2), keylineLineWidth(1)]));
    expect(lines.get(0).material.type).toBe('LavenderHazardKeylineLine');

    // A southern storm at 140 kn: the mirrored spiral, the top step, the same billboard.
    const southern = cyclone({ intensityKt: 140, position: { lat: -15, lon: 120 } });
    renderer.apply(snapshot([southern]));
    expect(points.get(0)).toBe(spiral);
    expect(spiral.image).toBe(cycloneGlyphId(cycloneSpiralPx(4), 4, 'south'));
    expect(washes.get(0)).toBe(wash);
    expect(wash.image).toBe(pickWashGlyphId(cycloneGlyphPx(cycloneSpiralPx(4))));
    expect(wash.position).toEqual(spiral.position);
    renderer.destroy();
  });
});
