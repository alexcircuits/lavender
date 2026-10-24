/**
 * Submarine cables and coastal landing points renderer (TeleGeography CC BY-NC-SA 3.0).
 * Renders fiber-optic submarine cable routes across ocean basins as geodesic polylines
 * and landing stations as luminous coastal point primitives.
 *
 * Implements interactive selection, visual highlighting, and spatial centroid location.
 */
import { Cartesian3, Color } from '@cesium/core';
import {
  Material,
  type PointPrimitive,
  type PointPrimitiveCollection,
  type Polyline,
  type PolylineCollection,
} from '@cesium/engine';
import type { EntityRef } from '../index';
import type { Located, RenderContext } from './types';

export const DEFAULT_CABLE_COLOR =
  Color.fromCssColorString('#39d5ff') ?? new Color(0.224, 0.835, 1.0, 0.7);
export const DEFAULT_LANDING_COLOR =
  Color.fromCssColorString('#8fffd2') ?? new Color(0.561, 1.0, 0.824, 0.9);
export const DEFAULT_CABLE_WIDTH = 2.0;
export const SELECTED_CABLE_WIDTH = 4.0;
export const LANDING_POINT_PIXEL_SIZE = 6;
export const CABLE_LOCATED_PX = 24;

export interface SubmarineCableItem {
  readonly id: string;
  readonly name: string;
  readonly color?: string | undefined;
  readonly coordinates: readonly (readonly (readonly [number, number])[])[];
  readonly bbox?:
    | {
        readonly west: number;
        readonly south: number;
        readonly east: number;
        readonly north: number;
      }
    | undefined;
}

export interface LandingPointItem {
  readonly id: string;
  readonly name: string;
  readonly position: {
    readonly lon: number;
    readonly lat: number;
  };
  readonly isTbd?: boolean | undefined;
}

interface CableEntry {
  readonly item: SubmarineCableItem;
  readonly polylines: Polyline[];
  centroid: Cartesian3 | undefined;
}

interface LandingEntry {
  readonly item: LandingPointItem;
  readonly point: PointPrimitive;
}

export class CableRenderer {
  private readonly polylines: PolylineCollection;
  private readonly points: PointPrimitiveCollection;
  private readonly ctx: RenderContext;

  private readonly cables = new Map<string, CableEntry>();
  private readonly landings = new Map<string, LandingEntry>();
  private selectedId: string | null = null;

  constructor(polylines: PolylineCollection, points: PointPrimitiveCollection, ctx: RenderContext) {
    this.polylines = polylines;
    this.points = points;
    this.ctx = ctx;
  }

  /**
   * Applies a collection of submarine cables to the polyline primitive collection.
   */
  setCables(items: readonly SubmarineCableItem[]): void {
    const nextIds = new Set<string>();

    for (const item of items) {
      nextIds.add(item.id);
      const existing = this.cables.get(item.id);

      if (existing) {
        // Update color/state if needed
        continue;
      }

      const isSelected = item.id === this.selectedId;
      const baseColor = item.color
        ? (Color.fromCssColorString(item.color) ?? DEFAULT_CABLE_COLOR)
        : DEFAULT_CABLE_COLOR;
      const color = isSelected ? this.ctx.colors.selection : baseColor.withAlpha(0.7);
      const width = isSelected ? SELECTED_CABLE_WIDTH : DEFAULT_CABLE_WIDTH;

      const cablePolylines: Polyline[] = [];
      let centroidX = 0;
      let centroidY = 0;
      let centroidZ = 0;
      let pointCount = 0;

      for (const segment of item.coordinates) {
        if (segment.length < 2) continue;

        const positions: Cartesian3[] = [];
        for (const [lon, lat] of segment) {
          if (!Number.isFinite(lon) || !Number.isFinite(lat)) continue;
          const cart = Cartesian3.fromDegrees(lon, lat, 0);
          positions.push(cart);
          centroidX += cart.x;
          centroidY += cart.y;
          centroidZ += cart.z;
          pointCount++;
        }

        if (positions.length >= 2) {
          const polyline = this.polylines.add({
            positions,
            width,
            material: Material.fromType('Color', { color }),
          });

          // Tag with EntityRef for interactive picking
          (polyline as unknown as { entityRef: EntityRef }).entityRef = {
            feed: 'cables' as const as unknown as EntityRef['feed'],
            id: item.id,
          };

          cablePolylines.push(polyline);
        }
      }

      const centroid =
        pointCount > 0
          ? new Cartesian3(centroidX / pointCount, centroidY / pointCount, centroidZ / pointCount)
          : undefined;

      this.cables.set(item.id, {
        item,
        polylines: cablePolylines,
        centroid,
      });
    }

    // Prune removed cables
    for (const [id, entry] of this.cables) {
      if (!nextIds.has(id)) {
        for (const poly of entry.polylines) {
          this.polylines.remove(poly);
        }
        this.cables.delete(id);
      }
    }

    this.ctx.requestRender();
  }

  /**
   * Applies coastal landing stations to the point primitive collection.
   */
  setLandings(items: readonly LandingPointItem[]): void {
    const nextIds = new Set<string>();

    for (const item of items) {
      nextIds.add(item.id);
      if (this.landings.has(item.id)) continue;

      const pos = Cartesian3.fromDegrees(item.position.lon, item.position.lat, 0);
      const point = this.points.add({
        position: pos,
        pixelSize: LANDING_POINT_PIXEL_SIZE,
        color: DEFAULT_LANDING_COLOR,
        outlineColor: this.ctx.colors.outline,
        outlineWidth: 1,
      });

      // Tag for picking
      (point as unknown as { entityRef: EntityRef }).entityRef = {
        feed: 'cables' as const as unknown as EntityRef['feed'],
        id: item.id,
      };

      this.landings.set(item.id, { item, point });
    }

    // Prune removed landings
    for (const [id, entry] of this.landings) {
      if (!nextIds.has(id)) {
        this.points.remove(entry.point);
        this.landings.delete(id);
      }
    }

    this.ctx.requestRender();
  }

  /**
   * Sets the active selection, highlighting the chosen cable.
   */
  setSelected(id: string | null): void {
    if (this.selectedId === id) return;
    const prevId = this.selectedId;
    this.selectedId = id;

    // Reset previous selection
    if (prevId) {
      const prevEntry = this.cables.get(prevId);
      if (prevEntry) {
        const baseColor = prevEntry.item.color
          ? (Color.fromCssColorString(prevEntry.item.color) ?? DEFAULT_CABLE_COLOR)
          : DEFAULT_CABLE_COLOR;
        for (const poly of prevEntry.polylines) {
          poly.width = DEFAULT_CABLE_WIDTH;
          poly.material = Material.fromType('Color', {
            color: baseColor.withAlpha(0.7),
          });
        }
      }
    }

    // Apply new selection
    if (id) {
      const entry = this.cables.get(id);
      if (entry) {
        for (const poly of entry.polylines) {
          poly.width = SELECTED_CABLE_WIDTH;
          poly.material = Material.fromType('Color', {
            color: this.ctx.colors.selection,
          });
        }
      }
    }

    this.ctx.requestRender();
  }

  /**
   * Returns the world Cartesian centroid and symbol size of an entity for camera framing.
   */
  locate(id: string): Located | undefined {
    const cable = this.cables.get(id);
    if (cable?.centroid) {
      return {
        position: cable.centroid,
        symbolPx: CABLE_LOCATED_PX,
      };
    }
    const landing = this.landings.get(id);
    if (landing) {
      return {
        position: landing.point.position,
        symbolPx: LANDING_POINT_PIXEL_SIZE,
      };
    }
    return undefined;
  }

  viewChanged(): void {
    // Camera movement hook (LOD updates if needed)
  }

  clear(): void {
    for (const entry of this.cables.values()) {
      for (const poly of entry.polylines) {
        this.polylines.remove(poly);
      }
    }
    this.cables.clear();

    for (const entry of this.landings.values()) {
      this.points.remove(entry.point);
    }
    this.landings.clear();

    this.selectedId = null;
    this.ctx.requestRender();
  }

  destroy(): void {
    this.clear();
  }

  get cableCount(): number {
    return this.cables.size;
  }

  get landingCount(): number {
    return this.landings.size;
  }
}
