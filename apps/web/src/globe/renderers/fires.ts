/**
 * Active fires: one BillboardCollection of FireCell entities (NASA FIRMS), drawn as filled
 * square cells (design-system §4: fire = filled cell) so they read apart from the earthquake
 * rings and the cyclone spiral that share the hazard ramp. Grouped cells (count > 1) and single
 * detections (count = 1) are sized by detection count and peak FRP, coloured by total FRP on
 * the 5-step ramp, inside the two-tone hazard edge. Keyed in-place updates avoid primitive
 * churn. Cells that fail the shared fire validity rule (feeds/fireCells.ts) are skipped.
 */
import { Cartesian3, Color, Ellipsoid } from '@cesium/core';
import { type Billboard, BillboardCollection, type PrimitiveCollection } from '@cesium/engine';
import type { FeedSnapshot, FireCell } from '../../contracts';
import { fireCellId, isValidFireCell } from '../../feeds/fireCells';
import type { EntityRef } from '../index';
import { GLYPH_SUPERSAMPLE } from './glyphs';
import {
  drawFireCell,
  fireCellGlyphId,
  fireCellGlyphPx,
  GlyphCache,
  hazardInk,
} from './hazardGlyphs';
import { KeyedEntries } from './keyed';
import {
  FIRE_MAX_PX,
  FIRE_MIN_PX,
  fireRampIndex,
  fireSizePx,
  isStale,
  STALE_ALPHA,
} from './symbology';
import type { FeedRenderer, Located, RenderContext } from './types';

export { FIRE_MAX_PX, FIRE_MIN_PX, fireCellId, fireRampIndex, fireSizePx };

interface FireEntry {
  readonly billboard: Billboard;
  sidePx: number;
  glyph: string;
}

const scratchPosition = new Cartesian3();

function positionOf(cell: FireCell): Cartesian3 {
  return Cartesian3.fromDegrees(cell.lon, cell.lat, 0, Ellipsoid.WGS84, scratchPosition);
}

/** The cell's square side in whole CSS pixels (its glyph adds the hazard edge). */
function sideOf(cell: FireCell): number {
  return Math.round(fireSizePx(cell.count, cell.maxFrp));
}

export class FireRenderer implements FeedRenderer {
  readonly feed = 'fires' as const;
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #billboards: BillboardCollection;
  readonly #entries: KeyedEntries<FireCell, FireEntry>;
  readonly #glyphs = new GlyphCache();
  /** Glyphs carry their colours; the billboard colour only dims stale symbols. */
  #tint = Color.WHITE;

  constructor(primitives: PrimitiveCollection, ctx: RenderContext) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#billboards = primitives.add(new BillboardCollection()) as BillboardCollection;
    this.#entries = new KeyedEntries<FireCell, FireEntry>(fireCellId, {
      create: (item, id) => this.#create(item, id),
      update: (entry, item) => this.#update(entry, item),
      remove: (entry) => {
        this.#billboards.remove(entry.billboard);
      },
    });
  }

  get size(): number {
    return this.#entries.size;
  }

  apply(snapshot: FeedSnapshot): void {
    if (snapshot.items.kind !== 'fires') return;
    this.#setAlpha(isStale(snapshot.freshness) ? STALE_ALPHA : 1);
    // Items are untrusted wire data: a malformed cell is skipped, never the whole batch.
    this.#entries.apply(snapshot.items.items.filter(isValidFireCell));
    this.#ctx.requestRender();
  }

  clear(): void {
    if (this.#entries.clear() > 0) this.#ctx.requestRender();
  }

  setSelected(_id: string | null): void {}

  /** The halo is a ring: its diameter spans the square glyph's diagonal. */
  locate(id: string): Located | undefined {
    const entry = this.#entries.get(id);
    if (!entry) return undefined;
    const glyphPx = fireCellGlyphPx(entry.sidePx);
    return { position: entry.billboard.position, symbolPx: Math.ceil(glyphPx * Math.SQRT2) };
  }

  viewChanged(): void {}

  destroy(): void {
    this.#entries.clear();
    this.#primitives.remove(this.#billboards);
  }

  #create(item: FireCell, id: string): FireEntry {
    const ref: EntityRef = Object.freeze({ feed: 'fires', id });
    const billboard = this.#billboards.add({
      position: positionOf(item),
      scale: 1 / GLYPH_SUPERSAMPLE,
      color: this.#tint,
      id: ref,
    });
    const entry: FireEntry = { billboard, sidePx: 0, glyph: '' };
    this.#style(entry, item);
    return entry;
  }

  #update(entry: FireEntry, item: FireCell): void {
    entry.billboard.position = positionOf(item);
    this.#style(entry, item);
  }

  #style(entry: FireEntry, item: FireCell): void {
    const sidePx = sideOf(item);
    const step = fireRampIndex(item.frp);
    const glyph = fireCellGlyphId(sidePx, step);
    if (glyph === entry.glyph) return;
    entry.sidePx = sidePx;
    entry.glyph = glyph;
    const { colors } = this.#ctx;
    entry.billboard.setImage(
      glyph,
      this.#glyphs.get(glyph, () =>
        drawFireCell(sidePx, hazardInk(colors, colors.hazardRamp[step])),
      ),
    );
  }

  #setAlpha(alpha: number): void {
    if (alpha === this.#tint.alpha) return;
    this.#tint = Color.WHITE.withAlpha(alpha);
    for (const [, entry] of this.#entries.entries()) entry.billboard.color = this.#tint;
  }
}
