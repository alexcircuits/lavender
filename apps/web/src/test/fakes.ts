/**
 * Shared test doubles: a scriptable `LavenderPlatform`, a recording `GlobeController` and
 * small contract fixtures. Tests drive feeds by calling `platform.subs[i].onEvent(...)`.
 */
import { vi } from 'vitest';
import type {
  AircraftTrack,
  Catalog,
  FeedId,
  FeedQuery,
  FeedSnapshot,
  GeoPoint,
  ImageryDescriptor,
  LayerDescriptor,
  SatellitePasses,
  SatellitePassRequest,
  SatelliteTrack,
  SatelliteTrackRequest,
  SearchQuery,
  SearchResults,
  SystemStatus,
} from '../contracts';
import type { CameraPose, EntityRef, GlobeController, ViewState } from '../globe';
import type {
  CallOptions,
  FeedEvent,
  LavenderPlatform,
  PreferenceStore,
  TransportState,
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
