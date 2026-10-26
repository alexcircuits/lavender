/**
 * GNSS Navigation Integrity & Interference cell renderer.
 * Displays 0.5° × 0.5° geographic cells with center severity badge billboards
 * ("OK" for nominal low cells, "${Math.round(percentDegraded)}%" for medium/high anomalies).
 */
import { Cartesian3, Color } from '@cesium/core';
import { type Billboard, BillboardCollection, type PrimitiveCollection } from '@cesium/engine';
import type { FeedId } from '../../contracts';
import type { EntityRef } from '../index';
import { DEFAULT_FONT_SANS } from '../tokens';
import { GLYPH_SUPERSAMPLE } from './glyphs';
import { KeyedEntries } from './keyed';
import type { Located, MapColors, RenderContext } from './types';
import type { GnssCell, GnssLevel, GnssSnapshot } from './wasmGnss';

export type { GnssCell, GnssLevel, GnssSnapshot } from './wasmGnss';

export const GNSS_BADGE_PX = 28;
export const GNSS_SELECTED_BADGE_PX = 34;

export const GNSS_LEVEL_COLORS: Readonly<Record<GnssLevel, string>> = Object.freeze({
  low: '#3fb950',
  medium: '#e3b341',
  high: '#f85149',
});

/**
 * Formats the center badge text for a GNSS cell:
 * `"OK"` for `'low'` cells and `"${Math.round(cell.percentDegraded)}%"` for `'medium'` / `'high'`.
 */
export function gnssBadgeLabel(cell: Pick<GnssCell, 'level' | 'percentDegraded'>): string {
  if (cell.level === 'low') {
    return 'OK';
  }
  return `${Math.round(cell.percentDegraded)}%`;
}

/**
 * Resolves the CSS color string for a GNSS cell severity level using map palette tokens
 * with fallbacks to the standard GNSS integrity colors.
 */
export function gnssLevelCssColor(level: GnssLevel, colors?: MapColors): string {
  if (level === 'high') {
    return colors?.css.hazardRamp[4] ?? GNSS_LEVEL_COLORS.high;
  }
  if (level === 'medium') {
    return colors?.css.hazardRamp[2] ?? GNSS_LEVEL_COLORS.medium;
  }
  return colors?.css.maritime ?? GNSS_LEVEL_COLORS.low;
}

/**
 * Draws a 2× supersampled circular/pill center badge canvas for a GNSS cell billboard.
 */
export function drawGnssBadge(
  text: string,
  fillCss: string,
  outlineCss: string,
  inkCss = '#fafafa',
): HTMLCanvasElement {
  const canvas = document.createElement('canvas');
  const side = Math.ceil(GNSS_BADGE_PX * GLYPH_SUPERSAMPLE);
  canvas.width = side;
  canvas.height = side;
  const ctx = canvas.getContext('2d');
  if (!ctx) return canvas;

  const s = GLYPH_SUPERSAMPLE;
  const centre = side / 2;
  const radius = (GNSS_BADGE_PX / 2 - 2) * s;

  ctx.beginPath();
  ctx.arc(centre, centre, radius + 1.5 * s, 0, Math.PI * 2);
  ctx.fillStyle = outlineCss;
  ctx.fill();

  ctx.beginPath();
  ctx.arc(centre, centre, radius, 0, Math.PI * 2);
  ctx.fillStyle = fillCss;
  ctx.fill();

  ctx.font = `600 ${Math.round(9.5 * s)}px ${DEFAULT_FONT_SANS}`;
  ctx.fillStyle = inkCss;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText(text, centre, centre + 0.5 * s);

  return canvas;
}

interface GnssEntry {
  readonly billboard: Billboard;
  badgeText: string;
  level: GnssLevel;
}

export class GnssRenderer {
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #billboards: BillboardCollection;
  readonly #cells = new Map<string, GnssCell>();
  readonly #badgeCache = new Map<string, HTMLCanvasElement>();
  #selectedId: string | null = null;
  readonly #entries: KeyedEntries<GnssCell, GnssEntry>;

  constructor(
    primitives: PrimitiveCollection,
    ctx: RenderContext,
    billboards?: BillboardCollection,
  ) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#billboards = billboards ?? primitives.add(new BillboardCollection());

    this.#entries = new KeyedEntries<GnssCell, GnssEntry>((c) => c.id, {
      create: (cell, id) => {
        this.#cells.set(id, cell);
        const isSelected = id === this.#selectedId;
        const badgeText = gnssBadgeLabel(cell);
        const image = this.#getBadgeImage(cell.level, badgeText);
        const pos = Cartesian3.fromDegrees(cell.center.lon, cell.center.lat, 0);
        const color = isSelected
          ? this.#ctx.colors.selection
          : (Color.fromCssColorString(gnssLevelCssColor(cell.level, this.#ctx.colors)) ??
            Color.WHITE);
        const scale = (isSelected ? GNSS_SELECTED_BADGE_PX / GNSS_BADGE_PX : 1) / GLYPH_SUPERSAMPLE;

        const billboard = this.#billboards.add({
          position: pos,
          image,
          color,
          scale,
          id: { feed: 'gnss' as unknown as FeedId, id } satisfies EntityRef,
        });

        return { billboard, badgeText, level: cell.level };
      },
      update: (entry, cell, id) => {
        this.#cells.set(id, cell);
        const isSelected = id === this.#selectedId;
        const badgeText = gnssBadgeLabel(cell);
        const pos = Cartesian3.fromDegrees(cell.center.lon, cell.center.lat, 0);
        const color = isSelected
          ? this.#ctx.colors.selection
          : (Color.fromCssColorString(gnssLevelCssColor(cell.level, this.#ctx.colors)) ??
            Color.WHITE);
        const scale = (isSelected ? GNSS_SELECTED_BADGE_PX / GNSS_BADGE_PX : 1) / GLYPH_SUPERSAMPLE;

        entry.billboard.position = pos;
        entry.billboard.color = color;
        entry.billboard.scale = scale;

        if (entry.badgeText !== badgeText || entry.level !== cell.level) {
          entry.badgeText = badgeText;
          entry.level = cell.level;
          const image = this.#getBadgeImage(cell.level, badgeText);
          if (typeof entry.billboard.setImage === 'function') {
            entry.billboard.setImage(`lv-gnss-${cell.level}-${badgeText}`, image);
          } else {
            (entry.billboard as unknown as { image: HTMLCanvasElement }).image = image;
          }
        }
      },
      remove: (entry, id) => {
        this.#cells.delete(id);
        this.#billboards.remove(entry.billboard);
      },
    });
  }

  #getBadgeImage(level: GnssLevel, badgeText: string): HTMLCanvasElement {
    const key = `${level}:${badgeText}`;
    let cached = this.#badgeCache.get(key);
    if (!cached) {
      const fillCss = gnssLevelCssColor(level, this.#ctx.colors);
      const outlineCss = this.#ctx.colors.css.outline;
      const inkCss = this.#ctx.colors.css.ink;
      cached = drawGnssBadge(badgeText, fillCss, outlineCss, inkCss);
      this.#badgeCache.set(key, cached);
    }
    return cached;
  }

  setCells(cells: readonly GnssCell[]): void {
    this.#entries.apply(cells);
    this.#ctx.requestRender();
  }

  setSnapshot(snapshot: GnssSnapshot | null): void {
    this.setCells(snapshot?.cells ?? []);
  }

  apply(snapshotOrCells: GnssSnapshot | readonly GnssCell[]): void {
    if (Array.isArray(snapshotOrCells)) {
      this.setCells(snapshotOrCells);
    } else {
      this.setCells((snapshotOrCells as GnssSnapshot).cells);
    }
  }

  getCell(id: string): GnssCell | undefined {
    return this.#cells.get(id);
  }

  getCells(): GnssCell[] {
    return [...this.#cells.values()];
  }

  setSelected(id: string | null): void {
    if (this.#selectedId === id) return;
    this.#selectedId = id;
    for (const [cellId, entry] of this.#entries.entries()) {
      const cell = this.#cells.get(cellId);
      const isSelected = cellId === id;
      const color = isSelected
        ? this.#ctx.colors.selection
        : (Color.fromCssColorString(gnssLevelCssColor(cell?.level ?? 'low', this.#ctx.colors)) ??
          Color.WHITE);
      const scale = (isSelected ? GNSS_SELECTED_BADGE_PX / GNSS_BADGE_PX : 1) / GLYPH_SUPERSAMPLE;
      entry.billboard.color = color;
      entry.billboard.scale = scale;
    }
    this.#ctx.requestRender();
  }

  locate(id: string): Located | undefined {
    const cell = this.#cells.get(id);
    if (!cell) return undefined;
    return {
      position: Cartesian3.fromDegrees(cell.center.lon, cell.center.lat, 0),
      symbolPx: GNSS_SELECTED_BADGE_PX,
    };
  }

  viewChanged(): void {}

  clear(): void {
    this.#entries.clear();
    this.#cells.clear();
    this.#selectedId = null;
    this.#ctx.requestRender();
  }

  destroy(): void {
    this.#entries.clear();
    this.#cells.clear();
    this.#badgeCache.clear();
    this.#selectedId = null;
    this.#primitives.remove(this.#billboards);
  }
}
