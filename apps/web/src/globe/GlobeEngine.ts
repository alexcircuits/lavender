// The globe engine. src/globe/** is the only place allowed to import @cesium/engine
// (ADR 0003/0004, enforced by Biome and src/architecture.test.ts); this module is reached
// exclusively through the dynamic import in ./index.ts, so the engine is its own lazy chunk.
//
// Composition only: imagery (imagery.ts), camera (camera.ts + cesiumCamera.ts), picking and
// cursor (interaction.ts), view normalisation (view.ts) and one renderer per feed
// (renderers/*). Explicit rendering throughout: nothing renders unless something changed.
import '@cesium/engine/Source/Widget/CesiumWidget.css';
import {
  Cartesian2,
  Cartesian3,
  Math as CesiumMath,
  Color,
  Ellipsoid,
  Rectangle,
} from '@cesium/core';
import {
  CesiumWidget,
  CreditDisplay,
  EllipsoidTerrainProvider,
  ImageryLayer,
  Ion,
  PointPrimitiveCollection,
  PolylineCollection,
} from '@cesium/engine';
import type {
  AircraftTrack,
  BoundaryFeature,
  CameraSource,
  CctvDetectionsResponse,
  FeedId,
  FeedSnapshot,
  GeoPoint,
  ImageryDescriptor,
  Route,
  SatelliteTrack,
  TransitTrail,
  VesselTrackResponse,
} from '../contracts';
import type { AnnotationDraft, MapAnnotation } from '../features/annotations/model';
import { CameraService, HOME_VIEW, MAX_HEIGHT_M, prefersReducedMotion } from './camera';
import { captureFrame } from './capture';
import { cesiumCameraPort } from './cesiumCamera';
import { watchContextLoss } from './contextLoss';
import { BaseImagery, bundledProvider } from './imagery';
import type {
  CameraPose,
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
import { AnnotationRenderer } from './renderers/annotations';
import { BikeshareRenderer } from './renderers/bikeshare';
import {
  type BoundaryClassVisibility,
  BoundaryRenderer,
  DEFAULT_BOUNDARY_VISIBILITY,
} from './renderers/boundaries';
import { CableRenderer, type SubmarineCableItem } from './renderers/cables';
import { CctvRenderer } from './renderers/cctv';
import { CycloneRenderer } from './renderers/cyclones';
import { DirectionsRenderer } from './renderers/directions';
import { EarthquakeRenderer } from './renderers/earthquakes';
import { FireRenderer } from './renderers/fires';
import { LaunchPadRenderer } from './renderers/launches';
import { PerimeterRenderer } from './renderers/perimeters';
import { SatelliteRenderer } from './renderers/satellites';
import { SelectionHalo } from './renderers/selection';
import { aircraftAltitudeM, hasValidPosition } from './renderers/symbology';
import { TransitRenderer } from './renderers/transit';
import {
  type FeedRenderer,
  type Located,
  type RenderContext,
  toMapColors,
} from './renderers/types';
import { VesselRenderer } from './renderers/vessels';
import { cssTokenReader, readPalette } from './tokens';
import { EntityTrackingService, type TrackingState } from './tracking';
import { sameView, toViewState, viewEdgeSamples } from './view';

/** Render errors tolerated in a burst before the engine stops restarting the loop. */
const MAX_RENDER_RESTARTS = 3;
/** An error this long after the previous one starts a new burst. */
const RESTART_WINDOW_MS = 60_000;
const MIN_ZOOM_DISTANCE_M = 100;

interface Mounted {
  readonly widget: CesiumWidget;
  readonly renderers: ReadonlyMap<FeedId, FeedRenderer>;
  readonly aircraft: AircraftRenderer;
  readonly satellites: SatelliteRenderer;
  readonly vessels: VesselRenderer;
  readonly transit: TransitRenderer;
  readonly bikeshare: BikeshareRenderer;
  readonly boundaries: BoundaryRenderer;
  readonly cables: CableRenderer;
  readonly cctv: CctvRenderer;
  readonly directions: DirectionsRenderer;
  readonly tracking: EntityTrackingService;
  readonly annotations: AnnotationRenderer;
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
  readonly #cameraIntentListeners = new Listeners<void>('camera intent');
  readonly #followListeners = new Listeners<TrackingState>('follow');
  #following: EntityRef | null = null;
  #mounted: Mounted | null = null;
  readonly #pointListeners = new Listeners<GeoPoint | null>('map point');
  #drawing = false;
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
      canvasSizePx: () => ({ width: scene.canvas.clientWidth, height: scene.canvas.clientHeight }),
    };
    const primitives = scene.primitives;
    const satellites = new SatelliteRenderer(primitives, ctx, {
      onMoved: () => {
        if (this.#selection?.feed === 'satellites') this.#refreshHalo();
        if (this.#following?.feed === 'satellites') {
          const located = satellites.locate(this.#following.id);
          if (located) {
            tracking.updateTargetPosition(located.position);
          }
        }
      },
    });
    const vessels = new VesselRenderer(primitives, ctx);
    const aircraft = new AircraftRenderer(primitives, ctx);
    const transit = new TransitRenderer(primitives, ctx);
    const bikeshare = new BikeshareRenderer(primitives, ctx);
    const boundaryPolylines = primitives.add(new PolylineCollection());
    const boundaries = new BoundaryRenderer(boundaryPolylines, ctx);
    const cablePolylines = primitives.add(new PolylineCollection());
    const cablePoints = primitives.add(new PointPrimitiveCollection());
    const cables = new CableRenderer(cablePolylines, cablePoints, ctx);
    const cctv = new CctvRenderer(primitives, ctx);
    const directions = new DirectionsRenderer(primitives, ctx);
    const renderers = new Map<FeedId, FeedRenderer>([
      ['earthquakes', new EarthquakeRenderer(primitives, ctx)],
      ['cyclones', new CycloneRenderer(primitives, ctx)],
      ['satellites', satellites],
      ['aircraft', aircraft],
      ['fires', new FireRenderer(primitives, ctx)],
      ['launches', new LaunchPadRenderer(primitives, ctx)],
      ['vessels', vessels],
      ['perimeters', new PerimeterRenderer(primitives, ctx)],
      ['bikeshare', bikeshare],
      ['transit', transit],
    ]);
    const tracking = new EntityTrackingService({
      camera: scene.camera,
      prefersReducedMotion,
      requestRender: () => scene.requestRender(),
    });
    tracking.onStateChanged((state) => {
      this.#followListeners.emit(state);
      if (state.status === 'released') {
        this.#following = null;
      }
    });
    const cleanup: Array<() => void> = [];
    cleanup.push(() => tracking.destroy());
    cleanup.push(() => boundaries.destroy());
    cleanup.push(() => cables.destroy());
    cleanup.push(() => cctv.destroy());
    cleanup.push(() => directions.destroy());
    if (scene.preUpdate?.addEventListener) {
      cleanup.push(
        scene.preUpdate.addEventListener(() => {
          if (this.isFollowing() && !scene.isDestroyed()) {
            tracking.tick(performance.now());
          }
        }),
      );
    }
    cleanup.push(
      scene.postRender.addEventListener(() => {
        if (this.isFollowing() && !scene.isDestroyed()) {
          scene.requestRender();
        }
      }),
    );
    cleanup.push(scene.renderError.addEventListener((_s, error) => this.#renderError(error)));
    cleanup.push(scene.camera.moveEnd.addEventListener(() => this.#emitView()));
    const cameraIntent = () => {
      if (this.isFollowing()) {
        this.stopFollowing();
      }
      this.#cameraIntentListeners.emit(undefined);
    };
    let userSettleTimer: ReturnType<typeof setTimeout> | null = null;
    const scheduleUserSettle = () => {
      if (userSettleTimer !== null) clearTimeout(userSettleTimer);
      userSettleTimer = setTimeout(() => {
        userSettleTimer = null;
        if (!scene.isDestroyed()) scene.requestRender();
      }, 550);
    };
    widget.canvas.addEventListener('pointerdown', cameraIntent);
    widget.canvas.addEventListener('pointerup', scheduleUserSettle);
    widget.canvas.addEventListener('wheel', cameraIntent, { passive: true });
    widget.canvas.addEventListener('wheel', scheduleUserSettle, { passive: true });
    cleanup.push(() => {
      if (userSettleTimer !== null) clearTimeout(userSettleTimer);
      widget.canvas.removeEventListener('pointerdown', cameraIntent);
      widget.canvas.removeEventListener('pointerup', scheduleUserSettle);
      widget.canvas.removeEventListener('wheel', cameraIntent);
      widget.canvas.removeEventListener('wheel', scheduleUserSettle);
    });
    const offFirstFrame = scene.postRender.addEventListener(() => {
      offFirstFrame();
      this.#emitView();
    });
    cleanup.push(offFirstFrame);
    cleanup.push(watchContextLoss(widget.canvas, { onLost: (error) => this.#report(error) }));
    const cameraPort = cesiumCameraPort(scene, () => this.#emitView());
    cleanup.push(() => cameraPort.destroy());

    return {
      widget,
      renderers,
      aircraft,
      satellites,
      vessels,
      transit,
      bikeshare,
      boundaries,
      cables,
      cctv,
      directions,
      tracking,
      annotations: new AnnotationRenderer(primitives, ctx, scene),
      halo: new SelectionHalo(primitives, colors),
      imagery: new BaseImagery(scene, prefersReducedMotion),
      camera: new CameraService(cameraPort, prefersReducedMotion),
      interaction: new Interaction(widget, {
        pick: (entity) => this.#pickListeners.emit(entity),
        cursor: (position) => this.#cursorListeners.emit(position),
        point: (position) => this.#pointListeners.emit(position),
        drawing: () => this.#drawing,
        // Open ring centres and spiral gaps answer a click only when nothing drawn is there.
        pickAssists: [...renderers.values()].flatMap((r) => (r.pickAssists ? [r.pickAssists] : [])),
        pointPick: (pt) => {
          const breakaway = boundaries.findBreakawayAt(pt.lon, pt.lat);
          return breakaway ? { feed: 'boundaries', id: breakaway.id } : null;
        },
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
    if (this.#following?.feed === snapshot.feed) this.#updateTrackingTarget(snapshot);
  }

  clearFeed(feed: FeedId): void {
    const m = this.#mounted;
    if (!m) return;
    m.renderers.get(feed)?.clear();
    if (this.#selection?.feed === feed) this.#refreshHalo();
    if (this.#following?.feed === feed) this.stopFollowing();
    m.widget.scene.requestRender();
  }

  setSelection(selection: EntityRef | null): void {
    this.#selection = selection ? { feed: selection.feed, id: selection.id } : null;
    const m = this.#mounted;
    if (!m) return;
    for (const renderer of m.renderers.values()) {
      renderer.setSelected(renderer.feed === selection?.feed ? selection.id : null);
    }
    m.boundaries.setSelected(selection?.feed === 'boundaries' ? selection.id : null);
    m.cables.setSelected(selection?.feed === 'cables' ? selection.id : null);
    m.cctv.setSelected(selection?.feed === 'cctv' ? selection.id : null);
    m.halo.set(this.#selection);
    this.#refreshHalo();
  }

  showTrack(track: SatelliteTrack | null): void {
    this.#mounted?.satellites.showTrack(track);
  }

  showAircraftTrack(track: AircraftTrack | null): void {
    this.#mounted?.aircraft.showTrack(track);
  }

  showVesselTrack(track: VesselTrackResponse | null): void {
    this.#mounted?.vessels.showTrack(track);
  }

  showTransitTrail(trail: TransitTrail | null): void {
    this.#mounted?.transit.showTrail(trail);
  }

  setCables(items: readonly SubmarineCableItem[]): void {
    const m = this.#mounted;
    if (!m) return;
    m.cables.setCables(items);
    if (this.#selection?.feed === 'cables') this.#refreshHalo();
    m.widget.scene.requestRender();
  }

  setCctvCameras(cameras: readonly CameraSource[]): void {
    const m = this.#mounted;
    if (!m) return;
    m.cctv.setCameras(cameras);
    if (this.#selection?.feed === 'cctv') this.#refreshHalo();
    m.widget.scene.requestRender();
  }

  setCctvDetections(detections: CctvDetectionsResponse | null): void {
    this.#mounted?.cctv.setDetections(detections);
  }

  getCctvDetections(): CctvDetectionsResponse | null {
    return this.#mounted?.cctv.getDetections() ?? null;
  }

  setBoundaries(features: readonly BoundaryFeature[]): void {
    const m = this.#mounted;
    if (!m) return;
    m.boundaries.setFeatures(features);
    if (this.#selection?.feed === 'boundaries') this.#refreshHalo();
    m.widget.scene.requestRender();
  }

  setBoundaryClassVisibility(visibility: Partial<BoundaryClassVisibility>): void {
    const m = this.#mounted;
    if (!m) return;
    m.boundaries.setVisibility(visibility);
    if (this.#selection?.feed === 'boundaries') this.#refreshHalo();
    m.widget.scene.requestRender();
  }

  getBoundaryClassVisibility(): BoundaryClassVisibility {
    return this.#mounted?.boundaries.getVisibility() ?? { ...DEFAULT_BOUNDARY_VISIBILITY };
  }

  locateBoundary(id: string): Located | undefined {
    return this.#mounted?.boundaries.locate(id);
  }

  getBoundaryFeature(id: string): BoundaryFeature | undefined {
    return this.#mounted?.boundaries.getFeature(id);
  }

  getCable(id: string): SubmarineCableItem | undefined {
    return this.#mounted?.cables.getCable(id);
  }

  getCctvCamera(id: string): CameraSource | undefined {
    return this.#mounted?.cctv.getCamera(id);
  }

  setRoute(route: Route | null): void {
    const m = this.#mounted;
    if (!m) return;
    m.directions.setRoute(route);
    m.widget.scene.requestRender();
  }

  setRouteWaypoints(origin: [number, number] | null, destination: [number, number] | null): void {
    const m = this.#mounted;
    if (!m) return;
    m.directions.setWaypoints(origin, destination);
    m.widget.scene.requestRender();
  }

  selectRouteStep(index: number | null): void {
    const m = this.#mounted;
    if (!m) return;
    m.directions.setSelectedStep(index);
    if (index !== null) {
      const loc = m.directions.locateStep(index);
      if (loc) {
        m.halo.follow(loc);
      }
    }
    m.widget.scene.requestRender();
  }

  getRoute(): Route | null {
    return this.#mounted?.directions.getRoute() ?? null;
  }

  startFollowing(target: EntityRef): void {
    const m = this.#mounted;
    if (!m) return;
    this.#following = target;
    m.tracking.startTracking(target);

    if (
      target.feed === 'boundaries' ||
      target.feed === 'installations' ||
      target.feed === 'directions-step' ||
      target.feed === 'directions-waypoint'
    )
      return;
    let located: Located | undefined;
    if (target.feed === 'cables') {
      located = m.cables.locate(target.id);
    } else if (target.feed === 'cctv') {
      located = m.cctv.locate(target.id);
    } else {
      const renderer = m.renderers.get(target.feed);
      located = renderer?.locate?.(target.id);
    }
    if (located) {
      m.tracking.updateTargetPosition(located.position);
    }
    m.widget.scene.requestRender();
  }

  stopFollowing(): void {
    this.#following = null;
    this.#mounted?.tracking.release('manual');
  }

  isFollowing(): boolean {
    const state = this.#mounted?.tracking.getState();
    return state !== undefined && state.status !== 'released';
  }

  getFollowState(): TrackingState | null {
    const state = this.#mounted?.tracking.getState();
    return state && state.status !== 'released' ? state : null;
  }

  onFollowChange(listener: (state: TrackingState) => void): () => void {
    return this.#followListeners.add(listener);
  }

  setAnnotations(annotations: readonly MapAnnotation[], draft: AnnotationDraft | null): void {
    this.#mounted?.annotations.show(annotations, draft);
  }

  setDrawing(active: boolean): void {
    this.#drawing = active;
  }

  onMapPoint(listener: (point: GeoPoint | null) => void): () => void {
    return this.#pointListeners.add(listener);
  }

  setImagery(imagery: ImageryDescriptor): void {
    this.#mounted?.imagery.show(imagery);
  }

  flyTo(target: FlyTarget): void {
    this.#cameraIntentListeners.emit(undefined);
    this.#mounted?.camera.flyTo(target);
  }

  home(): void {
    this.#cameraIntentListeners.emit(undefined);
    this.#mounted?.camera.home();
  }

  zoomIn(): void {
    this.#cameraIntentListeners.emit(undefined);
    this.#mounted?.camera.zoomIn();
  }

  zoomOut(): void {
    this.#cameraIntentListeners.emit(undefined);
    this.#mounted?.camera.zoomOut();
  }

  resetNorth(): void {
    this.#cameraIntentListeners.emit(undefined);
    this.#mounted?.camera.resetNorth();
  }

  cameraPose(): CameraPose | null {
    const camera = this.#mounted?.widget.scene.camera;
    if (!camera) return null;
    const position = camera.positionCartographic;
    return {
      lon: CesiumMath.toDegrees(position.longitude),
      lat: CesiumMath.toDegrees(position.latitude),
      heightM: position.height,
      headingRad: camera.heading,
      pitchRad: camera.pitch,
      rollRad: camera.roll,
    };
  }

  restoreCamera(pose: CameraPose): void {
    const scene = this.#mounted?.widget.scene;
    if (!scene) return;
    scene.camera.cancelFlight();
    scene.camera.setView({
      destination: Cartesian3.fromDegrees(pose.lon, pose.lat, pose.heightM),
      orientation: { heading: pose.headingRad, pitch: pose.pitchRad, roll: pose.rollRad },
    });
    this.#emitView();
    scene.requestRender();
  }

  centrePosition(): GeoPoint | null {
    const mounted = this.#mounted;
    if (!mounted) return null;
    const canvas = mounted.widget.scene.canvas;
    try {
      return geoPointAt(
        mounted.widget,
        new Cartesian2(canvas.clientWidth / 2, canvas.clientHeight / 2),
      );
    } catch {
      return null;
    }
  }

  onCameraIntent(listener: () => void): () => void {
    return this.#cameraIntentListeners.add(listener);
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
    this.#following = null;
    this.#pickListeners.clear();
    this.#viewListeners.clear();
    this.#cursorListeners.clear();
    this.#cameraIntentListeners.clear();
    this.#pointListeners.clear();
    this.#followListeners.clear();
    if (!m) return;
    try {
      for (const c of m.cleanup.splice(0)) c();
      m.interaction.destroy();
      m.imagery.destroy();
      for (const renderer of m.renderers.values()) renderer.destroy();
      m.halo.destroy();
      m.annotations.destroy();
    } finally {
      if (!m.widget.isDestroyed()) m.widget.destroy();
    }
  }

  #updateTrackingTarget(snapshot: FeedSnapshot): void {
    const m = this.#mounted;
    if (!m || !this.#following || this.#following.feed !== snapshot.feed) return;
    const id = this.#following.id;

    if (snapshot.items.kind === 'aircraft') {
      const item = snapshot.items.items.find((a) => a.hex === id);
      if (item && hasValidPosition(item)) {
        const pos = Cartesian3.fromDegrees(
          item.position.lon,
          item.position.lat,
          aircraftAltitudeM(item),
          Ellipsoid.WGS84,
        );
        m.tracking.updateTargetPosition(
          pos,
          item.trackDeg ?? undefined,
          item.groundSpeedKt ?? undefined,
          Date.now(),
        );
      }
    } else if (snapshot.items.kind === 'vessels') {
      const item = snapshot.items.items.find((v) => v.mmsi === id);
      if (item && hasValidPosition(item)) {
        const pos = Cartesian3.fromDegrees(
          item.position.lon,
          item.position.lat,
          0,
          Ellipsoid.WGS84,
        );
        m.tracking.updateTargetPosition(
          pos,
          item.headingDeg ?? item.courseDeg ?? undefined,
          item.speedKnots ?? undefined,
          Date.now(),
        );
      }
    } else {
      const located = m.renderers.get(snapshot.feed)?.locate(id);
      if (located) {
        m.tracking.updateTargetPosition(located.position, undefined, undefined, Date.now());
      }
    }
  }

  #refreshHalo(): void {
    const m = this.#mounted;
    if (!m) return;
    const selection = this.#selection;
    if (selection?.feed === 'boundaries') {
      const located = m.boundaries.locate(selection.id);
      m.halo.follow(
        located ? { position: located.position, symbolPx: located.symbolPx } : undefined,
      );
    } else if (selection?.feed === 'cables') {
      m.halo.follow(m.cables.locate(selection.id));
    } else if (selection?.feed === 'cctv') {
      m.halo.follow(m.cctv.locate(selection.id));
    } else if (selection?.feed === 'directions-step') {
      m.halo.follow(m.directions.locateStep(Number(selection.id)) ?? undefined);
    } else if (selection?.feed === 'directions-waypoint') {
      m.halo.follow(
        m.directions.locateWaypoint(selection.id as 'origin' | 'destination') ?? undefined,
      );
    } else if (selection && selection.feed !== 'installations') {
      m.halo.follow(m.renderers.get(selection.feed)?.locate(selection.id));
    } else {
      m.halo.follow(undefined);
    }
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
      const centre = this.centrePosition();
      const rectangle = scene.camera.computeViewRectangle(scene.ellipsoid, new Rectangle());
      // Cesium bounds only the corners; the edges widen it (view.ts `widenToEdges`).
      const edges = viewEdgeSamples(scene.canvas.clientWidth, scene.canvas.clientHeight, (x, y) =>
        geoPointAt(m.widget, new Cartesian2(x, y)),
      );
      const view = toViewState(rectangle, centre, scene.camera.positionCartographic.height, edges);
      // A completion callback can precede Cesium's native moveEnd for the same pose.
      // Relayout still runs for rotations whose normalised query view did not change.
      for (const renderer of m.renderers.values()) renderer.viewChanged();
      m.annotations.viewChanged();
      if (sameView(this.#lastView, view)) return;
      this.#lastView = view;
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
