/**
 * Keyed storm positions and advisory-matched forecast geometry; no frame-time updates. The
 * position is a spiral glyph (design-system §4: cyclone = spiral + track) in the wind step's
 * ramp colour, or the neutral unknown-wind colour; track and cone outlines carry the same
 * two-tone hazard edge as the point symbols. A near-invisible wash per spiral, the layer's pick
 * assists, selects the storm between its arms when nothing drawn is under the cursor.
 */
import {
  Cartesian3,
  Color,
  ColorGeometryInstanceAttribute,
  GeometryInstance,
  PolygonGeometry,
  PolygonHierarchy,
} from '@cesium/core';
import {
  type Billboard,
  BillboardCollection,
  GroundPrimitive,
  PerInstanceColorAppearance,
  type Polyline,
  PolylineCollection,
  type PrimitiveCollection,
} from '@cesium/engine';
import type { Cyclone, FeedSnapshot } from '../../contracts';
import type { EntityRef } from '../index';
import { GLYPH_SUPERSAMPLE } from './glyphs';
import {
  cycloneGlyphId,
  cycloneGlyphPx,
  drawCycloneSpiral,
  drawPickWash,
  GlyphCache,
  hazardInk,
  pickWashGlyphId,
} from './hazardGlyphs';
import { cycloneSpiralPx, hemisphereOf, type RampStep } from './hazardShapes';
import { KeyedEntries } from './keyed';
import { keylineLineMaterial, keylineLineWidth } from './keylineLine';
import { type ArcBudget, surfaceArcPositions } from './surfaceArc';
import { hasValidPosition, isStale, STALE_ALPHA } from './symbology';
import type { FeedRenderer, Located, RenderContext } from './types';

type Coordinate = readonly [number, number];

type RampRank = RampStep | null;

/** Sustained wind in knots: depression, storm, category1/2, category3/4, category5. */
export function cycloneRampIndex(wind: number | undefined): RampRank {
  if (wind === undefined || !Number.isFinite(wind)) return null;
  if (wind < 34) return 0;
  if (wind < 64) return 1;
  if (wind < 96) return 2;
  return wind < 137 ? 3 : 4;
}

function position([lon, lat]: Coordinate): Cartesian3 {
  return Cartesian3.fromDegrees(lon, lat, 50);
}

/** Track and cone-outline cores; the hazard edge is added on both sides. */
const TRACK_CORE_PX = 2;
const CONE_CORE_PX = 1;
/** Draws the spiral in front of its own track, which starts at the storm's position. */
const SPIRAL_EYE_OFFSET = new Cartesian3(0, 0, -1000);

interface Entry {
  readonly billboard: Billboard;
  /** The hit area between the spiral's arms (pick assists). */
  readonly wash: Billboard;
  /** The spiral's on-screen side (glyph and hazard edge), for the selection halo. */
  glyphPx: number;
  glyph: string;
  item: Cyclone;
  signature: string;
  readonly lines: Polyline[];
  readonly fills: GroundPrimitive[];
  expandedVertices: number;
}

export class CycloneRenderer implements FeedRenderer {
  readonly feed = 'cyclones' as const;
  readonly #root: PrimitiveCollection;
  readonly #ctx: RenderContext;
  readonly #billboards: BillboardCollection;
  readonly #lines: PolylineCollection;
  readonly #washes: BillboardCollection;
  readonly #entries: KeyedEntries<Cyclone, Entry>;
  readonly #glyphs = new GlyphCache();
  #alpha = 1;
  #budget: ArcBudget = { remaining: 100_000 };

  constructor(root: PrimitiveCollection, ctx: RenderContext) {
    this.#root = root;
    this.#ctx = ctx;
    this.#billboards = root.add(new BillboardCollection());
    this.#lines = root.add(new PolylineCollection());
    this.#washes = root.add(new BillboardCollection());
    this.#entries = new KeyedEntries((storm) => storm.id, {
      create: (storm, id) => {
        const ref: EntityRef = { feed: 'cyclones', id };
        const options = {
          id: ref,
          position: position([storm.position.lon, storm.position.lat]),
          scale: 1 / GLYPH_SUPERSAMPLE,
          eyeOffset: SPIRAL_EYE_OFFSET,
        };
        const entry: Entry = {
          billboard: this.#billboards.add(options),
          wash: this.#washes.add(options),
          glyphPx: 0,
          glyph: '',
          item: storm,
          signature: '',
          lines: [],
          fills: [],
          expandedVertices: 0,
        };
        this.#update(entry, storm);
        return entry;
      },
      update: (entry, storm) => this.#update(entry, storm),
      remove: (entry) => {
        this.#clearGeometry(entry);
        this.#billboards.remove(entry.billboard);
        this.#washes.remove(entry.wash);
      },
    });
  }

  get size(): number {
    return this.#entries.size;
  }

  /** The between-the-arms washes (types.ts `PickAssist`). */
  get pickAssists(): BillboardCollection {
    return this.#washes;
  }

  apply(snapshot: FeedSnapshot): void {
    if (snapshot.items.kind !== 'cyclones') return;
    this.#alpha = isStale(snapshot.freshness) ? STALE_ALPHA : 1;
    this.#budget = { remaining: 100_000 };
    this.#entries.apply(snapshot.items.items.filter(hasValidPosition).slice(0, 32));
    this.#ctx.requestRender();
  }

  #update(entry: Entry, storm: Cyclone): void {
    entry.item = storm;
    const rank = cycloneRampIndex(storm.intensityKt);
    const { colors } = this.#ctx;
    const opaque = rank === null ? colors.space : colors.hazardRamp[rank];
    const color = opaque.withAlpha(this.#alpha);
    const at = position([storm.position.lon, storm.position.lat]);
    const tint = Color.WHITE.withAlpha(this.#alpha);
    entry.billboard.position = at;
    entry.billboard.color = tint;
    entry.wash.position = at;
    entry.wash.color = tint;
    this.#styleSpiral(entry, storm, rank, opaque);
    const keyline = colors.keyline.withAlpha(this.#alpha);
    const outline = colors.outline.withAlpha(this.#alpha);
    const current = storm.geometryStatus === 'current';
    const signature = JSON.stringify([
      current && storm.track,
      current && storm.cone,
      this.#alpha,
      storm.intensityKt,
    ]);
    if (entry.signature === signature && entry.expandedVertices <= this.#budget.remaining) {
      this.#budget.remaining -= entry.expandedVertices;
      return;
    }
    this.#clearGeometry(entry);
    if (!current) {
      entry.signature = signature;
      return;
    }
    let complete = true;
    const ref: EntityRef = { feed: 'cyclones', id: storm.id };
    const addLine = (points: readonly Coordinate[], corePx: number) => {
      const positions = surfaceArcPositions(points, this.#budget);
      if (!positions) {
        complete = false;
        return false;
      }
      entry.expandedVertices += positions.length;
      entry.lines.push(
        this.#lines.add({
          positions,
          width: keylineLineWidth(corePx),
          id: ref,
          material: keylineLineMaterial(color, keyline, outline),
        }),
      );
      return true;
    };
    if (storm.track) {
      const parts =
        storm.track.type === 'LineString' ? [storm.track.coordinates] : storm.track.coordinates;
      for (const part of parts) addLine(part, TRACK_CORE_PX);
    }
    if (storm.cone) {
      const polygons =
        storm.cone.type === 'Polygon' ? [storm.cone.coordinates] : storm.cone.coordinates;
      for (const rings of polygons) {
        const exterior = rings[0];
        if (!exterior || exterior.length < 4) continue;
        let supported = true;
        for (const ring of rings) if (!addLine(ring, CONE_CORE_PX)) supported = false;
        if (!supported) continue;
        const hierarchy = new PolygonHierarchy(
          exterior.map(position),
          rings.slice(1).map((hole) => new PolygonHierarchy(hole.map(position))),
        );
        try {
          entry.fills.push(
            this.#root.add(
              new GroundPrimitive({
                geometryInstances: new GeometryInstance({
                  id: ref,
                  geometry: new PolygonGeometry({ polygonHierarchy: hierarchy }),
                  attributes: {
                    color: ColorGeometryInstanceAttribute.fromColor(
                      color.withAlpha(0.16 * this.#alpha),
                    ),
                  },
                }),
                appearance: new PerInstanceColorAppearance({ translucent: true, closed: false }),
                asynchronous: true,
              }),
            ),
          );
        } catch {
          complete = false;
        }
      }
    }
    entry.signature = complete ? signature : '';
  }

  #styleSpiral(entry: Entry, storm: Cyclone, rank: RampRank, fill: Color): void {
    const spiralPx = cycloneSpiralPx(rank);
    const hemisphere = hemisphereOf(storm.position.lat);
    const glyph = cycloneGlyphId(spiralPx, rank ?? 'unknown', hemisphere);
    if (glyph === entry.glyph) return;
    const glyphPx = cycloneGlyphPx(spiralPx);
    if (glyphPx !== entry.glyphPx) {
      const wash = pickWashGlyphId(glyphPx);
      entry.wash.setImage(
        wash,
        this.#glyphs.get(wash, () => drawPickWash(glyphPx, this.#ctx.colors.css.outline)),
      );
    }
    entry.glyph = glyph;
    entry.glyphPx = glyphPx;
    entry.billboard.setImage(
      glyph,
      this.#glyphs.get(glyph, () =>
        drawCycloneSpiral(spiralPx, hazardInk(this.#ctx.colors, fill), hemisphere),
      ),
    );
  }

  #clearGeometry(entry: Entry): void {
    entry.expandedVertices = 0;
    for (const line of entry.lines.splice(0)) this.#lines.remove(line);
    for (const fill of entry.fills.splice(0)) this.#root.remove(fill);
  }

  clear(): void {
    if (this.#entries.clear() > 0) this.#ctx.requestRender();
  }
  setSelected(_id: string | null): void {}
  viewChanged(): void {}
  locate(id: string): Located | undefined {
    const entry = this.#entries.get(id);
    return entry ? { position: entry.billboard.position, symbolPx: entry.glyphPx } : undefined;
  }
  destroy(): void {
    this.#entries.clear();
    this.#root.remove(this.#billboards);
    this.#root.remove(this.#lines);
    this.#root.remove(this.#washes);
  }
}
