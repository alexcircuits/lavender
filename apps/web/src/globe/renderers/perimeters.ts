/**
 * Wildfire perimeters: active fire perimeter polygons and incident details (NIFC WFIGS).
 * Renders draped ground polygons (GroundPrimitive) with crisp outlined fire lines (Primitive
 * with PolylineGeometry). Translucent fill draped over terrain/ellipsoid, keyed diffing
 * to avoid primitive churn, and selection highlighting that turns the fire line to the
 * selection colour.
 */
import {
  Cartesian3,
  Color,
  ColorGeometryInstanceAttribute,
  GeometryInstance,
  PolygonGeometry,
  PolygonHierarchy,
  PolylineGeometry,
} from '@cesium/core';
import {
  GroundPrimitive,
  PerInstanceColorAppearance,
  PolylineColorAppearance,
  Primitive,
  type PrimitiveCollection,
} from '@cesium/engine';
import type { FeedSnapshot, FirePerimeter } from '../../contracts';
import type { EntityRef } from '../index';
import { KeyedEntries } from './keyed';
import { isStale, STALE_ALPHA } from './symbology';
import type { FeedRenderer, Located, RenderContext } from './types';

export const DEFAULT_PERIMETER_COLOR =
  Color.fromCssColorString('#f27036') ?? new Color(0.949, 0.439, 0.212, 1);
export const DEFAULT_PERIMETER_FILL_ALPHA = 0.25;
export const DEFAULT_PERIMETER_BORDER_WIDTH = 2;
export const PERIMETER_LOCATED_PX = 32;

type Coordinate = readonly [number, number];
type Ring = readonly Coordinate[];
type Polygon = readonly Ring[];
type MultiPolygon = readonly Polygon[];

export interface PerimeterItem extends Partial<FirePerimeter> {
  readonly id: string;
  readonly incidentName?: string;
  readonly containmentPct?: number;
  readonly rings?:
    | readonly (readonly Coordinate[])[]
    | readonly (readonly (readonly Coordinate[])[])[];
}

interface PerimeterEntry {
  item: FirePerimeter | PerimeterItem;
  readonly fills: GroundPrimitive[];
  readonly borders: Primitive[];
  anchor: Cartesian3 | undefined;
  signature: string;
}

function isValidCoordinate(coord: unknown): coord is [number, number] {
  if (!Array.isArray(coord) || coord.length < 2) return false;
  const lon = coord[0];
  const lat = coord[1];
  return (
    typeof lon === 'number' &&
    Number.isFinite(lon) &&
    Math.abs(lon) <= 180 &&
    typeof lat === 'number' &&
    Number.isFinite(lat) &&
    Math.abs(lat) <= 90
  );
}

function isValidRing(ring: unknown): ring is Array<[number, number]> {
  if (!Array.isArray(ring) || ring.length < 3) return false;
  return ring.every(isValidCoordinate);
}

/**
 * Extracts normalized array of polygons (each an array of rings [outer, ...holes])
 * from either GeoJSON MultiPolygon coordinates or single-polygon rings.
 */
export function extractPolygons(
  item: FirePerimeter | PerimeterItem,
): Array<Array<Array<[number, number]>>> {
  const candidate = item as {
    polygons?: unknown;
    rings?: unknown;
  };
  const result: Array<Array<Array<[number, number]>>> = [];

  if (Array.isArray(candidate.polygons)) {
    for (const poly of candidate.polygons) {
      if (!Array.isArray(poly) || poly.length === 0) continue;
      const validRings: Array<Array<[number, number]>> = [];
      for (const ring of poly) {
        if (isValidRing(ring)) validRings.push(ring);
      }
      if (validRings.length > 0 && validRings[0]) {
        result.push(validRings);
      }
    }
    if (result.length > 0) return result;
  }

  if (Array.isArray(candidate.rings) && candidate.rings.length > 0) {
    const first = candidate.rings[0];
    if (Array.isArray(first) && first.length > 0) {
      const firstElem = first[0];
      // Case 1: candidate.rings is an array of rings: [outerRing, ...holeRings]
      if (Array.isArray(firstElem) && typeof firstElem[0] === 'number') {
        const validRings: Array<Array<[number, number]>> = [];
        for (const ring of candidate.rings) {
          if (isValidRing(ring)) validRings.push(ring);
        }
        if (validRings.length > 0 && validRings[0]) {
          result.push(validRings);
        }
        return result;
      }
      // Case 2: candidate.rings is an array of polygons: [[outer, ...holes], ...]
      if (
        Array.isArray(firstElem) &&
        Array.isArray(firstElem[0]) &&
        typeof firstElem[0][0] === 'number'
      ) {
        for (const poly of candidate.rings) {
          if (!Array.isArray(poly) || poly.length === 0) continue;
          const validRings: Array<Array<[number, number]>> = [];
          for (const ring of poly) {
            if (isValidRing(ring)) validRings.push(ring);
          }
          if (validRings.length > 0 && validRings[0]) {
            result.push(validRings);
          }
        }
        return result;
      }
    }
  }

  return result;
}

export function isValidPerimeter(item: FirePerimeter | PerimeterItem): boolean {
  if (typeof item?.id !== 'string' || item.id.trim() === '') return false;
  return extractPolygons(item).length > 0;
}

function ensureClosedRing(ring: readonly (readonly [number, number])[]): Cartesian3[] {
  if (ring.length === 0) return [];
  const positions = ring.map(([lon, lat]) => Cartesian3.fromDegrees(lon, lat));
  const first = ring[0];
  const last = ring[ring.length - 1];
  if (first && last && (first[0] !== last[0] || first[1] !== last[1])) {
    positions.push(Cartesian3.fromDegrees(first[0], first[1]));
  }
  return positions;
}

function computeAnchor(polygons: MultiPolygon): Cartesian3 | undefined {
  if (polygons.length === 0) return undefined;
  let maxPoints = 0;
  let largestRing: Ring | undefined;
  for (const rings of polygons) {
    const outer = rings[0];
    if (outer && outer.length > maxPoints) {
      maxPoints = outer.length;
      largestRing = outer;
    }
  }
  if (!largestRing || largestRing.length === 0) return undefined;
  let lonSum = 0;
  let latSum = 0;
  const first = largestRing[0];
  const last = largestRing[largestRing.length - 1];
  const isClosed =
    largestRing.length > 1 &&
    first !== undefined &&
    last !== undefined &&
    first[0] === last[0] &&
    first[1] === last[1];
  const count = isClosed ? largestRing.length - 1 : largestRing.length;
  if (count <= 0) return undefined;
  for (let i = 0; i < count; i++) {
    const pt = largestRing[i];
    if (pt) {
      lonSum += pt[0];
      latSum += pt[1];
    }
  }
  return Cartesian3.fromDegrees(lonSum / count, latSum / count);
}

function anchorOf(
  item: FirePerimeter | PerimeterItem,
  polygons: MultiPolygon,
): Cartesian3 | undefined {
  if (
    item.anchor &&
    typeof item.anchor.lon === 'number' &&
    typeof item.anchor.lat === 'number' &&
    Number.isFinite(item.anchor.lon) &&
    Number.isFinite(item.anchor.lat)
  ) {
    return Cartesian3.fromDegrees(item.anchor.lon, item.anchor.lat);
  }
  return computeAnchor(polygons);
}

export class PerimeterRenderer implements FeedRenderer {
  readonly feed = 'perimeters' as const;
  readonly #root: PrimitiveCollection;
  readonly #ctx: RenderContext;
  readonly #entries: KeyedEntries<FirePerimeter, PerimeterEntry>;
  #selected: string | null = null;
  #alpha = 1;

  constructor(root: PrimitiveCollection, ctx: RenderContext) {
    this.#root = root;
    this.#ctx = ctx;
    this.#entries = new KeyedEntries<FirePerimeter, PerimeterEntry>((item) => item.id, {
      create: (item, id) => this.#create(item, id),
      update: (entry, item, id) => this.#update(entry, item, id),
      remove: (entry) => this.#remove(entry),
    });
  }

  get size(): number {
    return this.#entries.size;
  }

  apply(snapshot: FeedSnapshot): void {
    if (snapshot.items.kind !== 'perimeters') return;
    this.#alpha = isStale(snapshot.freshness) ? STALE_ALPHA : 1;
    this.#entries.apply(snapshot.items.items.filter(isValidPerimeter));
    this.#ctx.requestRender();
  }

  clear(): void {
    this.#selected = null;
    if (this.#entries.clear() > 0) this.#ctx.requestRender();
  }

  setSelected(id: string | null): void {
    if (this.#selected === id) return;
    const previousId = this.#selected;
    this.#selected = id;

    if (previousId) {
      const prevEntry = this.#entries.get(previousId);
      if (prevEntry) this.#updateBorders(prevEntry, false);
    }
    if (id) {
      const newEntry = this.#entries.get(id);
      if (newEntry) this.#updateBorders(newEntry, true);
    }
    this.#ctx.requestRender();
  }

  locate(id: string): Located | undefined {
    const entry = this.#entries.get(id);
    if (!entry?.anchor) return undefined;
    return { position: entry.anchor, symbolPx: PERIMETER_LOCATED_PX };
  }

  viewChanged(): void {}

  destroy(): void {
    this.#selected = null;
    this.#entries.clear();
  }

  #create(item: FirePerimeter, id: string): PerimeterEntry {
    const entry: PerimeterEntry = {
      item,
      fills: [],
      borders: [],
      anchor: undefined,
      signature: '',
    };
    this.#updateEntry(entry, item, id);
    return entry;
  }

  #update(entry: PerimeterEntry, item: FirePerimeter, id: string): void {
    this.#updateEntry(entry, item, id);
  }

  #remove(entry: PerimeterEntry): void {
    this.#clearPrimitives(entry);
  }

  #clearPrimitives(entry: PerimeterEntry): void {
    for (const fill of entry.fills.splice(0)) {
      this.#root.remove(fill);
    }
    for (const border of entry.borders.splice(0)) {
      this.#root.remove(border);
    }
  }

  #updateEntry(entry: PerimeterEntry, item: FirePerimeter, id: string): void {
    const isSelected = this.#selected === id;
    const polygons = extractPolygons(item);
    const signature = JSON.stringify([id, polygons, this.#alpha, isSelected]);

    if (entry.signature === signature) {
      entry.item = item;
      return;
    }

    entry.item = item;
    entry.signature = signature;
    this.#clearPrimitives(entry);

    entry.anchor = anchorOf(item, polygons);

    const ref: EntityRef = Object.freeze({ feed: 'perimeters', id });
    const fillColor = DEFAULT_PERIMETER_COLOR.withAlpha(DEFAULT_PERIMETER_FILL_ALPHA * this.#alpha);

    for (const rings of polygons) {
      const exterior = rings[0];
      if (!exterior || exterior.length < 3) continue;
      const holes = rings.slice(1).filter((hole) => hole.length >= 3);
      const exteriorPositions = exterior.map(([lon, lat]) => Cartesian3.fromDegrees(lon, lat));
      const holeHierarchies = holes.map(
        (hole) => new PolygonHierarchy(hole.map(([lon, lat]) => Cartesian3.fromDegrees(lon, lat))),
      );
      const hierarchy = new PolygonHierarchy(exteriorPositions, holeHierarchies);

      try {
        const fill = new GroundPrimitive({
          geometryInstances: new GeometryInstance({
            id: ref,
            geometry: new PolygonGeometry({
              polygonHierarchy: hierarchy,
            }),
            attributes: {
              color: ColorGeometryInstanceAttribute.fromColor(fillColor),
            },
          }),
          appearance: new PerInstanceColorAppearance({ translucent: true, closed: false }),
          asynchronous: false,
        });
        entry.fills.push(this.#root.add(fill) as GroundPrimitive);
      } catch {
        // Degenerate geometry skipped safely
      }
    }

    this.#updateBorders(entry, isSelected);
  }

  #updateBorders(entry: PerimeterEntry, isSelected: boolean): void {
    for (const border of entry.borders.splice(0)) {
      this.#root.remove(border);
    }

    const polygons = extractPolygons(entry.item);
    const borderColor = (
      isSelected ? this.#ctx.colors.selection : DEFAULT_PERIMETER_COLOR
    ).withAlpha(this.#alpha);
    const ref: EntityRef = Object.freeze({ feed: 'perimeters', id: entry.item.id });

    for (const rings of polygons) {
      for (const ring of rings) {
        if (ring.length < 3) continue;
        const positions = ensureClosedRing(ring);
        if (positions.length < 3) continue;

        try {
          const border = new Primitive({
            geometryInstances: new GeometryInstance({
              id: ref,
              geometry: new PolylineGeometry({
                positions,
                width: DEFAULT_PERIMETER_BORDER_WIDTH,
              }),
              attributes: {
                color: ColorGeometryInstanceAttribute.fromColor(borderColor),
              },
            }),
            appearance: new PolylineColorAppearance(),
            asynchronous: false,
          });
          entry.borders.push(this.#root.add(border) as Primitive);
        } catch {
          // Degenerate geometry skipped safely
        }
      }
    }

    entry.signature = JSON.stringify([entry.item.id, polygons, this.#alpha, isSelected]);
  }
}
