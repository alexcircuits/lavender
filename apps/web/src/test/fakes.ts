/**
 * Shared test doubles: a scriptable `LavenderPlatform`, a recording `GlobeController` and
 * small contract fixtures. Tests drive feeds by calling `platform.subs[i].onEvent(...)`.
 */
import { vi } from 'vitest';
import type {
  AircraftTrack,
  BBox,
  BoundaryClass,
  BoundaryResponse,
  CameraSource,
  Catalog,
  CctvDetectionsResponse,
  FeedId,
  FeedQuery,
  FeedSnapshot,
  GeoPoint,
  ImageryDescriptor,
  LayerDescriptor,
  Route,
  SatellitePasses,
  SatellitePassRequest,
  SatelliteTrack,
  SatelliteTrackRequest,
  SearchQuery,
  SearchResults,
  SystemStatus,
  TransitTrail,
  VesselTrackResponse,
  WindManifest,
} from '../contracts';
import type {
  CameraPose,
  EntityRef,
  GlobeController,
  SubmarineCableItem,
  ViewState,
} from '../globe';
import type {
  CallOptions,
  DivisionRecordItem,
  FeedEvent,
  LavenderPlatform,
  MilitaryInstallationItem,
  PreferenceStore,
  RecentImageryResponse,
  SecretStatusResponse,
  ThumbnailProbeResponse,
  TransportState,
  WindGridRequest,
  WindRequest,
} from '../platform';

export function layer(feed: FeedId, overrides: Partial<LayerDescriptor> = {}): LayerDescriptor {
  const family = feed === 'aircraft' ? 'air' : feed === 'satellites' ? 'space' : 'hazards';
  return {
    feed,
    name: feed[0]?.toUpperCase() + feed.slice(1),
    description: `${feed} layer`,
    family,
    sourceIds: [`${feed}-source`],
    availability: { state: 'available' },
    cadenceS: 60,
    scope: { kind: 'global' },
    defaultOn: false,
    ...overrides,
  };
}

export const imagery = (
  id: string,
  overrides: Partial<ImageryDescriptor> = {},
): ImageryDescriptor => ({
  id,
  name: id,
  kind: 'bundled',
  url: '',
  maximumLevel: 5,
  sourceId: 'natural-earth',
  tileHosts: [],
  isDark: false,
  isDefault: false,
  ...overrides,
});

export function catalogOf(layers: LayerDescriptor[], overrides: Partial<Catalog> = {}): Catalog {
  return {
    layers,
    sources: [],
    imagery: [imagery('offline', { isDefault: true }), imagery('night', { isDark: true })],
    licenceMode: 'evaluation',
    ...overrides,
  };
}

export function quakeSnapshot(ids: string[], overrides: Partial<FeedSnapshot> = {}): FeedSnapshot {
  return {
    feed: 'earthquakes',
    sources: ['usgs-earthquakes'],
    fetchedAt: '2026-10-11T12:00:00Z',
    generatedAt: '2026-10-11T12:00:01Z',
    freshness: { state: 'live' },
    coverage: { kind: 'complete' },
    sourceCount: ids.length + 1,
    pendingRegions: 0,
    items: {
      kind: 'earthquakes',
      items: ids.map((id) => ({
        id,
        magnitude: 3,
        depthKm: 5,
        position: { lat: 1, lon: 2 },
        time: '2026-10-11T11:00:00Z',
        tsunami: false,
      })),
    },
    notices: [],
    ...overrides,
  };
}

export const memoryPreferences = (
  initial: Record<string, unknown> = {},
): PreferenceStore & { data: Map<string, unknown> } => {
  const data = new Map<string, unknown>(Object.entries(initial));
  return {
    data,
    get: <T>(k: string) => data.get(k) as T | undefined,
    set: (k, v) => void data.set(k, v),
    remove: (k) => void data.delete(k),
  };
};

export interface FakeSub {
  readonly query: FeedQuery;
  readonly onEvent: (event: FeedEvent) => void;
  closed: boolean;
  readonly updates: FeedQuery[];
}

/** A scriptable platform. Override any method by assigning to the instance before use. */
export class FakePlatform implements LavenderPlatform {
  kind: 'web' | 'desktop' = 'web';
  readonly subs: FakeSub[] = [];
  readonly tracks: number[] = [];
  readonly searches: SearchQuery[] = [];
  preferences = memoryPreferences();
  catalogResult: Catalog | Error = catalogOf([
    layer('earthquakes'),
    layer('aircraft'),
    layer('satellites'),
  ]);
  statusResult: SystemStatus | Error = new Error('no status');
  searchResult: SearchResults = { coordinates: [], places: [], entities: [], notices: [] };
  #transport: TransportState = 'live';
  readonly #transportListeners = new Set<(s: TransportState) => void>();

  catalog = vi.fn(async () => {
    if (this.catalogResult instanceof Error) throw this.catalogResult;
    return this.catalogResult;
  });

  status = vi.fn(async () => {
    if (this.statusResult instanceof Error) throw this.statusResult;
    return this.statusResult;
  });

  snapshot = vi.fn(async (_query: FeedQuery): Promise<FeedSnapshot> => quakeSnapshot([]));

  search = vi.fn(async (query: SearchQuery, _options?: CallOptions) => {
    this.searches.push(query);
    return this.searchResult;
  });

  aircraftTrack = vi.fn(
    async (hex: string, _options?: CallOptions): Promise<AircraftTrack> => ({
      hex,
      points: [],
      sources: [],
    }),
  );

  vesselTrack = vi.fn(
    async (mmsi: string, _options?: CallOptions): Promise<VesselTrackResponse> => ({
      mmsi,
      samples: [],
      source: 'aisstream',
      retainedSec: 1_800,
    }),
  );

  satelliteTrack = vi.fn(async (r: SatelliteTrackRequest): Promise<SatelliteTrack> => {
    this.tracks.push(r.noradId);
    return { noradId: r.noradId, name: 'SAT', points: [], periodMin: 90 };
  });

  satellitePasses = vi.fn(
    async (request: SatellitePassRequest, _options?: CallOptions): Promise<SatellitePasses> => ({
      noradId: request.noradId,
      name: 'SAT',
      observer: request.observer,
      from: '2026-10-13T12:00:00Z',
      until: '2026-10-14T12:00:00Z',
      minElevationDeg: request.minElevationDeg ?? 0,
      passes: [],
    }),
  );

  windManifest = vi.fn(
    async (request: WindRequest, _options?: CallOptions): Promise<WindManifest> => ({
      schemaVersion: 1,
      model: request.model,
      stale: true,
      unavailable: true,
      reason: 'Wind upstream unavailable',
    }),
  );

  windStatus = vi.fn(
    async (request: WindRequest, options?: CallOptions): Promise<WindManifest> =>
      this.windManifest(request, options),
  );

  windGrid = vi.fn(
    async (_request: WindGridRequest, _options?: CallOptions): Promise<ArrayBuffer> =>
      new ArrayBuffer(0),
  );

  recentImagery = vi.fn(
    async (
      _bbox: BBox,
      _days?: number,
      _options?: CallOptions,
    ): Promise<RecentImageryResponse> => ({
      candidates: [],
      rankedLatest: { candidate: null, reason: null, certain: true },
      tileTemplates: {},
      attribution: { credit: '', licence: '', statement: '' },
      truncated: false,
      errors: [],
    }),
  );

  imageryThumbnailProbe = vi.fn(
    async (
      product: string,
      day: string,
      _bbox: BBox,
      _options?: CallOptions,
    ): Promise<ThumbnailProbeResponse> => ({
      product,
      day,
      dataPresent: true,
    }),
  );

  mapillaryStatus = vi.fn(
    async (_options?: CallOptions): Promise<{ configured: boolean }> => ({
      configured: true,
    }),
  );

  mapillarySearch = vi.fn(
    async (
      _bbox: BBox,
      _limit?: number,
      _options?: CallOptions,
    ): Promise<import('../platform/types').MapillarySearchResponse> => ({
      data: [],
    }),
  );

  boundaries = vi.fn(
    async (
      _bbox: BBox,
      _options?: {
        readonly classes?: readonly BoundaryClass[];
        readonly zoom?: number;
        readonly limit?: number;
        readonly signal?: AbortSignal;
      },
    ): Promise<BoundaryResponse> => ({
      features: [],
      total: 0,
      attribution: 'Made with Natural Earth',
    }),
  );

  cctvDetections = vi.fn(
    async (_cameraId: string, _options?: CallOptions): Promise<CctvDetectionsResponse> => ({
      cameraId: _cameraId,
      timestampMs: BigInt(0),
      frameSequence: BigInt(0),
      engine: 'fake',
      detections: [],
    }),
  );

  cctvSources = vi.fn(
    async (_bbox?: BBox, _limit?: number, _options?: CallOptions): Promise<CameraSource[]> => [],
  );

  transitTrail = vi.fn(
    async (feedId: string, vehicleId: string, _options?: CallOptions): Promise<TransitTrail> => ({
      version: 1,
      feedId,
      vehicleId,
      fixes: [],
      epochs: [],
      truncated: false,
    }),
  );

  submarineCables = vi.fn(
    async (
      _bbox?: BBox,
      _zoom?: number,
      _limit?: number,
      _options?: CallOptions,
    ): Promise<SubmarineCableItem[]> => [],
  );

  calculateRoute = vi.fn(
    async (
      profile: 'car' | 'bike' | 'foot',
      _coords: string,
      withSteps = true,
      _options?: CallOptions,
    ): Promise<Route> => ({
      ok: true,
      profile,
      distanceM: 1000,
      durationS: 600,
      geometry: [
        [13.4, 52.5],
        [13.41, 52.51],
      ],
      ...(withSteps
        ? {
            steps: [
              {
                index: 0,
                type: 'depart',
                name: 'Start',
                distanceM: 500,
                durationS: 300,
                lon: 13.4,
                lat: 52.5,
                instruction: 'Depart north',
              },
              {
                index: 1,
                type: 'arrive',
                name: 'End',
                distanceM: 500,
                durationS: 300,
                lon: 13.41,
                lat: 52.51,
                instruction: 'Arrive at destination',
              },
            ],
          }
        : {}),
    }),
  );

  divisionsAt = vi.fn(
    async (
      _lon: number,
      _lat: number,
      _options?: CallOptions,
    ): Promise<readonly DivisionRecordItem[]> => [],
  );

  divisionsSearch = vi.fn(
    async (_query: string, _options?: CallOptions): Promise<readonly DivisionRecordItem[]> => [],
  );

  militaryInstallations = vi.fn(
    async (
      _bbox: BBox,
      _limit?: number,
      _options?: CallOptions,
    ): Promise<{ readonly elements: readonly MilitaryInstallationItem[] }> => ({
      elements: [],
    }),
  );

  airQuality = vi.fn(
    async (_options?: CallOptions): Promise<import('../contracts').AirQualitySnapshot> => ({
      readings: [],
      freshness: { state: 'live' },
      updatedAt: '2026-10-25T12:00:00Z',
    }),
  );

  resolveEntity = vi.fn(
    async (
      scheme: string,
      value: string,
      _options?: CallOptions,
    ): Promise<import('../contracts').CanonicalEntity> => ({
      id: `canonical:${scheme}:${value}`,
      kind: 'aircraft',
      primaryLabel: value,
      identifiers: [{ scheme, value }],
      attributes: {},
      firstSeen: 1_729_000_000n,
      lastSeen: 1_729_000_000n,
      confidence: 1.0,
    }),
  );

  getEntity = vi.fn(
    async (
      id: string,
      _options?: CallOptions,
    ): Promise<import('../contracts').CanonicalEntity> => ({
      id,
      kind: 'aircraft',
      primaryLabel: id,
      identifiers: [],
      attributes: {},
      firstSeen: 1_729_000_000n,
      lastSeen: 1_729_000_000n,
      confidence: 1.0,
    }),
  );

  getEntityAliases = vi.fn(
    async (
      _id: string,
      _options?: CallOptions,
    ): Promise<readonly import('../contracts').EntityIdentifier[]> => [],
  );

  createSavedSearch = vi.fn(
    async (
      search: import('../contracts').SavedSearch,
      _options?: CallOptions,
    ): Promise<import('../contracts').SavedSearch> => search,
  );

  listSavedSearches = vi.fn(
    async (_options?: CallOptions): Promise<readonly import('../contracts').SavedSearch[]> => [],
  );

  getSavedSearch = vi.fn(
    async (id: string, _options?: CallOptions): Promise<import('../contracts').SavedSearch> => ({
      id,
      name: id,
      query: 'test',
      notifyOnMatch: false,
      createdAt: '2026-10-25T12:00:00Z',
    }),
  );

  deleteSavedSearch = vi.fn(async (_id: string, _options?: CallOptions): Promise<void> => {});

  savedSearchMatches = vi.fn(
    async (
      _id: string,
      _limit?: number,
      _options?: CallOptions,
    ): Promise<readonly import('../contracts').SavedSearchMatch[]> => [],
  );

  scenePacks = vi.fn(
    async (_options?: CallOptions): Promise<readonly import('../contracts').EvidencePack[]> => [],
  );

  mapCatalog = vi.fn(
    async (_options?: CallOptions): Promise<import('../contracts').MapCatalogSnapshot> => ({
      stacks: [],
    }),
  );

  secretStatus = vi.fn(
    async (_options?: CallOptions): Promise<SecretStatusResponse> => ({
      managedBy: 'memory',
      status: {
        firms: false,
        aisstream: false,
        opensky_id: false,
        opensky_secret: false,
        ll2: false,
        mapillary: false,
        tomtom: false,
      },
    }),
  );

  setSecret = vi.fn(
    async (_key: string, _value: string, _options?: CallOptions): Promise<void> => {},
  );

  clearSecret = vi.fn(async (_key: string, _options?: CallOptions): Promise<void> => {});

  subscribe = vi.fn((query: FeedQuery, onEvent: (e: FeedEvent) => void) => {
    const sub: FakeSub = { query, onEvent, closed: false, updates: [] };
    this.subs.push(sub);
    return {
      update: (q: FeedQuery) => void sub.updates.push(q),
      close: () => {
        sub.closed = true;
      },
    };
  });

  onTransportState(listener: (state: TransportState) => void): () => void {
    this.#transportListeners.add(listener);
    listener(this.#transport);
    return () => void this.#transportListeners.delete(listener);
  }

  setTransport(state: TransportState): void {
    this.#transport = state;
    for (const l of this.#transportListeners) l(state);
  }

  /** The open (not closed) subscription for a feed. */
  live(feed: FeedId): FakeSub | undefined {
    return this.subs.find((s) => s.query.feed === feed && !s.closed);
  }
}

export interface FakeGlobe {
  readonly globe: GlobeController & { [K in keyof GlobeController]: GlobeController[K] };
  pick(entity: EntityRef | null): void;
  view(view: ViewState): void;
  cursor(position: GeoPoint | null): void;
  cameraIntent(): void;
  point(point: GeoPoint | null): void;
}

export function fakeGlobe(): FakeGlobe {
  const pickers = new Set<(e: EntityRef | null) => void>();
  const viewers = new Set<(v: ViewState) => void>();
  const cursors = new Set<(p: GeoPoint | null) => void>();
  const pointListeners = new Set<(p: GeoPoint | null) => void>();
  const intents = new Set<() => void>();
  let pose: CameraPose = {
    lon: 10,
    lat: 20,
    heightM: 20_000_000,
    headingRad: 0,
    pitchRad: -Math.PI / 2,
    rollRad: 0,
  };
  let centre: GeoPoint | null = { lat: pose.lat, lon: pose.lon };
  const cameraIntent = () => {
    for (const listener of intents) listener();
  };
  const globe = {
    applySnapshot: vi.fn(),
    clearFeed: vi.fn(),
    setSelection: vi.fn(),
    showTrack: vi.fn(),
    showAircraftTrack: vi.fn(),
    showVesselTrack: vi.fn(),
    showTransitTrail: vi.fn(),
    setCables: vi.fn(),
    setCctvCameras: vi.fn(),
    setCctvDetections: vi.fn(),
    getCctvDetections: vi.fn(() => null),
    getCable: vi.fn(),
    getCctvCamera: vi.fn(),
    setRoute: vi.fn(),
    setRouteWaypoints: vi.fn(),
    selectRouteStep: vi.fn(),
    getRoute: vi.fn(() => null),
    setBoundaries: vi.fn(),
    setBoundaryClassVisibility: vi.fn(),
    getBoundaryClassVisibility: vi.fn(() => ({
      internationalLand: true,
      disputedLine: true,
      breakawayArea: true,
      maritimeIndicative: true,
    })),
    locateBoundary: vi.fn(),
    getBoundaryFeature: vi.fn(),
    startFollowing: vi.fn(),
    enterCockpit: vi.fn(),
    exitCockpit: vi.fn(),
    isCockpitActive: vi.fn(() => false),
    stopFollowing: vi.fn(),
    isFollowing: vi.fn(() => false),
    getFollowState: vi.fn(() => null),
    onFollowChange: vi.fn((_l: unknown) => () => {}),
    setImagery: vi.fn(),
    setAnnotations: vi.fn(),
    setDrawing: vi.fn(),
    onMapPoint: (listener: (p: GeoPoint | null) => void) => {
      pointListeners.add(listener);
      return () => void pointListeners.delete(listener);
    },
    flyTo: vi.fn(cameraIntent),
    home: vi.fn(cameraIntent),
    zoomIn: vi.fn(cameraIntent),
    zoomOut: vi.fn(cameraIntent),
    resetNorth: vi.fn(cameraIntent),
    cameraPose: vi.fn(() => pose),
    centrePosition: vi.fn(() => centre),
    restoreCamera: vi.fn((next: CameraPose) => {
      pose = next;
    }),
    onCameraIntent: (listener: () => void) => {
      intents.add(listener);
      return () => void intents.delete(listener);
    },
    onPick: (l: (e: EntityRef | null) => void) => {
      pickers.add(l);
      return () => void pickers.delete(l);
    },
    onViewChange: (l: (v: ViewState) => void) => {
      viewers.add(l);
      return () => void viewers.delete(l);
    },
    onCursor: (l: (p: GeoPoint | null) => void) => {
      cursors.add(l);
      return () => void cursors.delete(l);
    },
    captureFrame: vi.fn(async () => null),
    setRecentImageryBox: vi.fn(),
    setRecentImagerySlot: vi.fn(),
    setRecentImageryMode: vi.fn(),
    setRecentImageryOpacity: vi.fn(),
    setRecentImagerySplitPosition: vi.fn(),
    startImageryBoxDrawing: vi.fn(),
    cancelImageryBoxDrawing: vi.fn(),
    isImageryBoxDrawing: vi.fn(() => false),
    setHeatmapContacts: vi.fn(),
    setHeatmapVisible: vi.fn(),
    setHeatmapOpacity: vi.fn(),
    clearHeatmap: vi.fn(),
    setAirQualityReadings: vi.fn(),
    getAirQualityReading: vi.fn(() => undefined),
    clearAirQuality: vi.fn(),
    setGnssCells: vi.fn(),
    getGnssCell: vi.fn(() => undefined),
    clearGnss: vi.fn(),
    setRadioStations: vi.fn(),
    getRadioStation: vi.fn(() => undefined),
    clearRadio: vi.fn(),
    setAlprCameras: vi.fn(),
    getAlprCamera: vi.fn(() => undefined),
    clearAlpr: vi.fn(),
    destroy: vi.fn(),
  } satisfies GlobeController;
  return {
    globe,
    cameraIntent,
    point: (p) => {
      for (const listener of pointListeners) listener(p);
    },
    pick: (e) => {
      for (const l of pickers) l(e);
    },
    view: (v) => {
      centre = v.centre;
      for (const l of viewers) l(v);
    },
    cursor: (p) => {
      for (const l of cursors) l(p);
    },
  };
}
