/** Keyed storm positions and advisory-matched forecast geometry; no frame-time updates. */
import {
  Cartesian3,
  Cartographic,
  ColorGeometryInstanceAttribute,
  EllipsoidGeodesic,
  GeometryInstance,
  PolygonGeometry,
  PolygonHierarchy,
} from '@cesium/core';
import {
  GroundPrimitive,
  Material,
  PerInstanceColorAppearance,
  type PointPrimitive,
  PointPrimitiveCollection,
  type Polyline,
  PolylineCollection,
  type PrimitiveCollection,
} from '@cesium/engine';
import type { Cyclone, FeedSnapshot } from '../../contracts';
import type { EntityRef } from '../index';
import { KeyedEntries } from './keyed';
import { hasValidPosition, isStale, STALE_ALPHA } from './symbology';
import type { FeedRenderer, Located, RenderContext } from './types';

type Coordinate = readonly [number, number];

/** Sustained wind in knots: depression, storm, category1/2, category3/4, category5. */
export function cycloneRampIndex(wind: number | undefined): 0 | 1 | 2 | 3 | 4 {
  if (wind === undefined || wind < 34) return 0;
  if (wind < 64) return 1;
  if (wind < 96) return 2;
  return wind < 137 ? 3 : 4;
}

function position([lon, lat]: Coordinate): Cartesian3 {
  return Cartesian3.fromDegrees(lon, lat, 50);
}

/** Sample surface arcs so long Cartesian chords do not disappear inside Earth. */
function arcPositions(coordinates: readonly Coordinate[]): Cartesian3[] {
  const positions: Cartesian3[] = [];
  for (let i = 1; i < coordinates.length; i++) {
    const from = coordinates[i - 1];
    const to = coordinates[i];
    if (!from || !to) continue;
    const arc = new EllipsoidGeodesic(
      Cartographic.fromDegrees(...from),
      Cartographic.fromDegrees(...to),
    );
    const count = Math.min(128, Math.max(1, Math.ceil(arc.surfaceDistance / 50_000)));
    for (let step = 0; step < count; step++) {
      const point = arc.interpolateUsingFraction(step / count);
      point.height = 50;
      positions.push(Cartographic.toCartesian(point));
    }
  }
  const last = coordinates.at(-1);
  if (last) positions.push(position(last));
  return positions;
}

interface Entry {
  readonly point: PointPrimitive;
  item: Cyclone;
  signature: string;
  readonly lines: Polyline[];
  readonly fills: GroundPrimitive[];
}

export class CycloneRenderer implements FeedRenderer {
  readonly feed = 'cyclones' as const;
  readonly #root: PrimitiveCollection;
  readonly #ctx: RenderContext;
  readonly #points: PointPrimitiveCollection;
  readonly #lines: PolylineCollection;
  readonly #entries: KeyedEntries<Cyclone, Entry>;
  #alpha = 1;

  constructor(root: PrimitiveCollection, ctx: RenderContext) {
    this.#root = root;
    this.#ctx = ctx;
    this.#points = root.add(new PointPrimitiveCollection());
    this.#lines = root.add(new PolylineCollection());
    this.#entries = new KeyedEntries((storm) => storm.id, {
      create: (storm, id) => {
        const ref: EntityRef = { feed: 'cyclones', id };
        const entry: Entry = {
          point: this.#points.add({ id: ref }),
          item: storm,
          signature: '',
          lines: [],
          fills: [],
        };
        this.#update(entry, storm);
        return entry;
      },
      update: (entry, storm) => this.#update(entry, storm),
      remove: (entry) => {
        this.#clearGeometry(entry);
        this.#points.remove(entry.point);
      },
    });
  }

  get size(): number {
    return this.#entries.size;
  }

  apply(snapshot: FeedSnapshot): void {
    if (snapshot.items.kind !== 'cyclones') return;
    this.#alpha = isStale(snapshot.freshness) ? STALE_ALPHA : 1;
    this.#entries.apply(snapshot.items.items.filter(hasValidPosition));
    this.#ctx.requestRender();
  }

  #update(entry: Entry, storm: Cyclone): void {
    entry.item = storm;
    const color = this.#ctx.colors.hazardRamp[cycloneRampIndex(storm.intensityKt)].withAlpha(
      this.#alpha,
    );
    entry.point.position = position([storm.position.lon, storm.position.lat]);
    entry.point.pixelSize = 12 + cycloneRampIndex(storm.intensityKt) * 2;
    entry.point.color = color;
    entry.point.outlineColor = this.#ctx.colors.outline.withAlpha(this.#alpha);
    entry.point.outlineWidth = 2;
    const current = storm.geometryStatus === 'current';
    const signature = JSON.stringify([
      current && storm.track,
      current && storm.cone,
      this.#alpha,
      storm.intensityKt,
    ]);
    if (entry.signature === signature) return;
    entry.signature = signature;
    this.#clearGeometry(entry);
    if (!current) return;
    const ref: EntityRef = { feed: 'cyclones', id: storm.id };
    const addLine = (points: readonly Coordinate[], width: number) => {
      if (points.length < 2) return;
      entry.lines.push(
        this.#lines.add({
          positions: arcPositions(points),
          width,
          id: ref,
          material: Material.fromType('Color', { color }),
        }),
      );
    };
    if (storm.track) {
      const parts =
        storm.track.type === 'LineString' ? [storm.track.coordinates] : storm.track.coordinates;
      for (const part of parts) addLine(part, 2);
    }
    if (storm.cone) {
      const polygons =
        storm.cone.type === 'Polygon' ? [storm.cone.coordinates] : storm.cone.coordinates;
      for (const rings of polygons) {
        const exterior = rings[0];
        if (!exterior || exterior.length < 4) continue;
        for (const ring of rings) addLine(ring, 1);
        const hierarchy = new PolygonHierarchy(
          exterior.map(position),
          rings.slice(1).map((hole) => new PolygonHierarchy(hole.map(position))),
        );
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
      }
    }
  }

  #clearGeometry(entry: Entry): void {
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
    return entry ? { position: entry.point.position, symbolPx: entry.point.pixelSize } : undefined;
  }
  destroy(): void {
    this.#entries.clear();
    this.#root.remove(this.#points);
    this.#root.remove(this.#lines);
  }
}
