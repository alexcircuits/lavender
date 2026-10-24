/** What renderers share: resolved map colours, render requests and the camera's position. */
import { type Cartesian3, Color } from '@cesium/core';
import type { FeedId, FeedSnapshot } from '../../contracts';
import { DEFAULT_PALETTE, type MapPalette } from '../tokens';

export interface MapColors {
  /** The CSS strings, for canvas-drawn glyphs. */
  readonly css: MapPalette;
  readonly air: Color;
  readonly maritime: Color;
  readonly space: Color;
  readonly selection: Color;
  readonly outline: Color;
  readonly keyline: Color;
  readonly ink: Color;
  readonly boundaryIntl: Color;
  readonly boundaryDisputed: Color;
  readonly boundaryBreakaway: Color;
  readonly boundaryMaritime: Color;
  readonly geofence: Color;
  readonly hazardRamp: readonly [Color, Color, Color, Color, Color];
}

/** Cesium returns undefined (despite its typings) for strings it cannot parse: system
 *  colour keywords under forced colours, oklch(), … */
function tryParse(value: string): Color | undefined {
  const parsed: Color | undefined = Color.fromCssColorString(value);
  return parsed;
}

function parse(value: string, fallback: string): Color {
  return tryParse(value) ?? Color.fromCssColorString(fallback);
}

/** Cesium colours for a palette; values Cesium cannot parse fall back to the defaults. */
export function toMapColors(palette: MapPalette): MapColors {
  const d = DEFAULT_PALETTE;
  const ramp = palette.hazardRamp;
  const css: MapPalette = {
    ...palette,
    air: tryParse(palette.air) ? palette.air : d.air,
    maritime: tryParse(palette.maritime) ? palette.maritime : d.maritime,
    selection: tryParse(palette.selection) ? palette.selection : d.selection,
    outline: tryParse(palette.outline) ? palette.outline : d.outline,
  };
  return {
    css,
    air: parse(palette.air, d.air),
    maritime: parse(palette.maritime, d.maritime),
    space: parse(palette.space, d.space),
    selection: parse(palette.selection, d.selection),
    outline: parse(palette.outline, d.outline),
    keyline: parse(palette.keyline, d.keyline),
    ink: parse(palette.ink, d.ink),
    boundaryIntl: parse(palette.boundaryIntl, d.boundaryIntl),
    boundaryDisputed: parse(palette.boundaryDisputed, d.boundaryDisputed),
    boundaryBreakaway: parse(palette.boundaryBreakaway, d.boundaryBreakaway),
    boundaryMaritime: parse(palette.boundaryMaritime, d.boundaryMaritime),
    geofence: parse(palette.geofence, d.geofence),
    hazardRamp: [
      parse(ramp[0], d.hazardRamp[0]),
      parse(ramp[1], d.hazardRamp[1]),
      parse(ramp[2], d.hazardRamp[2]),
      parse(ramp[3], d.hazardRamp[3]),
      parse(ramp[4], d.hazardRamp[4]),
    ],
  };
}

export interface RenderContext {
  readonly colors: MapColors;
  requestRender(): void;
  /** Camera position in world coordinates (label distance ranking). */
  cameraPosition(): Cartesian3;
  /** Camera height above the ellipsoid in metres. */
  cameraHeightM(): number;
  /** The globe canvas in CSS pixels, for screen-space level of detail (vessel thinning). */
  canvasSizePx?(): { readonly width: number; readonly height: number };
}

/** Where the selection halo goes for an entity. */
export interface Located {
  readonly position: Cartesian3;
  readonly symbolPx: number;
}

export interface FeedRenderer {
  readonly feed: FeedId;
  /** Diffs the snapshot's items into the renderer's primitives (ignores other feeds). */
  apply(snapshot: FeedSnapshot): void;
  /** Removes every primitive (layer off). */
  clear(): void;
  setSelected(id: string | null): void;
  /** Current position and symbol size of an entity, or undefined when it is not drawn. */
  locate(id: string): Located | undefined;
  /** The camera settled somewhere new (labels, level of detail). */
  viewChanged(): void;
  destroy(): void;
  /** Invisible hit areas of this layer's symbols, if any (see `PickAssist`). */
  readonly pickAssists?: PickAssist;
}

/**
 * Near-invisible hit areas over the open parts of symbols (inside a quake ring, between
 * cyclone spiral arms), kept apart from the drawn symbols. A click consults them only when no
 * drawn symbol is under the cursor (interaction.ts), so whatever is drawn there wins even
 * when a nearer symbol's open centre covers it.
 */
export interface PickAssist {
  show: boolean;
}
