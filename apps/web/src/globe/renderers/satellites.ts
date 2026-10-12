/**
 * Satellites: elements → SGP4 (satellite.js) on a ~1 s timer rather than every frame, in
 * chunks so a large catalogue never blocks a frame; points in `--map-space`. The selected
 * satellite's orbit (`showTrack`) is a polyline in `--map-selection`. While satellites are
 * drawn the globe renders about once a second (they move); the timer runs only while there is
 * something to propagate, so with the layer off or empty an idle globe renders nothing
 * (request-render mode, docs/performance/specification.md: 0 idle frames).
 */
import { Cartesian3, type Color, Ellipsoid } from '@cesium/core';
import {
  Material,
  type PointPrimitive,
  PointPrimitiveCollection,
  type Polyline,
  PolylineCollection,
  type PrimitiveCollection,
} from '@cesium/engine';
import type { FeedSnapshot, SatelliteElements, SatelliteTrack } from '../../contracts';
import type { EntityRef } from '../index';
import { KeyedEntries } from './keyed';
import {
  type EcfMetres,
  elementsKey,
  propagateEcf,
  type SatRec,
  siderealTime,
  toSatrec,
} from './propagation';
import { isStale, STALE_ALPHA } from './symbology';
import type { FeedRenderer, Located, RenderContext } from './types';

export const SATELLITE_PX = 6;
export const PROPAGATION_INTERVAL_MS = 1000;
/** Satellites propagated per slice before yielding to the event loop. */
export const PROPAGATION_CHUNK = 1500;
const TRACK_WIDTH_PX = 1.5;

interface SatelliteEntry {
  readonly point: PointPrimitive;
  satrec: SatRec | null;
  key: string;
  removed: boolean;
}

export interface SatelliteRendererOptions {
  /** Called after each propagation pass moved the points (the halo follows). */
  readonly onMoved?: () => void;
  readonly now?: () => Date;
  readonly intervalMs?: number;
  readonly chunkSize?: number;
}

const scratchEcf: EcfMetres = { x: 0, y: 0, z: 0 };
const scratchPosition = new Cartesian3();

export class SatelliteRenderer implements FeedRenderer {
  readonly feed = 'satellites' as const;
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #points: PointPrimitiveCollection;
  readonly #lines: PolylineCollection;
  readonly #entries: KeyedEntries<SatelliteElements, SatelliteEntry>;
  readonly #onMoved: () => void;
  readonly #now: () => Date;
  readonly #intervalMs: number;
  readonly #chunkSize: number;
  #timer: ReturnType<typeof setInterval> | null = null;
  #slice: ReturnType<typeof setTimeout> | null = null;
  #pass: { cancelled: boolean } | null = null;
  #track: Polyline | null = null;
  #alpha = 1;
  #colour: Color;
  #outline: Color;

  constructor(
    primitives: PrimitiveCollection,
    ctx: RenderContext,
    options: SatelliteRendererOptions = {},
  ) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#points = primitives.add(new PointPrimitiveCollection()) as PointPrimitiveCollection;
    this.#lines = primitives.add(new PolylineCollection()) as PolylineCollection;
    this.#onMoved = options.onMoved ?? (() => {});
    this.#now = options.now ?? (() => new Date());
    this.#intervalMs = options.intervalMs ?? PROPAGATION_INTERVAL_MS;
    this.#chunkSize = Math.max(1, options.chunkSize ?? PROPAGATION_CHUNK);
    this.#colour = ctx.colors.space;
    this.#outline = ctx.colors.outline;
    this.#entries = new KeyedEntries<SatelliteElements, SatelliteEntry>((s) => String(s.noradId), {
      create: (item, id) => this.#create(item, id),
      update: (entry, item) => {
        const key = elementsKey(item);
        if (key !== entry.key) {
          entry.key = key;
          entry.satrec = toSatrec(item);
        }
      },
      remove: (entry) => {
        entry.removed = true;
        this.#points.remove(entry.point);
      },
    });
  }

  get size(): number {
    return this.#entries.size;
  }

  /** Whether the propagation timer is running (it must not when nothing is drawn). */
  get ticking(): boolean {
    return this.#timer !== null;
  }

  apply(snapshot: FeedSnapshot): void {
    if (snapshot.items.kind !== 'satellites') return;
    this.#setAlpha(isStale(snapshot.freshness) ? STALE_ALPHA : 1);
    this.#entries.apply(snapshot.items.items);
    this.#cancelPass();
    this.#syncTimer();
    if (this.#entries.size > 0) this.#runPass();
    this.#ctx.requestRender();
  }

  clear(): void {
    this.#entries.clear();
    this.#cancelPass();
    this.#syncTimer();
    this.showTrack(null);
    this.#ctx.requestRender();
  }

  setSelected(_id: string | null): void {}

  locate(id: string): Located | undefined {
    const entry = this.#entries.get(id);
    if (!entry?.point.show) return undefined;
    return { position: entry.point.position, symbolPx: SATELLITE_PX };
  }

  viewChanged(): void {}

  /** Draws the selected satellite's orbit (sub-satellite points at altitude), or clears it. */
  showTrack(track: SatelliteTrack | null): void {
    if (this.#track) {
      this.#lines.remove(this.#track);
      this.#track = null;
      this.#ctx.requestRender();
    }
    if (!track) return;
    const positions = track.points
      .filter((p) => Number.isFinite(p.position.lat) && Number.isFinite(p.position.lon))
      .map((p) =>
        Cartesian3.fromDegrees(
          p.position.lon,
          p.position.lat,
          p.position.altM ?? 0,
          Ellipsoid.WGS84,
        ),
      );
    if (positions.length < 2) return;
    const ref: EntityRef = Object.freeze({ feed: 'satellites', id: String(track.noradId) });
    this.#track = this.#lines.add({
      positions,
      width: TRACK_WIDTH_PX,
      material: Material.fromType('Color', { color: this.#ctx.colors.selection }),
      id: ref,
    });
    this.#ctx.requestRender();
  }

  destroy(): void {
    this.#entries.clear();
    this.#cancelPass();
    this.#syncTimer();
    this.#track = null;
    this.#primitives.remove(this.#points);
    this.#primitives.remove(this.#lines);
  }

  #create(item: SatelliteElements, id: string): SatelliteEntry {
    const ref: EntityRef = Object.freeze({ feed: 'satellites', id });
    const point = this.#points.add({
      show: false, // until the first propagation places it
      pixelSize: SATELLITE_PX,
      color: this.#colour,
      outlineColor: this.#outline,
      outlineWidth: 1,
      id: ref,
    });
    return { point, satrec: toSatrec(item), key: elementsKey(item), removed: false };
  }

  #syncTimer(): void {
    if (this.#entries.size > 0 && this.#timer === null) {
      this.#timer = setInterval(() => this.#tick(), this.#intervalMs);
    } else if (this.#entries.size === 0 && this.#timer !== null) {
      clearInterval(this.#timer);
      this.#timer = null;
    }
  }

  #tick(): void {
    // Hidden documents do not render; propagate again when visible.
    if (typeof document !== 'undefined' && document.hidden) return;
    this.#runPass();
  }

  #cancelPass(): void {
    if (this.#pass) this.#pass.cancelled = true;
    this.#pass = null;
    if (this.#slice !== null) clearTimeout(this.#slice);
    this.#slice = null;
  }

  /** One propagation of every satellite at a single instant, sliced into chunks. */
  #runPass(): void {
    if (this.#pass) return; // the previous pass is still slicing; this tick is skipped
    const date = this.#now();
    const gmst = siderealTime(date);
    const queue = [...this.#entries.entries()].map(([, entry]) => entry);
    const pass = { cancelled: false };
    this.#pass = pass;
    let index = 0;
    const step = () => {
      this.#slice = null;
      if (pass.cancelled) return;
      try {
        const end = Math.min(queue.length, index + this.#chunkSize);
        for (; index < end; index += 1) {
          const entry = queue[index];
          if (entry) this.#place(entry, date, gmst);
        }
        if (index < queue.length) {
          this.#slice = setTimeout(step, 0);
          return;
        }
      } catch (error) {
        console.error('Globe satellite propagation failed', error);
      }
      // Finished (or failed): the next tick starts a fresh pass either way.
      if (this.#pass === pass) this.#pass = null;
      this.#onMoved();
      this.#ctx.requestRender();
    };
    step();
  }

  #place(entry: SatelliteEntry, date: Date, gmst: number): void {
    if (entry.removed) return;
    if (!entry.satrec || !propagateEcf(entry.satrec, date, gmst, scratchEcf)) {
      entry.point.show = false;
      return;
    }
    scratchPosition.x = scratchEcf.x;
    scratchPosition.y = scratchEcf.y;
    scratchPosition.z = scratchEcf.z;
    entry.point.position = scratchPosition;
    entry.point.show = true;
  }

  #setAlpha(alpha: number): void {
    if (alpha === this.#alpha) return;
    this.#alpha = alpha;
    this.#colour = this.#ctx.colors.space.withAlpha(alpha);
    this.#outline = this.#ctx.colors.outline.withAlpha(alpha);
    for (const [, entry] of this.#entries.entries()) {
      entry.point.color = this.#colour;
      entry.point.outlineColor = this.#outline;
    }
  }
}
