/**
 * Earthquakes: a PointPrimitiveCollection; diameter by magnitude (clamped), colour by the
 * five-step hazard ramp, and the 1 px `--map-outline` every point symbol carries.
 */
import { Cartesian3, type Color, Ellipsoid } from '@cesium/core';
import {
  type PointPrimitive,
  PointPrimitiveCollection,
  type PrimitiveCollection,
} from '@cesium/engine';
import type { Earthquake, FeedSnapshot } from '../../contracts';
import type { EntityRef } from '../index';
import { KeyedEntries } from './keyed';
import { hasValidPosition, isStale, quakeRampIndex, quakeSizePx, STALE_ALPHA } from './symbology';
import type { FeedRenderer, Located, MapColors, RenderContext } from './types';

type Ramp = MapColors['hazardRamp'];

interface QuakeEntry {
  readonly point: PointPrimitive;
  magnitude: number;
}

const scratchPosition = new Cartesian3();

function positionOf(quake: Earthquake): Cartesian3 {
  const { lat, lon } = quake.position;
  return Cartesian3.fromDegrees(lon, lat, 0, Ellipsoid.WGS84, scratchPosition);
}

export class EarthquakeRenderer implements FeedRenderer {
  readonly feed = 'earthquakes' as const;
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #points: PointPrimitiveCollection;
  readonly #entries: KeyedEntries<Earthquake, QuakeEntry>;
  #alpha = 1;
  #ramp: Ramp;
  #outline: Color;

  constructor(primitives: PrimitiveCollection, ctx: RenderContext) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#points = primitives.add(new PointPrimitiveCollection()) as PointPrimitiveCollection;
    this.#ramp = ctx.colors.hazardRamp;
    this.#outline = ctx.colors.outline;
    this.#entries = new KeyedEntries<Earthquake, QuakeEntry>((q) => q.id, {
      create: (item, id) => this.#create(item, id),
      update: (entry, item) => this.#update(entry, item),
      remove: (entry) => {
        this.#points.remove(entry.point);
      },
    });
  }

  get size(): number {
    return this.#entries.size;
  }

  apply(snapshot: FeedSnapshot): void {
    if (snapshot.items.kind !== 'earthquakes') return;
    this.#setAlpha(isStale(snapshot.freshness) ? STALE_ALPHA : 1);
    this.#entries.apply(snapshot.items.items.filter(hasValidPosition));
    this.#ctx.requestRender();
  }

  clear(): void {
    if (this.#entries.clear() > 0) this.#ctx.requestRender();
  }

  setSelected(_id: string | null): void {}

  locate(id: string): Located | undefined {
    const entry = this.#entries.get(id);
    return entry ? { position: entry.point.position, symbolPx: entry.point.pixelSize } : undefined;
  }

  viewChanged(): void {}

  destroy(): void {
    this.#entries.clear();
    this.#primitives.remove(this.#points);
  }

  #create(item: Earthquake, id: string): QuakeEntry {
    const ref: EntityRef = Object.freeze({ feed: 'earthquakes', id });
    const point = this.#points.add({
      position: positionOf(item),
      pixelSize: quakeSizePx(item.magnitude),
      color: this.#ramp[quakeRampIndex(item.magnitude)],
      outlineColor: this.#outline,
      outlineWidth: 1,
      id: ref,
    });
    return { point, magnitude: item.magnitude };
  }

  #update(entry: QuakeEntry, item: Earthquake): void {
    entry.point.position = positionOf(item);
    if (item.magnitude !== entry.magnitude) {
      entry.magnitude = item.magnitude;
      entry.point.pixelSize = quakeSizePx(item.magnitude);
      entry.point.color = this.#ramp[quakeRampIndex(item.magnitude)];
    }
  }

  #setAlpha(alpha: number): void {
    if (alpha === this.#alpha) return;
    this.#alpha = alpha;
    const [r0, r1, r2, r3, r4] = this.#ctx.colors.hazardRamp;
    this.#ramp = [
      r0.withAlpha(alpha),
      r1.withAlpha(alpha),
      r2.withAlpha(alpha),
      r3.withAlpha(alpha),
      r4.withAlpha(alpha),
    ];
    this.#outline = this.#ctx.colors.outline.withAlpha(alpha);
    for (const [, entry] of this.#entries.entries()) {
      entry.point.color = this.#ramp[quakeRampIndex(entry.magnitude)];
      entry.point.outlineColor = this.#outline;
    }
  }
}
