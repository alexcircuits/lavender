// The globe engine. src/globe/** is the only place allowed to import @cesium/engine
// (ADR 0003/0004, enforced by Biome and src/architecture.test.ts); this module is reached
// exclusively through the dynamic import in ./index.ts, so the engine is its own lazy chunk.
//
// Composition only: imagery (imagery.ts), camera (camera.ts + cesiumCamera.ts), picking and
// cursor (interaction.ts), view normalisation (view.ts) and one renderer per feed
// (renderers/*). Explicit rendering throughout: nothing renders unless something changed.
import '@cesium/engine/Source/Widget/CesiumWidget.css';
import { Cartesian2, Cartesian3, Color, Rectangle } from '@cesium/core';
import {
  CesiumWidget,
  CreditDisplay,
  EllipsoidTerrainProvider,
  ImageryLayer,
  Ion,
} from '@cesium/engine';
import type {
  FeedId,
  FeedSnapshot,
  GeoPoint,
  ImageryDescriptor,
  SatelliteTrack,
} from '../contracts';
import { CameraService, HOME_VIEW, MAX_HEIGHT_M, prefersReducedMotion } from './camera';
import { captureFrame } from './capture';
import { cesiumCameraPort } from './cesiumCamera';
import { watchContextLoss } from './contextLoss';
import { BaseImagery, bundledProvider } from './imagery';
import type {
  EntityRef,
  FlyTarget,
  FrameCapture,
  GlobeController,
  GlobeMountOptions,
  ViewState,
} from './index';
import { geoPointAt, Interaction } from './interaction';
import { Listeners } from './listeners';
import { AircraftRenderer } from './renderers/aircraft';
import { EarthquakeRenderer } from './renderers/earthquakes';
import { SatelliteRenderer } from './renderers/satellites';
import { SelectionHalo } from './renderers/selection';
import { type FeedRenderer, type RenderContext, toMapColors } from './renderers/types';
import { cssTokenReader, readPalette } from './tokens';
import { toViewState } from './view';

/** Render errors tolerated in a burst before the engine stops restarting the loop. */
const MAX_RENDER_RESTARTS = 3;
/** An error this long after the previous one starts a new burst. */
const RESTART_WINDOW_MS = 60_000;
const MIN_ZOOM_DISTANCE_M = 100;

interface Mounted {
  readonly widget: CesiumWidget;
  readonly renderers: ReadonlyMap<FeedId, FeedRenderer>;
  readonly satellites: SatelliteRenderer;
  readonly halo: SelectionHalo;
  readonly imagery: BaseImagery;
  readonly camera: CameraService;
  readonly interaction: Interaction;
  readonly cleanup: Array<() => void>;
}

function groundColor(): Color {
  const value = cssTokenReader()('--ground');
  const parsed: Color | undefined = value ? Color.fromCssColorString(value) : undefined;
  return parsed ?? Color.BLACK;
}

export class GlobeEngine implements GlobeController {
  readonly #options: GlobeMountOptions;
  readonly #pickListeners = new Listeners<EntityRef | null>('pick');
  readonly #viewListeners = new Listeners<ViewState>('view');
  readonly #cursorListeners = new Listeners<GeoPoint | null>('cursor');
  #mounted: Mounted | null = null;
  #selection: EntityRef | null = null;
  #lastView: ViewState | null = null;
  #restarts = 0;
  #lastRenderErrorAt = 0;

  constructor(options: GlobeMountOptions) {
    this.#options = options;
  }

  mount(): void {
    if (this.#mounted) return;
    // No Cesium ion: every resource is local or from the imagery catalog (ADR 0008). The
    // default "Cesium ion" logo credit would misstate where the data comes from, so it is
    // removed; data attribution is Lavender's own (Sources panel, credits button).
    Ion.defaultAccessToken = '';
    CreditDisplay.cesiumCredit = undefined as unknown as typeof CreditDisplay.cesiumCredit;
    const widget = new CesiumWidget(this.#options.host, {
      baseLayer: ImageryLayer.fromProviderAsync(bundledProvider()),
      terrainProvider: new EllipsoidTerrainProvider(),
      creditContainer: this.#options.credits,
      skyBox: false,
      requestRenderMode: true,
      maximumRenderTimeChange: Number.POSITIVE_INFINITY,
      showRenderLoopErrors: false,
      useBrowserRecommendedResolution: true,
    });
    try {
      this.#mounted = this.#build(widget);
    } catch (error) {
      // Never leak a WebGL context and its render loop behind a failed mount.
      widget.destroy();
      throw error;
    }
    if (this.#options.imagery) this.setImagery(this.#options.imagery);
    widget.scene.requestRender();
  }

  #build(widget: CesiumWidget): Mounted {
    const scene = widget.scene;
    scene.backgroundColor = groundColor();
    scene.screenSpaceCameraController.minimumZoomDistance = MIN_ZOOM_DISTANCE_M;
    scene.screenSpaceCameraController.maximumZoomDistance = MAX_HEIGHT_M;
    // Start at home without a flight (Cesium's default view is over North America).
    scene.camera.setView({
      destination: Cartesian3.fromDegrees(HOME_VIEW.lon, HOME_VIEW.lat, HOME_VIEW.heightM),
      orientation: { heading: 0, pitch: -Math.PI / 2, roll: 0 },
    });

    const colors = toMapColors(readPalette(cssTokenReader()));
    const ctx: RenderContext = {
      colors,
      requestRender: () => scene.requestRender(),
      cameraPosition: () => scene.camera.positionWC,
      cameraHeightM: () => scene.camera.positionCartographic.height,
    };
    const primitives = scene.primitives;
    const satellites = new SatelliteRenderer(primitives, ctx, {
      onMoved: () => {
        if (this.#selection?.feed === 'satellites') this.#refreshHalo();
      },
    });
    const renderers = new Map<FeedId, FeedRenderer>([
      ['earthquakes', new EarthquakeRenderer(primitives, ctx)],
      ['satellites', satellites],
      ['aircraft', new AircraftRenderer(primitives, ctx)],
    ]);
    const cleanup: Array<() => void> = [];
    cleanup.push(scene.renderError.addEventListener((_s, error) => this.#renderError(error)));
    cleanup.push(scene.camera.moveEnd.addEventListener(() => this.#emitView()));
    const offFirstFrame = scene.postRender.addEventListener(() => {
      offFirstFrame();
      this.#emitView();
    });
    cleanup.push(offFirstFrame);
    cleanup.push(watchContextLoss(widget.canvas, { onLost: (error) => this.#report(error) }));

    return {
      widget,
      renderers,
      satellites,
      halo: new SelectionHalo(primitives, colors),
      imagery: new BaseImagery(scene, prefersReducedMotion),
      camera: new CameraService(cesiumCameraPort(scene), prefersReducedMotion),
      interaction: new Interaction(widget, {
        pick: (entity) => this.#pickListeners.emit(entity),
        cursor: (position) => this.#cursorListeners.emit(position),
      }),
      cleanup,
    };
  }

  applySnapshot(snapshot: FeedSnapshot): void {
    const m = this.#mounted;
    const renderer = m?.renderers.get(snapshot.feed);
    if (!m || !renderer) return;
    try {
      renderer.apply(snapshot);
    } catch (error) {
      console.error(`Globe could not draw the ${snapshot.feed} snapshot`, error);
    }
    if (this.#selection?.feed === snapshot.feed) this.#refreshHalo();
  }

  clearFeed(feed: FeedId): void {
    const m = this.#mounted;
    if (!m) return;
    m.renderers.get(feed)?.clear();
    if (this.#selection?.feed === feed) this.#refreshHalo();
    m.widget.scene.requestRender();
  }

  setSelection(selection: EntityRef | null): void {
    this.#selection = selection ? { feed: selection.feed, id: selection.id } : null;
    const m = this.#mounted;
    if (!m) return;
    for (const renderer of m.renderers.values()) {
      renderer.setSelected(renderer.feed === selection?.feed ? selection.id : null);
    }
    m.halo.set(this.#selection);
    this.#refreshHalo();
  }

  showTrack(track: SatelliteTrack | null): void {
    this.#mounted?.satellites.showTrack(track);
  }

  setImagery(imagery: ImageryDescriptor): void {
    this.#mounted?.imagery.show(imagery);
  }

  flyTo(target: FlyTarget): void {
    this.#mounted?.camera.flyTo(target);
  }

  home(): void {
    this.#mounted?.camera.home();
  }

  zoomIn(): void {
    this.#mounted?.camera.zoomIn();
  }

  zoomOut(): void {
    this.#mounted?.camera.zoomOut();
  }

  resetNorth(): void {
    this.#mounted?.camera.resetNorth();
  }

  onPick(listener: (entity: EntityRef | null) => void): () => void {
    return this.#pickListeners.add(listener);
  }

  /** Late subscribers get the current view once (asynchronously), then every settled change. */
  onViewChange(listener: (view: ViewState) => void): () => void {
    let active = true;
    const off = this.#viewListeners.add(listener);
    const last = this.#lastView;
    if (last) {
      queueMicrotask(() => {
        if (!active) return;
        try {
          listener(last);
        } catch (error) {
          console.error('Globe view listener failed', error);
        }
      });
    }
    return () => {
      active = false;
      off();
    };
  }

  onCursor(listener: (position: GeoPoint | null) => void): () => void {
    return this.#cursorListeners.add(listener);
  }

  captureFrame(maxWidth = 640): Promise<FrameCapture | null> {
    const m = this.#mounted;
    if (!m || m.widget.isDestroyed()) return Promise.resolve(null);
    return captureFrame(m.widget.scene, m.widget.canvas, maxWidth);
  }

  destroy(): void {
    const m = this.#mounted;
    this.#mounted = null;
    this.#pickListeners.clear();
    this.#viewListeners.clear();
    this.#cursorListeners.clear();
    if (!m) return;
    try {
      for (const c of m.cleanup.splice(0)) c();
      m.interaction.destroy();
      m.imagery.destroy();
      for (const renderer of m.renderers.values()) renderer.destroy();
      m.halo.destroy();
    } finally {
      if (!m.widget.isDestroyed()) m.widget.destroy();
    }
  }

  #refreshHalo(): void {
    const m = this.#mounted;
    if (!m) return;
    const selection = this.#selection;
    m.halo.follow(selection ? m.renderers.get(selection.feed)?.locate(selection.id) : undefined);
    m.widget.scene.requestRender();
  }

  /**
   * Runs from Cesium's moveEnd/postRender, which are raised outside its render-error guard:
   * an exception here would stop the render loop silently, so it is caught and reported.
   */
  #emitView(): void {
    const m = this.#mounted;
    if (!m) return;
    try {
      const scene = m.widget.scene;
      const canvas = scene.canvas;
      const centre = geoPointAt(
        m.widget,
        new Cartesian2(canvas.clientWidth / 2, canvas.clientHeight / 2),
      );
      const rectangle = scene.camera.computeViewRectangle(scene.ellipsoid, new Rectangle());
      const view = toViewState(rectangle, centre, scene.camera.positionCartographic.height);
      this.#lastView = view;
      for (const renderer of m.renderers.values()) renderer.viewChanged();
      this.#viewListeners.emit(view);
    } catch (error) {
      console.error('Globe could not compute the view', error);
    }
  }

  /** Reports to the shell; a throwing callback must not break the engine's own handling. */
  #report(error: unknown): void {
    try {
      this.#options.onRenderError(error);
    } catch (callbackError) {
      console.error('Globe onRenderError callback failed', callbackError);
    }
  }

  #renderError(error: unknown): void {
    this.#report(error);
    const widget = this.#mounted?.widget;
    if (!widget) return;
    const now = performance.now();
    if (now - this.#lastRenderErrorAt > RESTART_WINDOW_MS) this.#restarts = 0;
    this.#lastRenderErrorAt = now;
    if (this.#restarts >= MAX_RENDER_RESTARTS) return;
    // The default loop stops on the first error (spike finding 1); restart it, bounded. The
    // widget's own handler has just cleared its loop flags, but the running rAF chain still
    // schedules one more frame: setting `useDefaultRenderLoop` now would start a second chain.
    // Two frames later the old chain has seen the flag off and exited (CesiumWidget internals).
    this.#restarts += 1;
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        if (widget.isDestroyed() || this.#mounted?.widget !== widget) return;
        widget.useDefaultRenderLoop = true;
        widget.scene.requestRender();
      }),
    );
  }
}
