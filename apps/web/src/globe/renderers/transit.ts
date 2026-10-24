/**
 * Public transit vehicle and trajectory trail renderer (GTFS-RT feeds).
 * Displays live buses, trams, ferries, and trains with heading and route context.
 * Historical vehicle trails are simplified using the Zig WASM RDP micro-kernel.
 */
import { Cartesian3, Color } from '@cesium/core';
import {
  PointPrimitiveCollection,
  PolylineCollection,
  type PointPrimitive,
  type Polyline,
  type PrimitiveCollection,
} from '@cesium/engine';
import type { FeedSnapshot, TransitTrail, TransitVehicle } from '../../contracts';
import type { EntityRef } from '../index';
import { KeyedEntries } from './keyed';
import type { FeedRenderer, Located, RenderContext } from './types';
import { simplifyRdpWasm } from './wasmGeometry';

const DEFAULT_TRANSIT_COLOR =
  Color.fromCssColorString('#38bdf8') ?? new Color(0.22, 0.74, 0.97, 0.9);
const SELECTED_TRAIL_COLOR =
  Color.fromCssColorString('#a78bfa') ?? new Color(0.655, 0.545, 0.98, 0.85);

export class TransitRenderer implements FeedRenderer {
  readonly feed = 'transit' as const;
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #points: PointPrimitiveCollection;
  readonly #trailPolylines: PolylineCollection;
  readonly #items = new Map<string, TransitVehicle>();
  #selectedId: string | null = null;
  #trailPolyline: Polyline | null = null;
  readonly #entries: KeyedEntries<TransitVehicle, PointPrimitive>;

  constructor(
    primitives: PrimitiveCollection,
    ctx: RenderContext,
    lines?: PolylineCollection,
    points?: PointPrimitiveCollection,
  ) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#points = points ?? primitives.add(new PointPrimitiveCollection());
    this.#trailPolylines = lines ?? primitives.add(new PolylineCollection());

    this.#entries = new KeyedEntries<TransitVehicle, PointPrimitive>(
      (v) => v.id,
      {
        create: (vehicle, id) => {
          this.#items.set(id, vehicle);
          const isSelected = id === this.#selectedId;
          const color = isSelected ? this.#ctx.colors.selection : DEFAULT_TRANSIT_COLOR;
          const size = isSelected ? 12 : 7;
          const pos = Cartesian3.fromDegrees(vehicle.position.lon, vehicle.position.lat, 0);

          return this.#points.add({
            position: pos,
            color,
            pixelSize: size,
            id: { feed: this.feed, id } satisfies EntityRef,
          });
        },
        update: (point, vehicle, id) => {
          this.#items.set(id, vehicle);
          const isSelected = id === this.#selectedId;
          const color = isSelected ? this.#ctx.colors.selection : DEFAULT_TRANSIT_COLOR;

          point.position = Cartesian3.fromDegrees(vehicle.position.lon, vehicle.position.lat, 0);
          point.color = color;
          point.pixelSize = isSelected ? 12 : 7;
        },
        remove: (point, id) => {
          this.#items.delete(id);
          this.#points.remove(point);
        },
      },
    );
  }

  apply(snapshot: FeedSnapshot): void {
    if (snapshot.items.kind !== 'transit') return;
    this.#entries.apply(snapshot.items.items);
    this.#ctx.requestRender();
  }

  setSelected(id: string | null): void {
    this.#selectedId = id;
    for (const [vehicleId, point] of this.#entries.entries()) {
      const isSelected = vehicleId === id;
      point.color = isSelected ? this.#ctx.colors.selection : DEFAULT_TRANSIT_COLOR;
      point.pixelSize = isSelected ? 12 : 7;
    }
    if (!id && this.#trailPolyline && typeof this.#trailPolylines.remove === 'function') {
      this.#trailPolylines.remove(this.#trailPolyline);
      this.#trailPolyline = null;
    }
    this.#ctx.requestRender();
  }

  /**
   * Shows the historical trail for the active transit vehicle.
   * Simplifies dense trajectory points using the Zig WASM RDP kernel.
   */
  showTrail(trail: TransitTrail | null): void {
    if (this.#trailPolyline && typeof this.#trailPolylines.remove === 'function') {
      this.#trailPolylines.remove(this.#trailPolyline);
      this.#trailPolyline = null;
    }

    if (!trail || trail.fixes.length < 2 || typeof this.#trailPolylines.add !== 'function') {
      this.#ctx.requestRender();
      return;
    }

    // Extract [lon, lat] pairs from fixes: [time_ms, lat, lon, flags, epoch_id]
    const rawCoords: Array<[number, number]> = trail.fixes.map((fix) => [fix[2], fix[1]]);

    // Accelerate polyline simplification via Zig WASM micro-kernel
    const simplified = simplifyRdpWasm(rawCoords, 0.00005);
    const positions = (simplified.length > 0 ? simplified : rawCoords).map(([lon, lat]) =>
      Cartesian3.fromDegrees(lon, lat, 0),
    );

    this.#trailPolyline = this.#trailPolylines.add({
      positions,
      width: 2.5,
      material: Color.fromCssColorString('#a78bfa') ?? SELECTED_TRAIL_COLOR,
    });

    this.#ctx.requestRender();
  }

  locate(id: string): Located | undefined {
    const vehicle = this.#items.get(id);
    if (!vehicle) return undefined;
    return {
      position: Cartesian3.fromDegrees(vehicle.position.lon, vehicle.position.lat, 0),
      symbolPx: 32,
    };
  }

  clear(): void {
    this.#entries.clear();
    this.#items.clear();
    this.#selectedId = null;
    if (this.#trailPolyline && typeof this.#trailPolylines.remove === 'function') {
      this.#trailPolylines.remove(this.#trailPolyline);
      this.#trailPolyline = null;
    }
    this.#ctx.requestRender();
  }

  viewChanged(): void {}

  destroy(): void {
    this.clear();
    if (typeof this.#primitives.remove === 'function') {
      this.#primitives.remove(this.#points);
      this.#primitives.remove(this.#trailPolylines);
    }
  }
}
