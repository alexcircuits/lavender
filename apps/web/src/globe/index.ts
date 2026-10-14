/**
 * The globe's public face. Everything here is engine-free: @cesium/engine is reached only
 * through the dynamic import below, so it lands in its own lazily loaded chunk and the shell
 * paints without it (docs/architecture/frontend.md §4, §7; scripts/check-dist.mjs verifies).
 *
 * `GlobeController` is the whole contract between the interface and the engine: the feed
 * controller pushes snapshots in, the shell drives the camera, and the engine reports picks,
 * view changes and the cursor position out. Nothing else crosses the boundary.
 */
import type {
  BBox,
  FeedId,
  FeedSnapshot,
  GeoPoint,
  ImageryDescriptor,
  SatelliteTrack,
} from '../contracts';

import type { AnnotationDraft, MapAnnotation } from '../features/annotations/model';

/** A selectable thing on the globe. */
export interface EntityRef {
  readonly feed: FeedId;
  readonly id: string;
}

/** What the camera currently shows. `bbox` is null when the view includes the horizon or
 *  space (the whole visible hemisphere), in which case feeds are queried worldwide. */
export interface ViewState {
  readonly bbox: BBox | null;
  readonly centre: GeoPoint | null;
  /** Camera height above the ellipsoid in metres. */
  readonly heightM: number;
}

/** Ellipsoid camera position in degrees/metres and orientation in radians. */
export interface CameraPose {
  readonly lon: number;
  readonly lat: number;
  readonly heightM: number;
  readonly headingRad: number;
  readonly pitchRad: number;
  readonly rollRad: number;
}

export type FlyTarget =
  | { readonly kind: 'point'; readonly position: GeoPoint; readonly heightM?: number }
  | { readonly kind: 'bbox'; readonly bbox: BBox };

export interface GlobeController {
  /** Replaces what the globe shows for `snapshot.feed`, updating primitives in place by id. */
  applySnapshot(snapshot: FeedSnapshot): void;
  /** Removes everything drawn for a feed (layer turned off). */
  clearFeed(feed: FeedId): void;
  /** Highlights one entity (lavender halo), or none. */
  setSelection(selection: EntityRef | null): void;
  /** Draws a satellite's ground track / orbit for the selected satellite, or clears it. */
  showTrack(track: SatelliteTrack | null): void;
  setAnnotations(annotations: readonly MapAnnotation[], draft: AnnotationDraft | null): void;
  /** Drawing owns clicks so a shape cannot accidentally select a feed entity. */
  setDrawing(active: boolean): void;
  onMapPoint(listener: (point: GeoPoint | null) => void): () => void;
  setImagery(imagery: ImageryDescriptor): void;
  flyTo(target: FlyTarget): void;
  home(): void;
  zoomIn(): void;
  zoomOut(): void;
  resetNorth(): void;
  cameraPose(): CameraPose | null;
  /** Ground under the viewport centre, or null when its ray misses the Earth. */
  centrePosition(): GeoPoint | null;
  /** Cuts to a validated shared view, cancelling any earlier flight. */
  restoreCamera(pose: CameraPose): void;
  /** Explicit commands and pointer/wheel input claim the camera before a delayed restore. */
  onCameraIntent(listener: () => void): () => void;
  /** Called with the picked entity, or null when empty globe is clicked. */
  onPick(listener: (entity: EntityRef | null) => void): () => void;
  /** Called (debounced by the engine) after the camera settles. */
  onViewChange(listener: (view: ViewState) => void): () => void;
  /** Called with the geographic position under the pointer (rAF-throttled), or null. */
  onCursor(listener: (position: GeoPoint | null) => void): () => void;
  /**
   * Diagnostics: renders a frame and returns it downscaled to at most `maxWidth` pixels,
   * with the share of lit pixels (rendering smoke checks; the desktop self-check). Null
   * when the globe cannot be read back.
   */
  captureFrame(maxWidth?: number): Promise<FrameCapture | null>;
  destroy(): void;
}

export interface FrameCapture {
  /** PNG data URL. */
  readonly dataUrl: string;
  readonly width: number;
  readonly height: number;
  /** Share of pixels brighter than near-black (0–1): a blank or frozen globe is ~0. */
  readonly litRatio: number;
}

export interface GlobeMountOptions {
  /** Element the canvas fills. Must carry the `lv-globe-host` class (base styles skip it). */
  readonly host: HTMLElement;
  /** Element that receives Cesium's credit display. */
  readonly credits: HTMLElement;
  /** Initial base imagery; the bundled offline layer is used when absent. */
  readonly imagery?: ImageryDescriptor;
  /** Render-loop failures are reported, never left as a silently frozen globe. */
  readonly onRenderError: (error: unknown) => void;
}

export type MountGlobe = (options: GlobeMountOptions) => Promise<GlobeController>;

declare global {
  interface Window {
    CESIUM_BASE_URL?: string;
  }
}

/** Loads the engine chunk on demand and mounts a globe into `host`. */
export const mountGlobe: MountGlobe = async (options) => {
  // Cesium resolves Workers, ThirdParty and Assets from here (copied to public/cesium).
  window.CESIUM_BASE_URL ??= `${import.meta.env.BASE_URL}cesium/`;
  const { GlobeEngine } = await import('./GlobeEngine');
  const engine = new GlobeEngine(options);
  engine.mount();
  return engine;
};
