/**
 * Bike-share station network renderer (GBFS v2 / v3 feeds).
 * Displays live docked stations, rental status, and dock availability.
 */
import { Cartesian3, Color } from '@cesium/core';
import {
  type PointPrimitive,
  PointPrimitiveCollection,
  type PrimitiveCollection,
} from '@cesium/engine';
import type { BikeshareStation, FeedSnapshot } from '../../contracts';
import type { EntityRef } from '../index';
import { KeyedEntries } from './keyed';
import type { FeedRenderer, Located, RenderContext } from './types';

const NORMAL_COLOR = Color.fromCssColorString('#00d26a') ?? new Color(0.0, 0.824, 0.416, 0.9);
const LOW_COLOR = Color.fromCssColorString('#f59e0b') ?? new Color(0.96, 0.62, 0.04, 0.9);
const EMPTY_COLOR = Color.fromCssColorString('#ef4444') ?? new Color(0.937, 0.267, 0.267, 0.9);

export class BikeshareRenderer implements FeedRenderer {
  readonly feed = 'bikeshare' as const;
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #points: PointPrimitiveCollection;
  readonly #items = new Map<string, BikeshareStation>();
  #selectedId: string | null = null;
  readonly #entries: KeyedEntries<BikeshareStation, PointPrimitive>;

  constructor(primitives: PrimitiveCollection, ctx: RenderContext) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#points = primitives.add(new PointPrimitiveCollection());

    this.#entries = new KeyedEntries<BikeshareStation, PointPrimitive>((s) => s.id, {
      create: (station, id) => {
        this.#items.set(id, station);
        const isSelected = id === this.#selectedId;
        const bikes = station.bikesAvailable ?? 0;
        const color = isSelected
          ? this.#ctx.colors.selection
          : bikes === 0
            ? EMPTY_COLOR
            : bikes < 3
              ? LOW_COLOR
              : NORMAL_COLOR;

        const size = isSelected ? 10 : 6;
        const pos = Cartesian3.fromDegrees(station.position.lon, station.position.lat, 0);

        return this.#points.add({
          position: pos,
          color,
          pixelSize: size,
          id: { feed: this.feed, id } satisfies EntityRef,
        });
      },
      update: (point, station, id) => {
        this.#items.set(id, station);
        const isSelected = id === this.#selectedId;
        const bikes = station.bikesAvailable ?? 0;
        const color = isSelected
          ? this.#ctx.colors.selection
          : bikes === 0
            ? EMPTY_COLOR
            : bikes < 3
              ? LOW_COLOR
              : NORMAL_COLOR;

        point.position = Cartesian3.fromDegrees(station.position.lon, station.position.lat, 0);
        point.color = color;
        point.pixelSize = isSelected ? 10 : 6;
      },
      remove: (point, id) => {
        this.#items.delete(id);
        this.#points.remove(point);
      },
    });
  }

  apply(snapshot: FeedSnapshot): void {
    if (snapshot.items.kind !== 'bikeshare') return;
    this.#entries.apply(snapshot.items.items);
    this.#ctx.requestRender();
  }

  setSelected(id: string | null): void {
    this.#selectedId = id;
    for (const [stationId, point] of this.#entries.entries()) {
      const isSelected = stationId === id;
      const station = this.#items.get(stationId);
      const bikes = station?.bikesAvailable ?? 0;

      point.color = isSelected
        ? this.#ctx.colors.selection
        : bikes === 0
          ? EMPTY_COLOR
          : bikes < 3
            ? LOW_COLOR
            : NORMAL_COLOR;
      point.pixelSize = isSelected ? 10 : 6;
    }
    this.#ctx.requestRender();
  }

  locate(id: string): Located | undefined {
    const station = this.#items.get(id);
    if (!station) return undefined;
    return {
      position: Cartesian3.fromDegrees(station.position.lon, station.position.lat, 0),
      symbolPx: 32,
    };
  }

  clear(): void {
    this.#entries.clear();
    this.#items.clear();
    this.#selectedId = null;
    this.#ctx.requestRender();
  }

  viewChanged(): void {}

  destroy(): void {
    this.clear();
    this.#primitives.remove(this.#points);
  }
}
