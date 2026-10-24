import {
  Cartesian3,
  type GeometryInstance,
  PolygonGeometry,
  PolygonHierarchy,
  PolylineGeometry,
} from '@cesium/core';
import { type GroundPrimitive, type Primitive, PrimitiveCollection } from '@cesium/engine';
import { describe, expect, it, vi } from 'vitest';
import type { FeedSnapshot, FirePerimeter } from '../../contracts';
import { entityFromPick } from '../entityRef';
import { DEFAULT_PALETTE } from '../tokens';
import { DEFAULT_PERIMETER_FILL_ALPHA, PerimeterRenderer } from './perimeters';
import { STALE_ALPHA } from './symbology';
import { type RenderContext, toMapColors } from './types';

function createPerimeter(
  id: string,
  overrides: Partial<FirePerimeter> & { rings?: number[][][] } = {},
): FirePerimeter {
  return {
    id,
    name: `Fire ${id}`,
    acres: 1250,
    containedPct: 45,
    anchor: { lon: -120.5, lat: 38.5 },
    extent: [-121, 38, -120, 39],
    detail: { level: 0, toleranceM: 0 },
    polygons: [
      [
        [
          [-121, 38],
          [-120, 38],
          [-120, 39],
          [-121, 39],
          [-121, 38],
        ],
      ],
    ],
    ...overrides,
  } as FirePerimeter;
}

function snapshot(items: FirePerimeter[], stale = false): FeedSnapshot {
  return {
    feed: 'perimeters',
    sources: ['nifc-wfigs'],
    generatedAt: '2026-10-21T00:00:00Z',
    items: { kind: 'perimeters', items },
    freshness: stale ? { state: 'stale', ageS: 3600 } : { state: 'live' },
    coverage: { kind: 'complete' },
    sourceCount: items.length,
    pendingRegions: 0,
    notices: [],
  };
}

function setup() {
  const root = new PrimitiveCollection();
  const requestRender = vi.fn();
  const colors = toMapColors(DEFAULT_PALETTE);
  const ctx: RenderContext = {
    colors,
    requestRender,
    cameraPosition: () => Cartesian3.ZERO,
    cameraHeightM: () => 500_000,
  };
  const renderer = new PerimeterRenderer(root, ctx);
  return { root, renderer, requestRender, colors };
}

function findBorder(root: PrimitiveCollection, id: string): Primitive {
  for (let i = 0; i < root.length; i++) {
    const p = root.get(i) as Primitive;
    const inst = p.geometryInstances as GeometryInstance | undefined;
    if (
      inst?.id &&
      (inst.id as { id?: string }).id === id &&
      inst.geometry instanceof PolylineGeometry
    ) {
      return p;
    }
  }
  throw new Error(`Border for ${id} not found in root`);
}

describe('PerimeterRenderer', () => {
  it('diffs snapshot items: adds new, updates in place, and removes missing perimeters', () => {
    const { root, renderer, requestRender } = setup();

    const fire1 = createPerimeter('cal-001');
    const fire2 = createPerimeter('cal-002', {
      anchor: { lon: -118.5, lat: 34.5 },
      polygons: [
        [
          [
            [-119, 34],
            [-118, 34],
            [-118, 35],
            [-119, 35],
            [-119, 34],
          ],
        ],
      ],
    });

    // 1. Initial snapshot with two perimeters
    renderer.apply(snapshot([fire1, fire2]));
    expect(renderer.size).toBe(2);
    expect(requestRender).toHaveBeenCalledTimes(1);

    // Each perimeter has 1 GroundPrimitive (fill) + 1 Primitive (border)
    expect(root.length).toBe(4);

    const initialFill1 = root.get(0) as GroundPrimitive;
    const initialBorder1 = root.get(1) as Primitive;

    // 2. Diffing update: keep fire1 with unchanged geometry, remove fire2, add fire3
    const fire3 = createPerimeter('cal-003');
    renderer.apply(snapshot([fire1, fire3]));
    expect(renderer.size).toBe(2);

    // Unchanged fire1 retains its primitives in place (no churn)
    expect(root.get(0)).toBe(initialFill1);
    expect(root.get(1)).toBe(initialBorder1);

    // Total count is still 4 (fire1: 2 primitives, fire3: 2 primitives; fire2 was removed)
    expect(root.length).toBe(4);

    // 3. Clear removes all primitives
    renderer.clear();
    expect(renderer.size).toBe(0);
    expect(root.length).toBe(0);

    // 4. Destroy cleans up everything
    renderer.destroy();
    expect(renderer.size).toBe(0);
    expect(root.length).toBe(0);
  });

  it('renders polygon hierarchy with holes, creating ground fills and border polylines', () => {
    const { root, renderer } = setup();

    // Polygon with exterior and an interior hole (island)
    const fireWithHole = createPerimeter('hole-fire', {
      polygons: [
        [
          // Exterior ring
          [
            [-121, 38],
            [-119, 38],
            [-119, 40],
            [-121, 40],
            [-121, 38],
          ],
          // Hole ring
          [
            [-120.6, 38.6],
            [-119.4, 38.6],
            [-119.4, 39.4],
            [-120.6, 39.4],
            [-120.6, 38.6],
          ],
        ],
      ],
    });

    renderer.apply(snapshot([fireWithHole]));
    expect(renderer.size).toBe(1);

    // 1 GroundPrimitive fill + 2 border primitives (exterior + hole) = 3 primitives
    expect(root.length).toBe(3);

    const fill = root.get(0) as GroundPrimitive;
    const fillInstances = fill.geometryInstances as GeometryInstance;
    const geometry = fillInstances.geometry;
    if (!(geometry instanceof PolygonGeometry)) throw new Error('Expected PolygonGeometry');

    const exteriorOnly = new PolygonGeometry({
      polygonHierarchy: new PolygonHierarchy(
        [
          [-121, 38],
          [-119, 38],
          [-119, 40],
          [-121, 40],
          [-121, 38],
        ].map(([lon, lat]) => Cartesian3.fromDegrees(lon ?? 0, lat ?? 0)),
      ),
    });

    // The geometry with holes has greater packedLength than exterior-only geometry
    expect(geometry.packedLength).toBeGreaterThan(exteriorOnly.packedLength);

    // Inspect the border primitives
    const outerBorder = root.get(1) as Primitive;
    const holeBorder = root.get(2) as Primitive;

    const outerBorderInst = outerBorder.geometryInstances as GeometryInstance;
    const holeBorderInst = holeBorder.geometryInstances as GeometryInstance;

    if (!(outerBorderInst.geometry instanceof PolylineGeometry)) {
      throw new Error('Expected PolylineGeometry for outer border');
    }
    if (!(holeBorderInst.geometry instanceof PolylineGeometry)) {
      throw new Error('Expected PolylineGeometry for hole border');
    }

    expect(outerBorderInst.geometry.packedLength).toBe(30); // 5 points
    expect(holeBorderInst.geometry.packedLength).toBe(30); // 5 points
  });

  it('supports single-polygon rings structure (outer ring + holes)', () => {
    const { root, renderer } = setup();

    const legacyFormat = {
      id: 'legacy-ring-fire',
      name: 'Legacy Ring Fire',
      rings: [
        [
          [-121, 38],
          [-119, 38],
          [-119, 40],
          [-121, 40],
          [-121, 38],
        ],
        [
          [-120.5, 38.5],
          [-119.5, 38.5],
          [-119.5, 39.5],
          [-120.5, 39.5],
          [-120.5, 38.5],
        ],
      ],
    } as unknown as FirePerimeter;

    renderer.apply(snapshot([legacyFormat]));
    expect(renderer.size).toBe(1);
    expect(root.length).toBe(3); // 1 fill + 2 borders

    const fill = root.get(0) as GroundPrimitive;
    expect((fill.geometryInstances as GeometryInstance).geometry).toBeInstanceOf(PolygonGeometry);
  });

  it('highlights borders on selection and reverts on deselection', () => {
    const { root, renderer, colors, requestRender } = setup();

    const fire1 = createPerimeter('fire-sel-1');
    const fire2 = createPerimeter('fire-sel-2', {
      polygons: [
        [
          [
            [-118, 33],
            [-117, 33],
            [-117, 34],
            [-118, 34],
            [-118, 33],
          ],
        ],
      ],
    });

    renderer.apply(snapshot([fire1, fire2]));

    // Before selection: both have default crisp fire border
    const border1Before = findBorder(root, 'fire-sel-1');
    const border2Before = findBorder(root, 'fire-sel-2');

    const b1AttrsBefore = (border1Before.geometryInstances as GeometryInstance).attributes;
    const b2AttrsBefore = (border2Before.geometryInstances as GeometryInstance).attributes;

    const defaultBorderBytes = Array.from(b1AttrsBefore.color.value as Uint8Array);
    expect(Array.from(b2AttrsBefore.color.value as Uint8Array)).toEqual(defaultBorderBytes);

    // 1. Select fire1: its border switches to the selection colour
    requestRender.mockClear();
    renderer.setSelected('fire-sel-1');
    expect(requestRender).toHaveBeenCalledTimes(1);

    const border1Selected = findBorder(root, 'fire-sel-1');
    const b1AttrsSelected = (border1Selected.geometryInstances as GeometryInstance).attributes;
    const selBytes = Array.from(b1AttrsSelected.color.value as Uint8Array);

    expect(selBytes).not.toEqual(defaultBorderBytes);
    // Matches selection color
    expect(selBytes[0]).toBe(Math.round(colors.selection.red * 255));
    expect(selBytes[1]).toBe(Math.round(colors.selection.green * 255));
    expect(selBytes[2]).toBe(Math.round(colors.selection.blue * 255));

    // fire2 remains unselected with default border
    const border2StillUnselected = findBorder(root, 'fire-sel-2');
    const b2AttrsStillUnselected = (border2StillUnselected.geometryInstances as GeometryInstance)
      .attributes;
    expect(Array.from(b2AttrsStillUnselected.color.value as Uint8Array)).toEqual(
      defaultBorderBytes,
    );

    // 2. Select fire2: fire1 reverts, fire2 highlights
    renderer.setSelected('fire-sel-2');
    const border1Reverted = findBorder(root, 'fire-sel-1');
    const border2Selected = findBorder(root, 'fire-sel-2');

    expect(
      Array.from(
        (border1Reverted.geometryInstances as GeometryInstance).attributes.color
          .value as Uint8Array,
      ),
    ).toEqual(defaultBorderBytes);
    expect(
      Array.from(
        (border2Selected.geometryInstances as GeometryInstance).attributes.color
          .value as Uint8Array,
      ),
    ).toEqual(selBytes);

    // 3. Clear selection (null)
    renderer.setSelected(null);
    const border2Reverted = findBorder(root, 'fire-sel-2');
    expect(
      Array.from(
        (border2Reverted.geometryInstances as GeometryInstance).attributes.color
          .value as Uint8Array,
      ),
    ).toEqual(defaultBorderBytes);
  });

  it('attaches EntityRef to primitives and supports picking', () => {
    const { root, renderer } = setup();

    const fire = createPerimeter('wfigs-fire-42');
    renderer.apply(snapshot([fire]));

    const fill = root.get(0) as GroundPrimitive;
    const border = root.get(1) as Primitive;

    const expectedRef = { feed: 'perimeters', id: 'wfigs-fire-42' };

    expect((fill.geometryInstances as GeometryInstance).id).toEqual(expectedRef);
    expect((border.geometryInstances as GeometryInstance).id).toEqual(expectedRef);

    // Scene pick resolution via entityFromPick
    expect(
      entityFromPick({ primitive: fill, id: (fill.geometryInstances as GeometryInstance).id }),
    ).toEqual(expectedRef);
    expect(
      entityFromPick({ primitive: border, id: (border.geometryInstances as GeometryInstance).id }),
    ).toEqual(expectedRef);
  });

  it('locates perimeter by anchor or computes anchor when not provided', () => {
    const { renderer } = setup();

    const fireExplicitAnchor = createPerimeter('fire-anchor', {
      anchor: { lon: -120.25, lat: 38.75 },
    });
    const fireComputedAnchor = createPerimeter('fire-computed', {
      anchor: undefined as unknown as FirePerimeter['anchor'],
      polygons: [
        [
          [
            [-122, 37],
            [-120, 37],
            [-120, 39],
            [-122, 39],
            [-122, 37],
          ],
        ],
      ],
    });

    renderer.apply(snapshot([fireExplicitAnchor, fireComputedAnchor]));

    // 1. Explicit anchor located
    const locExplicit = renderer.locate('fire-anchor');
    expect(locExplicit).toBeDefined();
    expect(locExplicit?.symbolPx).toBe(32);
    expect(locExplicit?.position).toEqual(Cartesian3.fromDegrees(-120.25, 38.75));

    // 2. Computed anchor located at polygon centroid (-121, 38)
    const locComputed = renderer.locate('fire-computed');
    expect(locComputed).toBeDefined();
    expect(locComputed?.symbolPx).toBe(32);
    expect(locComputed?.position).toEqual(Cartesian3.fromDegrees(-121, 38));

    // 3. Unknown id returns undefined
    expect(renderer.locate('non-existent')).toBeUndefined();
  });

  it('dims fills and borders when snapshot freshness is stale', () => {
    const { root, renderer } = setup();

    const fire = createPerimeter('dim-fire');
    renderer.apply(snapshot([fire], false)); // live

    const liveFill = root.get(0) as GroundPrimitive;
    const liveFillAttrs = (liveFill.geometryInstances as GeometryInstance).attributes;
    const liveAlpha = (liveFillAttrs.color.value as Uint8Array)[3];
    expect(liveAlpha).toBe(Math.round(DEFAULT_PERIMETER_FILL_ALPHA * 255));

    // Stale snapshot dims to STALE_ALPHA (0.6)
    renderer.apply(snapshot([fire], true));

    const staleFill = root.get(0) as GroundPrimitive;
    const staleFillAttrs = (staleFill.geometryInstances as GeometryInstance).attributes;
    const staleAlpha = (staleFillAttrs.color.value as Uint8Array)[3];
    expect(staleAlpha).toBe(Math.round(DEFAULT_PERIMETER_FILL_ALPHA * STALE_ALPHA * 255));

    // Return to live restores alpha
    renderer.apply(snapshot([fire], false));
    const restoredFill = root.get(0) as GroundPrimitive;
    const restoredFillAttrs = (restoredFill.geometryInstances as GeometryInstance).attributes;
    expect((restoredFillAttrs.color.value as Uint8Array)[3]).toBe(liveAlpha);
  });

  it('safely skips invalid or degenerate rings and non-perimeter feeds', () => {
    const { root, renderer } = setup();

    const badPerimeters = [
      { id: '' }, // empty id
      {
        id: 'bad-rings',
        polygons: [
          [
            // < 3 points
            [
              [0, 0],
              [1, 1],
            ],
          ],
        ],
      },
    ] as unknown as FirePerimeter[];

    renderer.apply(snapshot(badPerimeters));
    expect(renderer.size).toBe(0);
    expect(root.length).toBe(0);

    // Applying non-perimeters feed does nothing
    renderer.apply({
      feed: 'aircraft',
      sources: ['test'],
      generatedAt: '2026-10-21T00:00:00Z',
      items: { kind: 'aircraft', items: [] },
      freshness: { state: 'live' },
      coverage: { kind: 'complete' },
      sourceCount: 0,
      pendingRegions: 0,
      notices: [],
    });
    expect(renderer.size).toBe(0);
  });
});
