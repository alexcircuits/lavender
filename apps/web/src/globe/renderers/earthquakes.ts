/**
 * Earthquakes: one BillboardCollection of rings (design-system §4: quake = ring), the ring's
 * diameter by magnitude (6–24 px) and its colour by the five-step hazard ramp, inside the
 * two-tone hazard edge. Open centres keep overlapping events legible and read apart from the
 * filled fire cells and the cyclone spiral that share the ramp. A second collection holds one
 * near-invisible wash per ring, the layer's pick assists: a click inside a ring's open centre
 * selects it only when no drawn ring is under the cursor. Keyed in-place updates; the glyphs
 * only change with the size or step.
 */
import { Cartesian3, Color, Ellipsoid } from '@cesium/core';
import { type Billboard, BillboardCollection, type PrimitiveCollection } from '@cesium/engine';
import type { Earthquake, FeedSnapshot } from '../../contracts';
import type { EntityRef } from '../index';
import { GLYPH_SUPERSAMPLE } from './glyphs';
import {
  drawPickWash,
  drawQuakeRing,
  GlyphCache,
  hazardInk,
  pickWashGlyphId,
  quakeRingGlyphId,
  quakeRingGlyphPx,
} from './hazardGlyphs';
import { quakeRingPx } from './hazardShapes';
import { KeyedEntries } from './keyed';
import { hasValidPosition, isStale, quakeRampIndex, STALE_ALPHA } from './symbology';
import type { FeedRenderer, Located, RenderContext } from './types';

interface QuakeEntry {
  readonly billboard: Billboard;
  readonly wash: Billboard;
  ringPx: number;
  glyph: string;
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
  readonly #billboards: BillboardCollection;
  readonly #washes: BillboardCollection;
  readonly #entries: KeyedEntries<Earthquake, QuakeEntry>;
  readonly #glyphs = new GlyphCache();
  /** Glyphs carry their colours; the billboard colour only dims stale symbols. */
  #tint = Color.WHITE;

  constructor(primitives: PrimitiveCollection, ctx: RenderContext) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#billboards = primitives.add(new BillboardCollection()) as BillboardCollection;
    this.#washes = primitives.add(new BillboardCollection()) as BillboardCollection;
    this.#entries = new KeyedEntries<Earthquake, QuakeEntry>((q) => q.id, {
      create: (item, id) => this.#create(item, id),
      update: (entry, item) => this.#update(entry, item),
      remove: (entry) => {
        this.#billboards.remove(entry.billboard);
        this.#washes.remove(entry.wash);
      },
    });
  }

  get size(): number {
    return this.#entries.size;
  }

  /** The open-centre washes (types.ts `PickAssist`). */
  get pickAssists(): BillboardCollection {
    return this.#washes;
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

  /** The halo goes round the whole glyph, hazard edge included. */
  locate(id: string): Located | undefined {
    const entry = this.#entries.get(id);
    return entry
      ? { position: entry.billboard.position, symbolPx: quakeRingGlyphPx(entry.ringPx) }
      : undefined;
  }

  viewChanged(): void {}

  destroy(): void {
    this.#entries.clear();
    this.#primitives.remove(this.#billboards);
    this.#primitives.remove(this.#washes);
  }

  #create(item: Earthquake, id: string): QuakeEntry {
    const ref: EntityRef = Object.freeze({ feed: 'earthquakes', id });
    const options = {
      position: positionOf(item),
      scale: 1 / GLYPH_SUPERSAMPLE,
      color: this.#tint,
      id: ref,
    };
    const entry: QuakeEntry = {
      billboard: this.#billboards.add(options),
      wash: this.#washes.add(options),
      ringPx: 0,
      glyph: '',
    };
    this.#style(entry, item.magnitude);
    return entry;
  }

  #update(entry: QuakeEntry, item: Earthquake): void {
    const position = positionOf(item);
    entry.billboard.position = position;
    entry.wash.position = position;
    this.#style(entry, item.magnitude);
  }

  #style(entry: QuakeEntry, magnitude: number): void {
    const ringPx = quakeRingPx(magnitude);
    const step = quakeRampIndex(magnitude);
    const glyph = quakeRingGlyphId(ringPx, step);
    if (glyph === entry.glyph) return;
    const { colors } = this.#ctx;
    if (ringPx !== entry.ringPx) {
      const sidePx = quakeRingGlyphPx(ringPx);
      const wash = pickWashGlyphId(sidePx);
      entry.wash.setImage(
        wash,
        this.#glyphs.get(wash, () => drawPickWash(sidePx, colors.css.outline)),
      );
    }
    entry.ringPx = ringPx;
    entry.glyph = glyph;
    entry.billboard.setImage(
      glyph,
      this.#glyphs.get(glyph, () =>
        drawQuakeRing(ringPx, hazardInk(colors, colors.hazardRamp[step])),
      ),
    );
  }

  #setAlpha(alpha: number): void {
    if (alpha === this.#tint.alpha) return;
    this.#tint = Color.WHITE.withAlpha(alpha);
    for (const [, entry] of this.#entries.entries()) {
      entry.billboard.color = this.#tint;
      entry.wash.color = this.#tint;
    }
  }
}
