import { Cartesian3, Event as CesiumEvent, Rectangle } from '@cesium/core';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { CameraSource, CctvDetectionsResponse, TransitTrail } from '../contracts';
import { GlobeEngine } from './GlobeEngine';

const fixtures = vi.hoisted(() => {
  /** Which renderers relaid out, in order: each mock records its own name. */
  const relayouts: string[] = [];
  const renderer = (name: string) =>
    class {
      feed = name;
      viewChanged() {
        relayouts.push(name);
      }
      setSelected() {}
      locate() {
        return undefined;
      }
      destroy() {}
    };
  /** What the vessels renderer was asked to draw as the selected ship's track. */
  const vesselTracks: unknown[] = [];
  const vessels = class extends renderer('vessels') {
    showTrack(track: unknown) {
      vesselTracks.push(track);
    }
  };
  /** What the aircraft renderer was asked to draw as the selected aircraft's track. */
  const aircraftTracks: unknown[] = [];
  const aircraft = class extends renderer('aircraft') {
    showTrack(track: unknown) {
      aircraftTracks.push(track);
    }
  };
  const boundaryFeatures: unknown[] = [];
  const boundaryVisibilities: unknown[] = [];
  const boundarySelected: unknown[] = [];
  const cableItems: unknown[] = [];
  const cctvCameras: unknown[] = [];
  const transitTrails: unknown[] = [];
  /** Screen point → globe point; null picks the centre for every point. */
  const pick = {
    at: null as null | ((x: number, y: number) => { lat: number; lon: number } | null),
  };
  return {
    widget: null as unknown,
    centre: { lat: 20, lon: -60 },
    relayouts,
    renderer,
    vessels,
    vesselTracks,
    aircraft,
    aircraftTracks,
    boundaryFeatures,
    boundaryVisibilities,
    boundarySelected,
    cableItems,
    cctvCameras,
    transitTrails,
    pick,
  };
});

vi.mock('@cesium/engine', () => ({
  // Called with `new`: Vitest 5 requires a constructible (function or class) implementation.
  CesiumWidget: class {
    constructor() {
      Object.assign(this, fixtures.widget);
    }
  },
  CreditDisplay: {},
  EllipsoidTerrainProvider: class {},
  ImageryLayer: { fromProviderAsync: () => ({}) },
  Ion: {},
  PolylineCollection: class {
    add(options: unknown) {
      return { ...(typeof options === 'object' && options !== null ? options : {}), show: true };
    }
    remove() {}
    removeAll() {}
    destroy() {}
    isDestroyed() {
      return false;
    }
  },
  PointPrimitiveCollection: class {
    add(options: unknown) {
      return { ...(typeof options === 'object' && options !== null ? options : {}), show: true };
    }
    remove() {}
    removeAll() {}
    destroy() {}
    isDestroyed() {
      return false;
    }
  },
}));
vi.mock('./imagery', () => ({
  BaseImagery: class {
    destroy() {}
  },
  bundledProvider: () => ({}),
}));
vi.mock('./interaction', () => ({
  Interaction: class {
    destroy() {}
  },
  geoPointAt: (_widget: unknown, at: { x: number; y: number }) =>
    fixtures.pick.at ? fixtures.pick.at(at.x, at.y) : fixtures.centre,
}));
vi.mock('./tokens', () => ({ cssTokenReader: () => () => '', readPalette: () => ({}) }));
vi.mock('./renderers/types', () => ({ toMapColors: () => ({}) }));
vi.mock('./renderers/aircraft', () => ({ AircraftRenderer: fixtures.aircraft }));
vi.mock('./renderers/annotations', () => ({
  AnnotationRenderer: fixtures.renderer('annotations'),
}));
vi.mock('./renderers/cyclones', () => ({ CycloneRenderer: fixtures.renderer('cyclones') }));
vi.mock('./renderers/earthquakes', () => ({
  EarthquakeRenderer: fixtures.renderer('earthquakes'),
}));
vi.mock('./renderers/fires', () => ({ FireRenderer: fixtures.renderer('fires') }));
vi.mock('./renderers/launches', () => ({ LaunchPadRenderer: fixtures.renderer('launches') }));
vi.mock('./renderers/satellites', () => ({ SatelliteRenderer: fixtures.renderer('satellites') }));
vi.mock('./renderers/vessels', () => ({ VesselRenderer: fixtures.vessels }));
vi.mock('./renderers/bikeshare', () => ({ BikeshareRenderer: fixtures.renderer('bikeshare') }));
vi.mock('./renderers/transit', () => ({
  TransitRenderer: class extends fixtures.renderer('transit') {
    showTrail = vi.fn((trail: unknown) => {
      fixtures.transitTrails.push(trail);
    });
  },
}));
vi.mock('./renderers/cables', () => ({
  CableRenderer: class {
    setCables = vi.fn((cables: unknown) => {
      fixtures.cableItems.push(cables);
    });
    setSelected = vi.fn();
    locate = vi.fn();
    clear = vi.fn();
    destroy = vi.fn();
  },
}));
vi.mock('./renderers/cctv', () => ({
  CctvRenderer: class {
    #detections: unknown = null;
    setCameras = vi.fn((cams: unknown) => {
      fixtures.cctvCameras.push(cams);
    });
    setDetections = vi.fn((d: unknown) => {
      this.#detections = d;
    });
    getDetections = vi.fn(() => this.#detections);
    setSelected = vi.fn();
    locate = vi.fn();
    clear = vi.fn();
    destroy = vi.fn();
  },
}));
vi.mock('./renderers/boundaries', () => ({
  BOUNDARY_LOCATED_PX: 32,
  DEFAULT_BOUNDARY_VISIBILITY: {
    internationalLand: true,
    disputedLine: true,
    breakawayArea: true,
    maritimeIndicative: true,
  },
  BoundaryRenderer: class {
    setFeatures = vi.fn((f: unknown) => {
      fixtures.boundaryFeatures.push(f);
    });
    setVisibility = vi.fn((v: unknown) => {
      fixtures.boundaryVisibilities.push(v);
    });
    getVisibility = vi.fn(() => ({
      internationalLand: true,
      disputedLine: true,
      breakawayArea: true,
      maritimeIndicative: true,
    }));
    locate = vi.fn();
    getFeature = vi.fn();
    setSelected = vi.fn((id: unknown) => {
      fixtures.boundarySelected.push(id);
    });
    findBreakawayAt = vi.fn();
    destroy = vi.fn();
    clear = vi.fn();
  },
}));
vi.mock('./renderers/selection', () => ({
  SelectionHalo: class {
    set() {}
    follow() {}
    update() {}
    destroy() {}
  },
}));

interface MockFlight {
  readonly duration: number;
  readonly complete: () => void;
  readonly cancel: () => void;
}

function setup() {
  const canvas = document.createElement('canvas');
  let rectangle = Rectangle.fromDegrees(-70, 10, -50, 30);
  let flight: MockFlight | null = null;
  const scene = {
    canvas,
    screenSpaceCameraController: {},
    primitives: {
      add: vi.fn((p: unknown) => p),
      remove: vi.fn(),
    },
    ellipsoid: {},
    requestRender: vi.fn(),
    isDestroyed: () => false,
    renderError: new CesiumEvent(),
    preUpdate: new CesiumEvent(),
    postRender: new CesiumEvent(),
    camera: {
      positionCartographic: { height: 400_000 },
      positionWC: new Cartesian3(1, 2, 3),
      heading: 0,
      pitch: -Math.PI / 2,
      roll: 0,
      setView: vi.fn(),
      // Like Cesium: cancelling runs the flight's cancel callback synchronously, and a
      // flight of duration <= 0 completes synchronously.
      cancelFlight: vi.fn(() => {
        const cancelled = flight;
        flight = null;
        cancelled?.cancel();
      }),
      computeViewRectangle: () => rectangle,
      moveEnd: new CesiumEvent(),
      flyTo: vi.fn((options: MockFlight) => {
        if (options.duration <= 0) options.complete();
        else flight = options;
      }),
    },
  };
  const widget = { scene, canvas, destroy: vi.fn(), isDestroyed: () => false };
  fixtures.widget = widget;
  fixtures.centre = { lat: 20, lon: -60 };
  fixtures.relayouts.splice(0);
  fixtures.vesselTracks.splice(0);
  fixtures.pick.at = null;
  const engine = new GlobeEngine({
    host: document.createElement('div'),
    credits: document.createElement('div'),
    onRenderError: vi.fn(),
  });
  engine.mount();
  const listener = vi.fn();
  engine.onViewChange(listener);
  return {
    engine,
    scene,
    canvas,
    listener,
    setRectangle: (next: Rectangle) => {
      rectangle = next;
    },
    completeFlight: () => {
      const completed = flight;
      flight = null;
      completed?.complete();
    },
  };
}

const POSE = { lon: -40, lat: 30, heightM: 900_000, headingRad: 0, pitchRad: -1.5, rollRad: 0 };

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('GlobeEngine settled view publication', () => {
  it('publishes a settled flight once across completion, first frame and native moveEnd', () => {
    const { engine, scene, listener, completeFlight } = setup();
    engine.flyTo({ kind: 'point', position: { lat: 20, lon: -60 } });
    expect(listener).not.toHaveBeenCalled();
    completeFlight();
    expect(listener).toHaveBeenCalledTimes(1);
    scene.postRender.raiseEvent();
    scene.camera.moveEnd.raiseEvent();
    scene.camera.moveEnd.raiseEvent();
    expect(listener).toHaveBeenCalledTimes(1);
    engine.destroy();
  });

  it('still publishes changed native mouse views and relayouts equal query views', () => {
    const { engine, scene, listener, setRectangle } = setup();
    scene.camera.moveEnd.raiseEvent();
    expect(listener).toHaveBeenCalledTimes(1);
    setRectangle(Rectangle.fromDegrees(-69, 10, -49, 30));
    fixtures.centre = { lat: 20, lon: -59 };
    scene.camera.moveEnd.raiseEvent();
    expect(listener).toHaveBeenCalledTimes(2);
    // A pure rotation: the normalised query view is unchanged, so nothing is published, but
    // every feed renderer and the annotations still relayout (glyph rotation, label budget).
    fixtures.relayouts.splice(0);
    scene.camera.moveEnd.raiseEvent();
    expect(listener).toHaveBeenCalledTimes(2);
    expect([...fixtures.relayouts].sort()).toEqual([
      'aircraft',
      'annotations',
      'bikeshare',
      'cyclones',
      'earthquakes',
      'fires',
      'launches',
      'satellites',
      'transit',
      'vessels',
    ]);
    engine.destroy();
  });

  it('does not publish the mid-flight pose of a flight cancelled by a camera restore', () => {
    const { engine, scene, listener, setRectangle } = setup();
    engine.flyTo({ kind: 'point', position: { lat: 20, lon: -60 } });
    // Somewhere along the flight path when the restore cancels it.
    setRectangle(Rectangle.fromDegrees(-80, 0, -60, 20));
    fixtures.centre = { lat: 10, lon: -70 };
    scene.camera.setView.mockImplementation(() => {
      setRectangle(Rectangle.fromDegrees(-50, 20, -30, 40));
      fixtures.centre = { lat: 30, lon: -40 };
    });
    engine.restoreCamera(POSE);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener.mock.calls[0]?.[0]).toMatchObject({ centre: { lat: 30, lon: -40 } });
    engine.destroy();
  });

  it('coalesces wheel and pointerup settle renders and never fires one after destroy', () => {
    vi.useFakeTimers();
    const { engine, scene, canvas } = setup();
    canvas.dispatchEvent(new Event('wheel'));
    canvas.dispatchEvent(new Event('wheel'));
    canvas.dispatchEvent(new Event('pointerup'));
    scene.requestRender.mockClear();
    vi.advanceTimersByTime(549);
    expect(scene.requestRender).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(scene.requestRender).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);

    canvas.dispatchEvent(new Event('wheel'));
    expect(vi.getTimerCount()).toBe(1);
    engine.destroy();
    expect(vi.getTimerCount()).toBe(0);
    canvas.dispatchEvent(new Event('pointerup'));
    canvas.dispatchEvent(new Event('wheel'));
    vi.advanceTimersByTime(2_000);
    expect(vi.getTimerCount()).toBe(0);
    expect(scene.requestRender).toHaveBeenCalledTimes(1);
  });

  it('widens the published bbox to the screen edges, not only the corners (ENG-980 review)', () => {
    const { engine, scene, canvas, listener, setRectangle } = setup();
    Object.defineProperty(canvas, 'clientWidth', { value: 1280, configurable: true });
    Object.defineProperty(canvas, 'clientHeight', { value: 720, configurable: true });
    // 5 000 km over the Gulf of Finland: Cesium's corner box ends at 50.9° N, south of the
    // view's own centre; the top edge shows 72° N.
    setRectangle(Rectangle.fromDegrees(-61.76, 24.05, 111.12, 50.9));
    fixtures.centre = { lat: 59.94, lon: 24.68 };
    fixtures.pick.at = (x, y) => {
      if (x === 640 && y === 0) return { lat: 72.07, lon: 24.68 };
      if (x === 640 && y === 360) return fixtures.centre;
      return { lat: 40, lon: 24.68 };
    };
    scene.camera.moveEnd.raiseEvent();
    const bbox = listener.mock.calls.at(-1)?.[0]?.bbox;
    expect(bbox).toMatchObject({ south: 24.05, north: 72.07 });
    engine.destroy();
  });

  it('hands the selected vessel track to the vessels renderer and clears it', () => {
    const { engine } = setup();
    const track = { mmsi: '230990001', samples: [], source: 'aisstream', retainedSec: 1_800 };
    engine.showVesselTrack(track);
    engine.showVesselTrack(null);
    expect(fixtures.vesselTracks).toEqual([track, null]);
    engine.destroy();
    engine.showVesselTrack(track);
    expect(fixtures.vesselTracks).toHaveLength(2);
  });

  it('hands the selected aircraft track to the aircraft renderer and clears it', () => {
    const { engine } = setup();
    const track = { hex: '4aca2d', points: [], sources: ['test'] };
    engine.showAircraftTrack(track);
    engine.showAircraftTrack(null);
    expect(fixtures.aircraftTracks).toEqual([track, null]);
    engine.destroy();
  });

  it('manages entity following state and cancels on user camera interaction', () => {
    const { engine, canvas } = setup();
    const target = { feed: 'aircraft' as const, id: 'hex-123' };
    expect(engine.isFollowing()).toBe(false);
    expect(engine.getFollowState()).toBeNull();

    engine.startFollowing(target);
    expect(engine.isFollowing()).toBe(true);
    expect(engine.getFollowState()?.target).toEqual(target);
    expect(engine.getFollowState()?.status).toBe('tracking');

    // Canvas pointerdown (camera interaction) immediately cancels tracking
    canvas.dispatchEvent(new Event('pointerdown'));
    expect(engine.isFollowing()).toBe(false);
    expect(engine.getFollowState()).toBeNull();

    engine.destroy();
  });

  it('manages boundary features, class visibility, and boundary selection', () => {
    const { engine } = setup();
    const mockFeature = {
      id: 'disputed-1',
      name: 'Disputed Line',
      class: 'disputedLine' as const,
      status: 'Disputed',
      claimants: ['Country A', 'Country B'],
      lines: [
        [
          { lat: 10, lon: 20 },
          { lat: 10, lon: 21 },
        ],
      ],
      bbox: { west: 20, south: 10, east: 21, north: 10 },
    };

    engine.setBoundaries([mockFeature]);
    expect(fixtures.boundaryFeatures).toContainEqual([mockFeature]);

    engine.setBoundaryClassVisibility({ disputedLine: false });
    expect(fixtures.boundaryVisibilities).toContainEqual({ disputedLine: false });

    engine.setSelection({ feed: 'boundaries', id: 'disputed-1' });
    expect(fixtures.boundarySelected).toContain('disputed-1');

    engine.setSelection(null);
    expect(fixtures.boundarySelected).toContain(null);

    engine.destroy();
  });

  it('manages submarine cables, transit trails, and CCTV cameras & detections', () => {
    const { engine } = setup();

    const mockCable = {
      id: 'cable-1',
      name: 'Transatlantic-1',
      coordinates: [
        [
          [-70, 40],
          [-10, 50],
        ],
      ] as const,
    };
    engine.setCables([mockCable]);
    expect(fixtures.cableItems).toContainEqual([mockCable]);

    const mockTrail: TransitTrail = {
      version: 1,
      feedId: 'muni',
      vehicleId: 'veh-42',
      fixes: [[1_700_000_000_000, 37.77, -122.41, 0, 0]],
      epochs: [{ id: 1, trip: 'trip-42', route: '1', mode: 'bus' }],
      truncated: false,
    };
    engine.showTransitTrail(mockTrail);
    expect(fixtures.transitTrails).toContainEqual(mockTrail);

    const mockCam: CameraSource = {
      id: 'cam-ny-1',
      name: 'Hudson Pier Camera',
      city: 'New York',
      cityId: 'nyc',
      provider: 'nycdot',
      lat: 40.71,
      lon: -74.01,
      headingDeg: 180,
      pitchDeg: -10,
      fovDeg: 60,
      feedType: 'public',
      url: 'https://example.com/cam-ny-1',
      snapshotUrl: 'https://example.com/cam-ny-1.jpg',
      license: 'public-domain',
      code: 'nyc-1',
      sourceKind: 'city',
    };
    engine.setCctvCameras([mockCam]);
    expect(fixtures.cctvCameras).toContainEqual([mockCam]);

    const mockDetections: CctvDetectionsResponse = {
      cameraId: 'cam-ny-1',
      timestampMs: 1_700_000_000_000n,
      frameSequence: 1024n,
      engine: 'mojo-mlir-simd',
      detections: [
        {
          bbox: [0.25, 0.25, 0.6, 0.75],
          classId: 3,
          className: 'vessel',
          confidence: 0.94,
        },
      ],
    };
    engine.setCctvDetections(mockDetections);
    expect(engine.getCctvDetections()).toEqual(mockDetections);

    engine.destroy();
  });

  it('removes every scene and camera listener it added, including the camera port frame hook', () => {
    const { engine, scene } = setup();
    expect(scene.postRender.numberOfListeners).toBeGreaterThan(0);
    engine.destroy();
    expect(scene.postRender.numberOfListeners).toBe(0);
    expect(scene.renderError.numberOfListeners).toBe(0);
    expect(scene.camera.moveEnd.numberOfListeners).toBe(0);
  });
});
