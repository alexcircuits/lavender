/**
 * Administrative and territorial boundary features renderer (Natural Earth public domain).
 *
 * Renders international land borders, disputed lines of control, breakaway territories,
 * and maritime indicative lines with distinct styling, per-class toggles, and selection highlighting.
 */
import { Cartesian3, type Color } from '@cesium/core';
import { Material, type Polyline, type PolylineCollection } from '@cesium/engine';
import type { BoundaryClass, BoundaryFeature, GeoPoint } from '../../contracts';
import type { EntityRef } from '../index';
import type { Located, MapColors, RenderContext } from './types';
import { pointInPolygonWasm } from './wasmGeometry';

export const BOUNDARY_LOCATED_PX = 32;

export const DEFAULT_INTERNATIONAL_WIDTH = 1.5;
export const DEFAULT_DISPUTED_WIDTH = 2.0;
export const DEFAULT_BREAKAWAY_WIDTH = 2.0;
export const DEFAULT_MARITIME_WIDTH = 1.2;
export const SELECTED_BOUNDARY_WIDTH = 3.5;

/** Line style for one boundary class: what the globe draws and the legend swatch shows. */
export interface BoundaryClassStyle {
  readonly lineWidth: number;
  readonly stroke: 'solid' | 'dash';
  /** Cesium `PolylineDash` dash length; 0 for solid strokes. */
  readonly dashLength: number;
}

/**
 * Per-class line style (width + dash pattern). Disputed lines of control get a long dash,
 * maritime indicative lines a short dash, land borders and breakaway rings stay solid.
 */
export function boundaryClassStyle(bClass: BoundaryClass): BoundaryClassStyle {
  switch (bClass) {
    case 'internationalLand':
      return { lineWidth: DEFAULT_INTERNATIONAL_WIDTH, stroke: 'solid', dashLength: 0 };
    case 'disputedLine':
      return { lineWidth: DEFAULT_DISPUTED_WIDTH, stroke: 'dash', dashLength: 16 };
    case 'breakawayArea':
      return { lineWidth: DEFAULT_BREAKAWAY_WIDTH, stroke: 'solid', dashLength: 0 };
    case 'maritimeIndicative':
      return { lineWidth: DEFAULT_MARITIME_WIDTH, stroke: 'dash', dashLength: 10 };
  }
}

/** Per-class colour, read from the resolved map palette (tokens.css `--map-boundary-*`). */
export function boundaryClassColor(bClass: BoundaryClass, colors: MapColors): Color {
  switch (bClass) {
    case 'internationalLand':
      return colors.boundaryIntl;
    case 'disputedLine':
      return colors.boundaryDisputed;
    case 'breakawayArea':
      return colors.boundaryBreakaway;
    case 'maritimeIndicative':
      return colors.boundaryMaritime;
  }
}

export interface BoundaryClassVisibility {
  readonly internationalLand: boolean;
  readonly disputedLine: boolean;
  readonly breakawayArea: boolean;
  readonly maritimeIndicative: boolean;
}

export const DEFAULT_BOUNDARY_VISIBILITY: BoundaryClassVisibility = Object.freeze({
  internationalLand: true,
  disputedLine: true,
  breakawayArea: true,
  maritimeIndicative: true,
});

interface BoundaryEntry {
  readonly feature: BoundaryFeature;
  readonly polylines: Polyline[];
  centroid: Cartesian3 | undefined;
}

export class BoundaryRenderer {
  private readonly polylines: PolylineCollection;
  private readonly ctx: RenderContext;

  private readonly entries = new Map<string, BoundaryEntry>();
  private selectedId: string | null = null;
  private visibility: BoundaryClassVisibility = { ...DEFAULT_BOUNDARY_VISIBILITY };

  constructor(polylines: PolylineCollection, ctx: RenderContext) {
    this.polylines = polylines;
    this.ctx = ctx;
  }

  /**
   * Applies boundary features, creating or updating polylines.
   */
  setFeatures(features: readonly BoundaryFeature[]): void {
    const activeIds = new Set<string>();

    for (const feature of features) {
      activeIds.add(feature.id);
      const existing = this.entries.get(feature.id);
      if (existing) {
        continue;
      }

      const boundaryPolylines: Polyline[] = [];
      let centroidX = 0;
      let centroidY = 0;
      let centroidZ = 0;
      let pointCount = 0;

      const isVisible = this.visibility[feature.class] ?? true;
      const isSelected = feature.id === this.selectedId;

      const width = isSelected ? SELECTED_BOUNDARY_WIDTH : this.getDefaultWidth(feature.class);
      const color = isSelected ? this.ctx.colors.selection : this.getDefaultColor(feature.class);
      const material = this.createMaterial(feature.class, color);

      for (const line of feature.lines) {
        const positions: Cartesian3[] = [];
        for (const pt of line) {
          if (!Number.isFinite(pt.lon) || !Number.isFinite(pt.lat)) continue;
          const cart = Cartesian3.fromDegrees(pt.lon, pt.lat, 0);
          positions.push(cart);
          centroidX += cart.x;
          centroidY += cart.y;
          centroidZ += cart.z;
          pointCount++;
        }

        if (positions.length >= 2) {
          const ref: EntityRef = Object.freeze({
            feed: 'boundaries',
            id: feature.id,
          });

          const polyline = this.polylines.add({
            positions,
            width,
            material,
            show: isVisible,
            id: ref,
          });

          boundaryPolylines.push(polyline);
        }
      }

      const centroid =
        pointCount > 0
          ? new Cartesian3(centroidX / pointCount, centroidY / pointCount, centroidZ / pointCount)
          : undefined;

      this.entries.set(feature.id, {
        feature,
        polylines: boundaryPolylines,
        centroid,
      });
    }

    // Prune boundary features no longer present
    for (const [id, entry] of this.entries.entries()) {
      if (!activeIds.has(id)) {
        for (const poly of entry.polylines) {
          this.polylines.remove(poly);
        }
        this.entries.delete(id);
      }
    }
  }

  /**
   * Sets visibility per boundary class without reallocating primitives.
   */
  setVisibility(classes: Partial<BoundaryClassVisibility>): void {
    this.visibility = {
      ...this.visibility,
      ...classes,
    };

    for (const entry of this.entries.values()) {
      const isVisible = this.visibility[entry.feature.class] ?? true;
      for (const poly of entry.polylines) {
        poly.show = isVisible;
      }
    }
  }

  /**
   * Returns current class visibility settings.
   */
  getVisibility(): BoundaryClassVisibility {
    return { ...this.visibility };
  }

  /**
   * Sets selected boundary feature for interactive inspection highlighting.
   */
  setSelected(id: string | null): void {
    if (this.selectedId === id) return;
    const prevId = this.selectedId;
    this.selectedId = id;

    // Reset previous selection
    if (prevId) {
      const prev = this.entries.get(prevId);
      if (prev) {
        const color = this.getDefaultColor(prev.feature.class);
        const width = this.getDefaultWidth(prev.feature.class);
        const material = this.createMaterial(prev.feature.class, color);
        for (const poly of prev.polylines) {
          poly.width = width;
          poly.material = material;
        }
      }
    }

    // Apply new selection
    if (id) {
      const next = this.entries.get(id);
      if (next) {
        const color = this.ctx.colors.selection;
        const width = SELECTED_BOUNDARY_WIDTH;
        const material = Material.fromType('Color', { color });
        for (const poly of next.polylines) {
          poly.width = width;
          poly.material = material;
        }
      }
    }
  }

  /**
   * Returns spatial centroid and pixel size for camera framing.
   */
  locate(id: string): Located | undefined {
    const entry = this.entries.get(id);
    if (!entry?.centroid) return undefined;
    return {
      position: entry.centroid,
      symbolPx: BOUNDARY_LOCATED_PX,
    };
  }

  /**
   * Returns loaded feature metadata for inspector display.
   */
  getFeature(id: string): BoundaryFeature | undefined {
    return this.entries.get(id)?.feature;
  }

  /**
   * Returns count of currently active boundary features.
   */
  getFeatureCount(): number {
    return this.entries.size;
  }

  /**
   * Tests if a geographic coordinate (lon, lat) falls inside any loaded breakaway area polygon
   * using the Zig WebAssembly raycasting micro-kernel.
   */
  findBreakawayAt(lon: number, lat: number): BoundaryFeature | undefined {
    if (!this.visibility.breakawayArea) return undefined;
    for (const entry of this.entries.values()) {
      if (entry.feature.class !== 'breakawayArea') continue;
      const b = entry.feature.bbox;
      if (lon < b.west || lon > b.east || lat < b.south || lat > b.north) continue;
      for (const ring of entry.feature.lines) {
        if (ring.length >= 3) {
          const coords: Array<[number, number]> = ring.map((pt) => [pt.lon, pt.lat]);
          try {
            if (pointInPolygonWasm(lon, lat, coords)) {
              return entry.feature;
            }
          } catch {
            if (this.pointInPolygonFallback(lon, lat, ring)) {
              return entry.feature;
            }
          }
        }
      }
    }
    return undefined;
  }

  private pointInPolygonFallback(lon: number, lat: number, ring: readonly GeoPoint[]): boolean {
    let inside = false;
    for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
      const xi = ring[i]?.lon ?? 0;
      const yi = ring[i]?.lat ?? 0;
      const xj = ring[j]?.lon ?? 0;
      const yj = ring[j]?.lat ?? 0;
      const intersect = yi > lat !== yj > lat && lon < ((xj - xi) * (lat - yi)) / (yj - yi) + xi;
      if (intersect) inside = !inside;
    }
    return inside;
  }

  /**
   * Clears all boundary features and polylines.
   */
  clear(): void {
    for (const entry of this.entries.values()) {
      for (const poly of entry.polylines) {
        this.polylines.remove(poly);
      }
    }
    this.entries.clear();
    this.selectedId = null;
  }

  /**
   * Cleans up all resources.
   */
  destroy(): void {
    this.clear();
  }

  private getDefaultColor(bClass: BoundaryClass): Color {
    return boundaryClassColor(bClass, this.ctx.colors);
  }

  private getDefaultWidth(bClass: BoundaryClass): number {
    return boundaryClassStyle(bClass).lineWidth;
  }

  private createMaterial(bClass: BoundaryClass, color: Color): Material {
    const style = boundaryClassStyle(bClass);
    if (style.stroke === 'dash') {
      return Material.fromType('PolylineDash', { color, dashLength: style.dashLength });
    }
    return Material.fromType('Color', { color });
  }
}
