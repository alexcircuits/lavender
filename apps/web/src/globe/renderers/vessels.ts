/**
 * Vessels (AIS): one BillboardCollection of runtime-drawn glyphs tinted `--map-maritime`,
 * updated in place by MMSI. A ship making way is a hull chevron turned to its course over
 * ground (true heading when no course is reported) relative to local north; a ship at rest or
 * with unknown speed is a dot (vesselSymbology.ts). Above `THIN_MIN_HEIGHT_M` one ship per
 * screen cell of the actual canvas is drawn; the feed's count is untouched. Names are budgeted
 * labels at close zoom, and the selected ship's recent track (`showTrack`) is a
 * `--map-selection` trail.
 *
 * Applying a snapshot is the hot path (≤ 8 ms for 10 000 diffs, docs/performance/
 * specification.md): a report that changes nothing a ship is drawn with costs no billboard
 * write, and a ship hidden by thinning only has its report remembered; its billboard is
 * written when it is drawn again.
 *
 * Nothing here animates: the globe renders when a snapshot, the selection or the view
 * changes what is drawn (request-render mode).
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
import type { FeedSnapshot, Vessel, VesselTrackResponse, VesselTrackSample } from '../../contracts';
import type { EntityRef } from '../index';
import { localNorth } from './aircraft';
import { GLYPH_SUPERSAMPLE } from './glyphs';
import { KeyedEntries } from './keyed';
import { surfaceArcPositions } from './surfaceArc';
import {
  chooseLabelled,
  hasValidPosition,
  isStale,
  type LabelCandidate,
  STALE_ALPHA,
  trackRotationRad,
} from './symbology';
import type { FeedRenderer, Located, RenderContext } from './types';
import { drawVesselGlyph, VESSEL_CHEVRON_PX, vesselGlyphId, vesselGlyphPx } from './vesselGlyphs';
import {
  thinningCellDeg,
  thinningKeepers,
  VESSEL_LABEL_BUDGET,
  VESSEL_LABEL_MAX_HEIGHT_M,
  type VesselGlyphKind,
  vesselLabel,
  vesselSymbol,
} from './vesselSymbology';

interface VesselEntry {
  /** The MMSI (thinning breaks ties by it). */
  readonly id: string;
  readonly ref: EntityRef;
  readonly billboard: Billboard;
  /** The latest report: position (with `id` and `underway`, a `ThinShip`) and symbol. */
  lat: number;
  lon: number;
  kind: VesselGlyphKind;
  bearingDeg: number | null;
  underway: boolean;
  /** The glyph the billboard shows (it lags `kind` while the ship is hidden). */
  drawnKind: VesselGlyphKind;
  /** Where the billboard's local-north axis was computed. */
  axisLat: number;
  axisLon: number;
  /** The billboard does not show the latest report yet: it arrived while hidden. */
  pending: boolean;
  /** The wire name the label text was made from. */
  name: string | undefined;
  text: string | null;
  label: Label | null;
  /** Hidden by low-zoom thinning. */
  thinned: boolean;
}

type Coordinate = readonly [number, number];

const LABEL_OFFSET = new Cartesian2(VESSEL_CHEVRON_PX / 2 + 2, 0);
const LABEL_BACKGROUND_ALPHA = 0.72;
const LABEL_PADDING = new Cartesian2(4, 2);
const TRACK_WIDTH_PX = 2;
/** Samples drawn from a track at most (the server keeps 64 per ship by default). */
const MAX_TRACK_SAMPLES = 1_000;
/** Interpolated trail vertices at most (surfaceArc budget). */
const MAX_TRACK_VERTICES = 20_000;
/**
 * A ship's local-north axis (what its chevron turns from) is recomputed once it has moved
 * this many degrees (about 1 km) from where it was last computed: the axis turns by about
 * 0.01° per kilometre, far below what a 16 px chevron can show.
 */
const NORTH_AXIS_REFRESH_DEG = 0.01;
const scratchPosition = new Cartesian3();
const scratchNorth = new Cartesian3();

/** Whether a vessel from the wire can be drawn: a non-empty MMSI and a valid position. */
export function isDrawableVessel(vessel: Vessel): boolean {
  return (
    typeof vessel.mmsi === 'string' &&
    vessel.mmsi.length > 0 &&
    typeof vessel.position === 'object' &&
    vessel.position !== null &&
    hasValidPosition(vessel) &&
    Math.abs(vessel.position.lon) <= 180
  );
}

function positionOf(lat: number, lon: number): Cartesian3 {
  return Cartesian3.fromDegrees(lon, lat, 0, Ellipsoid.WGS84, scratchPosition);
}

export class VesselRenderer implements FeedRenderer {
  readonly feed = 'vessels' as const;
  readonly #ctx: RenderContext;
  readonly #primitives: PrimitiveCollection;
  readonly #billboards: BillboardCollection;
  readonly #labels: LabelCollection;
  readonly #lines: PolylineCollection;
  readonly #entries: KeyedEntries<Vessel, VesselEntry>;
  /** Ships that carry a name label, so the budget pass need not visit every ship. */
  readonly #labelled = new Map<string, VesselEntry>();
  readonly #glyphs = new Map<VesselGlyphKind, HTMLCanvasElement>();
  /** The trail: one line per run of samples that surface arcs can join. */
  #track: Polyline[] = [];
  /** Ships hidden by thinning (none: the thinning pass can stop early at close zoom). */
  #thinnedCount = 0;
  /** Whether the snapshot being applied changed anything drawn. */
  #redraw = false;
  #stale = false;
  #selected: string | null = null;
  #tint: Color;
  #ink: Color;
  #outline: Color;

  constructor(primitives: PrimitiveCollection, ctx: RenderContext) {
    this.#ctx = ctx;
    this.#primitives = primitives;
    this.#lines = primitives.add(new PolylineCollection()) as PolylineCollection;
    this.#billboards = primitives.add(new BillboardCollection()) as BillboardCollection;
    this.#labels = primitives.add(new LabelCollection()) as LabelCollection;
    this.#tint = Color.clone(ctx.colors.maritime);
    this.#ink = Color.clone(ctx.colors.ink);
    this.#outline = Color.clone(ctx.colors.outline);
    this.#entries = new KeyedEntries<Vessel, VesselEntry>((v) => v.mmsi, {
      create: (item, id) => this.#create(item, id),
      update: (entry, item) => this.#update(entry, item),
      remove: (entry, id) => this.#remove(entry, id),
    });
  }

  get size(): number {
    return this.#entries.size;
  }

  /** Ships currently drawn, after thinning (for tests and diagnostics). */
  get drawn(): number {
    return this.#entries.size - this.#thinnedCount;
  }

  /** Names currently drawn (for tests and diagnostics). */
  labelledIds(): string[] {
    return [...this.#labelled.keys()];
  }

  /** The glyph a ship is drawn with, if it is in the renderer (for tests and diagnostics). */
  glyphOf(id: string): VesselGlyphKind | undefined {
    return this.#entries.get(id)?.kind;
  }

  apply(snapshot: FeedSnapshot): void {
    if (snapshot.items.kind !== 'vessels') return;
    this.#redraw = this.#setStale(isStale(snapshot.freshness));
    const diff = this.#entries.apply(snapshot.items.items.filter(isDrawableVessel));
    if (diff.added > 0 || diff.removed > 0) this.#redraw = true;
    if (this.#updateThinning()) this.#redraw = true;
    if (this.#updateLabels()) this.#redraw = true;
    // An unchanged picture (fixture replays, quiet seas) draws no new frame.
    if (this.#redraw) this.#ctx.requestRender();
  }

  clear(): void {
    const removed = this.#entries.clear();
    const hadTrack = this.#clearTrack();
    if (removed > 0 || hadTrack) this.#ctx.requestRender();
  }

  setSelected(id: string | null): void {
    this.#selected = id;
    this.#updateThinning();
    this.#updateLabels();
    this.#ctx.requestRender();
  }

  locate(id: string): Located | undefined {
    const entry = this.#entries.get(id);
    if (!entry || entry.thinned) return undefined;
    return { position: entry.billboard.position, symbolPx: vesselGlyphPx(entry.kind) };
  }

  viewChanged(): void {
    const thinned = this.#updateThinning();
    const labelled = this.#updateLabels();
    if (thinned || labelled) this.#ctx.requestRender();
  }

  /** Draws the selected ship's recent track as a trail, or clears it. */
  showTrack(track: VesselTrackResponse | null): void {
    const hadTrack = this.#clearTrack();
    if (track) {
      const ref: EntityRef = Object.freeze({ feed: 'vessels', id: track.mmsi });
      for (const positions of trailRuns(track)) {
        const line = this.#lines.add({
          positions,
          width: TRACK_WIDTH_PX,
          material: Material.fromType('Color', { color: this.#ctx.colors.selection }),
          id: ref,
        });
        this.#track.push(line);
      }
    }
    if (hadTrack || this.#track.length > 0) this.#ctx.requestRender();
  }

  destroy(): void {
    this.#entries.clear();
    this.#track = [];
    this.#primitives.remove(this.#billboards);
    this.#primitives.remove(this.#labels);
    this.#primitives.remove(this.#lines);
  }

  #clearTrack(): boolean {
    if (this.#track.length === 0) return false;
    for (const line of this.#track) this.#lines.remove(line);
    this.#track = [];
    return true;
  }

  #glyph(kind: VesselGlyphKind): HTMLCanvasElement {
    let glyph = this.#glyphs.get(kind);
    if (!glyph) {
      glyph = drawVesselGlyph(kind, this.#ctx.colors.css.outline);
      this.#glyphs.set(kind, glyph);
    }
    return glyph;
  }

  #create(item: Vessel, id: string): VesselEntry {
    const ref: EntityRef = Object.freeze({ feed: 'vessels', id });
    const { lat, lon } = item.position;
    const position = positionOf(lat, lon);
    const symbol = vesselSymbol(item);
    const billboard = this.#billboards.add({
      position,
      scale: 1 / GLYPH_SUPERSAMPLE,
      color: this.#tint,
      rotation: trackRotationRad(symbol.bearingDeg ?? undefined),
      alignedAxis: localNorth(position, scratchNorth),
      id: ref,
    });
    billboard.setImage(vesselGlyphId(symbol.kind), this.#glyph(symbol.kind));
    return {
      id,
      ref,
      billboard,
      lat,
      lon,
      kind: symbol.kind,
      bearingDeg: symbol.bearingDeg,
      underway: symbol.kind === 'underway',
      drawnKind: symbol.kind,
      axisLat: lat,
      axisLon: lon,
      pending: false,
      name: item.name,
      text: vesselLabel(item),
      label: null,
      thinned: false,
    };
  }

  /** Takes a report; the billboard is written only if the ship is drawn and it changed. */
  #update(entry: VesselEntry, item: Vessel): void {
    const { lat, lon } = item.position;
    const symbol = vesselSymbol(item);
    const renamed = item.name !== entry.name;
    if (
      !renamed &&
      lat === entry.lat &&
      lon === entry.lon &&
      symbol.kind === entry.kind &&
      symbol.bearingDeg === entry.bearingDeg
    ) {
      return;
    }
    entry.lat = lat;
    entry.lon = lon;
    entry.kind = symbol.kind;
    entry.bearingDeg = symbol.bearingDeg;
    entry.underway = symbol.kind === 'underway';
    if (renamed) {
      entry.name = item.name;
      entry.text = vesselLabel(item);
    }
    if (entry.thinned) {
      entry.pending = true;
      return;
    }
    this.#draw(entry);
    this.#redraw = true;
  }

  /** Writes the latest report to the ship's billboard and label. */
  #draw(entry: VesselEntry): void {
    const { billboard } = entry;
    const position = positionOf(entry.lat, entry.lon);
    // Cesium skips unchanged values, so an unchanged field costs no buffer write.
    billboard.position = position;
    if (
      Math.abs(entry.lat - entry.axisLat) > NORTH_AXIS_REFRESH_DEG ||
      Math.abs(entry.lon - entry.axisLon) > NORTH_AXIS_REFRESH_DEG
    ) {
      billboard.alignedAxis = localNorth(position, scratchNorth);
      entry.axisLat = entry.lat;
      entry.axisLon = entry.lon;
    }
    billboard.rotation = trackRotationRad(entry.bearingDeg ?? undefined);
    if (entry.drawnKind !== entry.kind) {
      entry.drawnKind = entry.kind;
      billboard.setImage(vesselGlyphId(entry.kind), this.#glyph(entry.kind));
    }
    if (entry.label) {
      entry.label.position = position;
      if (entry.text !== null) entry.label.text = entry.text;
    }
    entry.pending = false;
  }

  #remove(entry: VesselEntry, id: string): void {
    this.#billboards.remove(entry.billboard);
    if (entry.label) this.#labels.remove(entry.label);
    entry.label = null;
    this.#labelled.delete(id);
    if (entry.thinned) this.#thinnedCount -= 1;
  }

  /** Dims or restores every symbol; returns whether the stale state changed. */
  #setStale(stale: boolean): boolean {
    if (stale === this.#stale) return false;
    this.#stale = stale;
    const alpha = stale ? STALE_ALPHA : 1;
    this.#tint = this.#ctx.colors.maritime.withAlpha(alpha);
    this.#ink = this.#ctx.colors.ink.withAlpha(alpha);
    this.#outline = this.#ctx.colors.outline.withAlpha(alpha);
    this.#entries.forEach((entry) => {
      entry.billboard.color = this.#tint;
    });
    for (const { label } of this.#labelled.values()) {
      if (!label) continue;
      label.fillColor = this.#ink;
      label.outlineColor = this.#outline;
      label.backgroundColor = this.#labelBackground();
    }
    return true;
  }

  #labelBackground(): Color {
    return this.#ctx.colors.outline.withAlpha(
      LABEL_BACKGROUND_ALPHA * (this.#stale ? STALE_ALPHA : 1),
    );
  }

  /** Applies low-zoom thinning; returns whether any ship was shown or hidden. */
  #updateThinning(): boolean {
    const canvas = this.#ctx.canvasSizePx?.();
    const cellDeg = thinningCellDeg(
      this.#ctx.cameraHeightM(),
      canvas ? Math.max(canvas.width, canvas.height) : undefined,
    );
    if (cellDeg === null && this.#thinnedCount === 0) return false;
    let kept: ReadonlySet<VesselEntry> | null = null;
    if (cellDeg !== null) {
      const all: VesselEntry[] = [];
      this.#entries.forEach((entry) => {
        all.push(entry);
      });
      kept = thinningKeepers(all, cellDeg, this.#selected);
    }
    let changed = false;
    this.#entries.forEach((entry) => {
      const thinned = kept !== null && !kept.has(entry);
      if (thinned === entry.thinned) return;
      entry.thinned = thinned;
      this.#thinnedCount += thinned ? 1 : -1;
      if (!thinned && entry.pending) this.#draw(entry);
      entry.billboard.show = !thinned;
      changed = true;
    });
    return changed;
  }

  /** Applies the label budget; returns whether any label was added or removed. */
  #updateLabels(): boolean {
    const height = this.#ctx.cameraHeightM();
    const candidates: LabelCandidate[] = [];
    if (height <= VESSEL_LABEL_MAX_HEIGHT_M) {
      const camera = this.#ctx.cameraPosition();
      this.#entries.forEach((entry, id) => {
        if (entry.text === null || entry.thinned) return;
        candidates.push({
          id,
          distanceSq: Cartesian3.distanceSquared(camera, entry.billboard.position),
        });
      });
    } else if (this.#selected !== null && this.#entries.get(this.#selected)?.text) {
      candidates.push({ id: this.#selected, distanceSq: 0 });
    }
    const chosen = chooseLabelled(candidates, this.#selected, height, VESSEL_LABEL_BUDGET);
    let changed = false;
    for (const [id, entry] of this.#labelled) {
      if (chosen.has(id)) continue;
      if (entry.label) this.#labels.remove(entry.label);
      entry.label = null;
      this.#labelled.delete(id);
      changed = true;
    }
    for (const id of chosen) {
      const entry = this.#entries.get(id);
      if (!entry || entry.label || entry.text === null) continue;
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
      this.#labelled.set(id, entry);
      changed = true;
    }
    return changed;
  }
}

/** A track sample the trail can use (the response is checked, not trusted). */
function isUsableSample(sample: unknown): sample is VesselTrackSample {
  if (typeof sample !== 'object' || sample === null) return false;
  const { lat, lon, epochSec } = sample as Partial<Record<keyof VesselTrackSample, unknown>>;
  return (
    typeof lat === 'number' &&
    typeof lon === 'number' &&
    typeof epochSec === 'number' &&
    Number.isFinite(lat) &&
    Number.isFinite(lon) &&
    Number.isFinite(epochSec) &&
    Math.abs(lat) <= 90 &&
    Math.abs(lon) <= 180
  );
}

/**
 * The trail of a track: its usable samples, oldest first, joined by lifted surface arcs
 * (antimeridian crossings take the short way). A step no arc can draw, such as a position
 * glitch to the far side of the Earth, splits the trail there instead of dropping all of it,
 * so each run of joined samples is a line of its own. A ship that stayed in one place has no
 * trail.
 */
function trailRuns(track: VesselTrackResponse): Cartesian3[][] {
  if (!Array.isArray(track.samples)) return [];
  const coordinates: Coordinate[] = (track.samples as unknown[])
    .filter(isUsableSample)
    .sort((a, b) => a.epochSec - b.epochSec)
    .slice(-MAX_TRACK_SAMPLES)
    .map((s) => [s.lon, s.lat] as const);
  const budget = { remaining: MAX_TRACK_VERTICES };
  const whole = surfaceArcPositions(coordinates, budget);
  if (whole) return [whole];
  const runs: Cartesian3[][] = [];
  let run: Coordinate[] = [];
  const endRun = () => {
    const positions = surfaceArcPositions(run, budget);
    if (positions) runs.push(positions);
  };
  for (const coordinate of coordinates) {
    const previous = run[run.length - 1];
    const joins =
      previous === undefined ||
      surfaceArcPositions([previous, coordinate], { remaining: MAX_TRACK_VERTICES }) !== null ||
      (previous[0] === coordinate[0] && previous[1] === coordinate[1]);
    if (!joins) {
      endRun();
      run = [];
    }
    run.push(coordinate);
  }
  endRun();
  return runs;
}
