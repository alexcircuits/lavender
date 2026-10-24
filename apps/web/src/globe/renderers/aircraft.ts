/**
 * Aircraft: a BillboardCollection of runtime-drawn glyphs tinted `--map-air`, rotated to the
 * true track relative to local north, at geometric (else barometric) altitude. Military
 * contacts get the hollow delta glyph. Callsign labels are budgeted: the selected aircraft
 * always, otherwise the nearest `LABEL_BUDGET` and only below `LABEL_MAX_HEIGHT_M`.
 */
import { Cartesian2, Cartesian3, Color, Ellipsoid } from '@cesium/core';
import {
  type Billboard,
  BillboardCollection,
  HorizontalOrigin,
  type Label,
  LabelCollection,
  LabelStyle,
  Material,
  type Polyline,
  PolylineCollection,
  type PrimitiveCollection,
  VerticalOrigin,
} from '@cesium/engine';
import type { Aircraft, AircraftTrack, FeedSnapshot } from '../../contracts';
import type { EntityRef } from '../index';
import {
  AIRCRAFT_GLYPH_PX,
  type AircraftGlyphKind,
  aircraftGlyphId,
  drawAircraftGlyph,
  GLYPH_SUPERSAMPLE,
} from './glyphs';
import { KeyedEntries } from './keyed';
import {
  aircraftAltitudeM,
  aircraftLabel,
  chooseLabelled,
  hasValidPosition,
  isStale,
  LABEL_MAX_HEIGHT_M,
  type LabelCandidate,
  STALE_ALPHA,
  trackRotationRad,
} from './symbology';
import type { FeedRenderer, Located, RenderContext } from './types';

interface AircraftEntry {
  readonly ref: EntityRef;
  readonly billboard: Billboard;
  kind: AircraftGlyphKind;
  text: string | null;
  label: Label | null;
}

const LABEL_OFFSET = new Cartesian2(AIRCRAFT_GLYPH_PX / 2 + 4, 0);
/** Label backing: a translucent ground-colour pill keeps callsigns legible over bright
 *  imagery (city lights, cloud) where an outline alone does not. */
const LABEL_BACKGROUND_ALPHA = 0.72;
const LABEL_PADDING = new Cartesian2(4, 2);
const TRACK_WIDTH_PX = 2;
const scratchPosition = new Cartesian3();
const scratchNormal = new Cartesian3();
const scratchNorth = new Cartesian3();

/** Unit vector pointing to local (geodetic) north at `position`; zero at the poles. */
export function localNorth(position: Cartesian3, result: Cartesian3): Cartesian3 {
  const n = Ellipsoid.WGS84.geodeticSurfaceNormal(position, scratchNormal);
  result.x = -n.x * n.z;
  result.y = -n.y * n.z;
  result.z = 1 - n.z * n.z;
  const length = Cartesian3.magnitude(result);
  if (length < 1e-9) return Cartesian3.clone(Cartesian3.ZERO, result);
  return Cartesian3.divideByScalar(result, length, result);
}

function positionOf(aircraft: Aircraft): Cartesian3 {
  const { lat, lon } = aircraft.position;
  return Cartesian3.fromDegrees(
    lon,
    lat,
    aircraftAltitudeM(aircraft),
    Ellipsoid.WGS84,
    scratchPosition,
  );
}

const kindOf = (aircraft: Aircraft): AircraftGlyphKind =>
  aircraft.isMilitary ? 'military' : 'civil';

export class AircraftRenderer implements FeedRenderer {
  readonly feed = 'aircraft' as const;
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #billboards: BillboardCollection;
  readonly #labels: LabelCollection;
  readonly #lines: PolylineCollection;
  readonly #entries: KeyedEntries<Aircraft, AircraftEntry>;
  readonly #glyphs = new Map<AircraftGlyphKind, HTMLCanvasElement>();
  #track: Polyline | null = null;
  #stale = false;
  #selected: string | null = null;
  #tint: Color;
  #ink: Color;
  #outline: Color;

  constructor(primitives: PrimitiveCollection, ctx: RenderContext) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#billboards = primitives.add(new BillboardCollection()) as BillboardCollection;
    this.#labels = primitives.add(new LabelCollection()) as LabelCollection;
    this.#lines = primitives.add(new PolylineCollection()) as PolylineCollection;
    this.#tint = Color.clone(ctx.colors.air);
    this.#ink = Color.clone(ctx.colors.ink);
    this.#outline = Color.clone(ctx.colors.outline);
    this.#entries = new KeyedEntries<Aircraft, AircraftEntry>((a) => a.hex, {
      create: (item, id) => this.#create(item, id),
      update: (entry, item) => this.#update(entry, item),
      remove: (entry) => this.#remove(entry),
    });
  }

  get size(): number {
    return this.#entries.size;
  }

  /** Labels currently drawn (for tests and diagnostics). */
  labelledIds(): string[] {
    return [...this.#entries.entries()].filter(([, e]) => e.label !== null).map(([id]) => id);
  }

  apply(snapshot: FeedSnapshot): void {
    if (snapshot.items.kind !== 'aircraft') return;
    this.#setStale(isStale(snapshot.freshness));
    this.#entries.apply(snapshot.items.items.filter(hasValidPosition));
    this.#updateLabels();
    this.#ctx.requestRender();
  }

  clear(): void {
    const removed = this.#entries.clear();
    const hadTrack = this.#clearTrack();
    if (removed > 0 || hadTrack) this.#ctx.requestRender();
  }

  setSelected(id: string | null): void {
    this.#selected = id;
    this.#updateLabels();
    this.#ctx.requestRender();
  }

  locate(id: string): Located | undefined {
    const entry = this.#entries.get(id);
    return entry ? { position: entry.billboard.position, symbolPx: AIRCRAFT_GLYPH_PX } : undefined;
  }

  viewChanged(): void {
    if (this.#updateLabels()) this.#ctx.requestRender();
  }

  /** Draws the selected aircraft's recent track as a trail in selection colour, or clears it. */
  showTrack(track: AircraftTrack | null): void {
    const hadTrack = this.#clearTrack();
    if (track) {
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
      if (positions.length >= 2) {
        const ref: EntityRef = Object.freeze({ feed: 'aircraft', id: track.hex });
        this.#track = this.#lines.add({
          positions,
          width: TRACK_WIDTH_PX,
          material: Material.fromType('Color', { color: this.#ctx.colors.selection }),
          id: ref,
        });
      }
    }
    if (hadTrack || this.#track !== null) {
      this.#ctx.requestRender();
    }
  }

  destroy(): void {
    this.#entries.clear();
    this.#track = null;
    this.#primitives.remove(this.#billboards);
    this.#primitives.remove(this.#labels);
    this.#primitives.remove(this.#lines);
  }

  #clearTrack(): boolean {
    if (!this.#track) return false;
    this.#lines.remove(this.#track);
    this.#track = null;
    return true;
  }

  #glyph(kind: AircraftGlyphKind): HTMLCanvasElement {
    let glyph = this.#glyphs.get(kind);
    if (!glyph) {
      glyph = drawAircraftGlyph(kind, this.#ctx.colors.css.outline);
      this.#glyphs.set(kind, glyph);
    }
    return glyph;
  }

  #create(item: Aircraft, id: string): AircraftEntry {
    const ref: EntityRef = Object.freeze({ feed: 'aircraft', id });
    const position = positionOf(item);
    const billboard = this.#billboards.add({
      position,
      scale: 1 / GLYPH_SUPERSAMPLE,
      color: this.#tint,
      rotation: trackRotationRad(item.trackDeg),
      alignedAxis: localNorth(position, scratchNorth),
      id: ref,
    });
    const kind = kindOf(item);
    billboard.setImage(aircraftGlyphId(kind), this.#glyph(kind));
    return { ref, billboard, kind, text: aircraftLabel(item), label: null };
  }

  #update(entry: AircraftEntry, item: Aircraft): void {
    const { billboard } = entry;
    const position = positionOf(item);
    billboard.position = position;
    billboard.alignedAxis = localNorth(position, scratchNorth);
    billboard.rotation = trackRotationRad(item.trackDeg);
    const kind = kindOf(item);
    if (kind !== entry.kind) {
      entry.kind = kind;
      billboard.setImage(aircraftGlyphId(kind), this.#glyph(kind));
    }
    entry.text = aircraftLabel(item);
    if (entry.label) {
      entry.label.position = position;
      if (entry.text !== null) entry.label.text = entry.text;
    }
  }

  #remove(entry: AircraftEntry): void {
    this.#billboards.remove(entry.billboard);
    if (entry.label) this.#labels.remove(entry.label);
    entry.label = null;
  }

  #setStale(stale: boolean): void {
    if (stale === this.#stale) return;
    this.#stale = stale;
    const alpha = stale ? STALE_ALPHA : 1;
    this.#tint = this.#ctx.colors.air.withAlpha(alpha);
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
  }

  #labelBackground(): Color {
    return this.#ctx.colors.outline.withAlpha(
      LABEL_BACKGROUND_ALPHA * (this.#stale ? STALE_ALPHA : 1),
    );
  }

  /** Applies the label budget; returns whether any label was added or removed. */
  #updateLabels(): boolean {
    const height = this.#ctx.cameraHeightM();
    const candidates: LabelCandidate[] = [];
    if (height <= LABEL_MAX_HEIGHT_M) {
      const camera = this.#ctx.cameraPosition();
      for (const [id, entry] of this.#entries.entries()) {
        if (entry.text === null) continue;
        candidates.push({
          id,
          distanceSq: Cartesian3.distanceSquared(camera, entry.billboard.position),
        });
      }
    } else if (this.#selected !== null && this.#entries.get(this.#selected)?.text) {
      candidates.push({ id: this.#selected, distanceSq: 0 });
    }
    const chosen = chooseLabelled(candidates, this.#selected, height);
    let changed = false;
    for (const [id, entry] of this.#entries.entries()) {
      const wanted = entry.text !== null && chosen.has(id);
      if (wanted && entry.text !== null && !entry.label) {
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
