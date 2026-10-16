/**
 * Launch pads: one billboard per pad, aggregating the launches that share it
 * (`groupLaunchPads`). The glyph is a small rocket-on-pad triangle tinted `--map-space` (the
 * Space family, design-system §4), so it reads apart from the satellites' points, with a count
 * badge when several launches share the pad and the 1 px `--map-outline` every point symbol
 * carries. Pad names are labelled for the selected pad always and, below
 * `LABEL_MAX_HEIGHT_M`, for the nearest `PAD_LABEL_BUDGET` pads.
 *
 * Picking a pad with one launch selects that launch; a busier pad selects the pad itself.
 * Entries are keyed by pad and updated in place; nothing redraws unless what it draws changed
 * (a refresh with the same pads asks for no frame), or the selection or the view did. Trajectories are deliberately not drawn (ENG-990 defers them).
 */
import { Cartesian2, Cartesian3, Color, Ellipsoid } from '@cesium/core';
import {
  type Billboard,
  BillboardCollection,
  HorizontalOrigin,
  type Label,
  LabelCollection,
  LabelStyle,
  type PrimitiveCollection,
  VerticalOrigin,
} from '@cesium/engine';
import type { FeedSnapshot } from '../../contracts';
import {
  groupLaunchPads,
  isPadId,
  type LaunchPadGroup,
  padPickId,
  wellFormedLaunches,
} from '../../lib/launches';
import type { EntityRef } from '../index';
import { DEFAULT_FONT_SANS } from '../tokens';
import { GLYPH_SUPERSAMPLE, haloSizePx } from './glyphs';
import { KeyedEntries } from './keyed';
import {
  chooseLabelled,
  isStale,
  LABEL_MAX_HEIGHT_M,
  type LabelCandidate,
  STALE_ALPHA,
} from './symbology';
import type { FeedRenderer, Located, RenderContext } from './types';

/** The glyph's canvas side in CSS pixels: the pad mark plus room for the count badge. */
export const PAD_GLYPH_PX = 24;
/**
 * The size the selection halo rings: the whole glyph, so a selected busy pad keeps its badge
 * readable inside the ring (mark and badge stay within 13 px of the centre).
 */
export const PAD_SYMBOL_PX = PAD_GLYPH_PX;
/**
 * The count badge: a circle at the glyph's upper right, in canvas CSS pixels, as large as the
 * selection halo's inner ring allows (it overlaps the mark's upper right, as badges do).
 */
export const PAD_BADGE = { x: 17.5, y: 7, radius: 5.5 } as const;
/** Badge text sizes: a single digit, and the narrower size "9+" needs to fit the circle. */
const BADGE_DIGIT_PX = 9;
const BADGE_MANY_PX = 8;
/** Unselected pads carrying a name label at once. */
export const PAD_LABEL_BUDGET = 12;
const LABEL_MAX_CHARS = 28;
/** Labels start just outside the selection halo's outer ring (`haloSizePx(PAD_SYMBOL_PX) / 2`). */
const LABEL_OFFSET = new Cartesian2(haloSizePx(PAD_SYMBOL_PX) / 2, 0);
const LABEL_BACKGROUND_ALPHA = 0.72;
const LABEL_PADDING = new Cartesian2(4, 2);

/** The count badge's text: none for a single launch, "9+" past nine. */
export function padBadge(count: number): string | null {
  if (!Number.isFinite(count) || count <= 1) return null;
  return count > 9 ? '9+' : String(Math.floor(count));
}

/** The billboard image id for a pad glyph (one atlas entry per badge). */
export function padGlyphId(count: number): string {
  return `lv-launch-pad-${padBadge(count) ?? '1'}`;
}

/**
 * The badge's face for `badge`: the globe label's family (`labelFont` is "500 12px <family>"),
 * bold, 9 px for a digit and 8 px for "9+".
 */
export function badgeFont(labelFont: string, badge: string): string {
  const family = labelFont.replace(/^\s*\d+\s+[\d.]+px\s+/, '').trim();
  const size = badge.length > 1 ? BADGE_MANY_PX : BADGE_DIGIT_PX;
  return `600 ${size}px ${family || DEFAULT_FONT_SANS}`;
}

/** The label text for a pad: its name, shortened for the globe. */
export function padLabel(group: Pick<LaunchPadGroup, 'name'>): string {
  const name = group.name.trim();
  return name.length > LABEL_MAX_CHARS ? `${name.slice(0, LABEL_MAX_CHARS - 1)}…` : name;
}

/**
 * The pad glyph, drawn white (tinted by the billboard) with a dark `outline`: an upward
 * triangle standing on a pad bar, and for `count > 1` a dark badge with the count in white,
 * in the globe label's family (`labelFont`).
 */
export function drawPadGlyph(count: number, outline: string, labelFont: string): HTMLCanvasElement {
  const canvas = document.createElement('canvas');
  const s = GLYPH_SUPERSAMPLE;
  canvas.width = PAD_GLYPH_PX * s;
  canvas.height = PAD_GLYPH_PX * s;
  const ctx = canvas.getContext('2d');
  if (!ctx) return canvas;
  ctx.scale(s, s);
  ctx.lineJoin = 'round';
  ctx.strokeStyle = outline;
  ctx.lineWidth = 2; // 1 px shows outside the fill
  ctx.fillStyle = '#ffffff';
  ctx.beginPath();
  ctx.moveTo(12, 4);
  ctx.lineTo(18, 16);
  ctx.lineTo(6, 16);
  ctx.closePath();
  ctx.stroke();
  ctx.fill();
  ctx.beginPath();
  ctx.rect(4.5, 18, 15, 2);
  ctx.stroke();
  ctx.fill();
  const badge = padBadge(count);
  if (badge) {
    ctx.beginPath();
    ctx.arc(PAD_BADGE.x, PAD_BADGE.y, PAD_BADGE.radius, 0, Math.PI * 2);
    ctx.fillStyle = outline;
    ctx.fill();
    ctx.fillStyle = '#ffffff';
    ctx.font = badgeFont(labelFont, badge);
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(badge, PAD_BADGE.x, PAD_BADGE.y + 0.5);
  }
  return canvas;
}

interface PadEntry {
  ref: EntityRef;
  readonly billboard: Billboard;
  count: number;
  text: string;
  label: Label | null;
}

const scratchPosition = new Cartesian3();

function positionOf(group: LaunchPadGroup): Cartesian3 {
  return Cartesian3.fromDegrees(
    group.longitude,
    group.latitude,
    0,
    Ellipsoid.WGS84,
    scratchPosition,
  );
}

const refOf = (group: LaunchPadGroup): EntityRef =>
  Object.freeze({ feed: 'launches', id: padPickId(group) });

export class LaunchPadRenderer implements FeedRenderer {
  readonly feed = 'launches' as const;
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #billboards: BillboardCollection;
  readonly #labels: LabelCollection;
  readonly #entries: KeyedEntries<LaunchPadGroup, PadEntry>;
  readonly #glyphs = new Map<string, HTMLCanvasElement>();
  /** Launch id → the pad it launches from, for selection and the halo. */
  #padOfLaunch = new Map<string, string>();
  #selectedId: string | null = null;
  #stale = false;
  /** Whether the snapshot being applied changed anything drawn (set by the entry handlers). */
  #changed = false;
  #tint: Color;
  #ink: Color;
  #outline: Color;

  constructor(primitives: PrimitiveCollection, ctx: RenderContext) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#billboards = primitives.add(new BillboardCollection()) as BillboardCollection;
    this.#labels = primitives.add(new LabelCollection()) as LabelCollection;
    this.#tint = Color.clone(ctx.colors.space);
    this.#ink = Color.clone(ctx.colors.ink);
    this.#outline = Color.clone(ctx.colors.outline);
    this.#entries = new KeyedEntries<LaunchPadGroup, PadEntry>((group) => group.id, {
      create: (group) => this.#create(group),
      update: (entry, group) => this.#update(entry, group),
      remove: (entry) => this.#remove(entry),
    });
  }

  /** Pads drawn. */
  get size(): number {
    return this.#entries.size;
  }

  /** Pads currently labelled (tests and diagnostics). */
  labelledIds(): string[] {
    return [...this.#entries.entries()].filter(([, e]) => e.label !== null).map(([id]) => id);
  }

  /** Draws a snapshot; a refresh that changes nothing drawn asks for no frame. */
  apply(snapshot: FeedSnapshot): void {
    if (snapshot.items.kind !== 'launches') return;
    this.#changed = this.#setStale(isStale(snapshot.freshness));
    const groups = groupLaunchPads(wellFormedLaunches(snapshot.items.items));
    this.#entries.apply(groups);
    this.#padOfLaunch = new Map(
      groups.flatMap((group) => group.launches.map((launch) => [launch.id, group.id] as const)),
    );
    if (this.#updateLabels()) this.#changed = true;
    if (this.#changed) this.#ctx.requestRender();
    this.#changed = false;
  }

  clear(): void {
    this.#padOfLaunch = new Map();
    if (this.#entries.clear() > 0) this.#ctx.requestRender();
  }

  setSelected(id: string | null): void {
    this.#selectedId = id;
    if (this.#updateLabels()) this.#ctx.requestRender();
  }

  /** A pad by its id, or the pad a launch launches from. */
  locate(id: string): Located | undefined {
    const padId = this.#padIdOf(id);
    const entry = padId === null ? undefined : this.#entries.get(padId);
    return entry ? { position: entry.billboard.position, symbolPx: PAD_SYMBOL_PX } : undefined;
  }

  viewChanged(): void {
    if (this.#updateLabels()) this.#ctx.requestRender();
  }

  destroy(): void {
    this.#entries.clear();
    this.#primitives.remove(this.#billboards);
    this.#primitives.remove(this.#labels);
  }

  #padIdOf(id: string): string | null {
    return isPadId(id) ? id : (this.#padOfLaunch.get(id) ?? null);
  }

  #glyph(count: number): HTMLCanvasElement {
    const id = padGlyphId(count);
    let glyph = this.#glyphs.get(id);
    if (!glyph) {
      glyph = drawPadGlyph(count, this.#ctx.colors.css.outline, this.#ctx.colors.css.labelFont);
      this.#glyphs.set(id, glyph);
    }
    return glyph;
  }

  #create(group: LaunchPadGroup): PadEntry {
    const ref = refOf(group);
    const count = group.launches.length;
    const billboard = this.#billboards.add({
      position: positionOf(group),
      scale: 1 / GLYPH_SUPERSAMPLE,
      color: this.#tint,
      id: ref,
    });
    billboard.setImage(padGlyphId(count), this.#glyph(count));
    this.#changed = true;
    return { ref, billboard, count, text: padLabel(group), label: null };
  }

  /** A pad keeps its place; its launches (and so the pick target and badge) can change. */
  #update(entry: PadEntry, group: LaunchPadGroup): void {
    const position = positionOf(group);
    if (!Cartesian3.equals(entry.billboard.position, position)) {
      entry.billboard.position = position;
      if (entry.label) entry.label.position = position;
      this.#changed = true;
    }
    // The pick target changes nothing drawn: no frame for it alone.
    if (padPickId(group) !== entry.ref.id) {
      entry.ref = refOf(group);
      entry.billboard.id = entry.ref;
      if (entry.label) entry.label.id = entry.ref;
    }
    const count = group.launches.length;
    if (padGlyphId(count) !== padGlyphId(entry.count)) {
      entry.billboard.setImage(padGlyphId(count), this.#glyph(count));
      this.#changed = true;
    }
    entry.count = count;
    const text = padLabel(group);
    if (text !== entry.text) {
      entry.text = text;
      if (entry.label) entry.label.text = text;
      this.#changed = true;
    }
  }

  #remove(entry: PadEntry): void {
    this.#billboards.remove(entry.billboard);
    if (entry.label) this.#labels.remove(entry.label);
    entry.label = null;
    this.#changed = true;
  }

  /** Dims or restores every pad; returns whether that changed. */
  #setStale(stale: boolean): boolean {
    if (stale === this.#stale) return false;
    this.#stale = stale;
    const alpha = stale ? STALE_ALPHA : 1;
    this.#tint = this.#ctx.colors.space.withAlpha(alpha);
    this.#ink = this.#ctx.colors.ink.withAlpha(alpha);
    this.#outline = this.#ctx.colors.outline.withAlpha(alpha);
    for (const [, entry] of this.#entries.entries()) {
      entry.billboard.color = this.#tint;
      if (entry.label) {
        entry.label.fillColor = this.#ink;
        entry.label.outlineColor = this.#outline;
        entry.label.backgroundColor = this.#labelBackground();
      }
    }
    return true;
  }

  #labelBackground(): Color {
    return this.#ctx.colors.outline.withAlpha(
      LABEL_BACKGROUND_ALPHA * (this.#stale ? STALE_ALPHA : 1),
    );
  }

  /** Applies the label budget; returns whether any label was added or removed. */
  #updateLabels(): boolean {
    const selectedPad = this.#selectedId === null ? null : this.#padIdOf(this.#selectedId);
    const height = this.#ctx.cameraHeightM();
    const candidates: LabelCandidate[] = [];
    if (height <= LABEL_MAX_HEIGHT_M) {
      const camera = this.#ctx.cameraPosition();
      for (const [id, entry] of this.#entries.entries()) {
        candidates.push({
          id,
          distanceSq: Cartesian3.distanceSquared(camera, entry.billboard.position),
        });
      }
    } else if (selectedPad !== null && this.#entries.get(selectedPad)) {
      candidates.push({ id: selectedPad, distanceSq: 0 });
    }
    const chosen = chooseLabelled(candidates, selectedPad, height, PAD_LABEL_BUDGET);
    let changed = false;
    for (const [id, entry] of this.#entries.entries()) {
      const wanted = entry.text !== '' && chosen.has(id);
      if (wanted && !entry.label) {
        entry.label = this.#labels.add({
          position: entry.billboard.position,
          text: entry.text,
          font: this.#ctx.colors.css.labelFont,
          fillColor: this.#ink,
          outlineColor: this.#outline,
          outlineWidth: 2,
          style: LabelStyle.FILL_AND_OUTLINE,
          showBackground: true,
          backgroundColor: this.#labelBackground(),
          backgroundPadding: LABEL_PADDING,
          horizontalOrigin: HorizontalOrigin.LEFT,
          verticalOrigin: VerticalOrigin.CENTER,
          pixelOffset: LABEL_OFFSET,
          id: entry.ref,
        });
        changed = true;
      } else if (!wanted && entry.label) {
        this.#labels.remove(entry.label);
        entry.label = null;
        changed = true;
      }
    }
    return changed;
  }
}
