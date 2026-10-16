/**
 * One behavioural contract, two transports (ADR 0005): every `LavenderPlatform` adapter
 * must pass the same suite. The web adapter runs against a fake HTTP server and fake
 * WebSocket; the desktop adapter against a fake Tauri `invoke` and channels.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import type {
  ClientFrame,
  FeedQuery,
  FeedSnapshot,
  ServerFrame,
  VesselTrackResponse,
  WeatherOverlay,
  WindManifest,
} from '../contracts';
import { type ChannelLike, TauriPlatform } from './tauri/TauriPlatform';
import {
  type FeedEvent,
  type LavenderPlatform,
  PlatformError,
  type PreferenceStore,
  type WindRequest,
} from './types';
import { feedParams, type SocketLike, WebPlatform, windParams } from './web/WebPlatform';
import { decodeWindGrid } from './windGrid';

const snapshot = (feed: FeedSnapshot['feed'], count: number): FeedSnapshot => ({
  feed,
  sources: ['test'],
  generatedAt: '2026-10-11T12:00:00Z',
  freshness: { state: 'live' },
  coverage: { kind: 'complete' },
  sourceCount: count,
  pendingRegions: 0,
  items: { kind: 'earthquakes', items: [] },
  notices: [],
});

const memoryPrefs = (): PreferenceStore => {
  const m = new Map<string, unknown>();
  return {
    get: <T>(k: string) => m.get(k) as T | undefined,
    set: (k, v) => void m.set(k, v),
    remove: (k) => void m.delete(k),
  };
};

const GRID_ID = 'gfs-20261015-0-f6-180';

/** A 2 × 2 grid (180° spacing) as lavender-core serves it: little-endian f32, U then V. */
function windGridBytes(): ArrayBuffer {
  const view = new DataView(new ArrayBuffer(2 * 4 * 4));
  for (const [i, value] of [1.5, -2, 0.25, 3, 4, -5, 6.5, -7].entries()) {
    view.setFloat32(i * 4, value, true);
  }
  return view.buffer;
}

const windManifest = (request: WindRequest, withGrid: boolean): WindManifest => ({
  schemaVersion: 1,
  model: request.model,
  cycle: { date: '20261015', hour: 0, forecastHour: 6 },
  grid: { nx: 2, ny: 2, lo1: 0, la1: 90, dx: 180, dy: 180 },
  stale: false,
  unavailable: false,
  reason: null,
  ...(withGrid
    ? { gridId: GRID_ID, gridUrl: `/api/v1/wind/grid/${GRID_ID}.bin?model=${request.model}` }
    : {}),
  attribution: 'Wind: NOAA Global Forecast System (GFS)',
});

const unknownGrid = { code: 'notFound', message: 'unknown_grid', retryable: false };

/** The one vessel the fake backend has kept a track for (synthetic MMSI and positions). */
const TRACKED_MMSI = '230990001';
const vesselTrack: VesselTrackResponse = {
  mmsi: TRACKED_MMSI,
  samples: [
    { lat: 60.1, lon: 24.9, epochSec: 1_791_892_200 },
    { lat: 60.1, lon: 24.91, epochSec: 1_791_892_260 },
  ],
  source: 'aisstream',
  retainedSec: 1_800,
  voyage: {
    callSign: 'XTEST',
    navigationalStatus: 'At anchor',
    eta: { month: 10, day: 15, hour: 18, minute: 0 },
    draughtM: 6.2,
    dimensions: { lengthM: 70, beamM: 12 },
  },
};
const noTrack = {
  code: 'notFound',
  message: 'No retained track is available for vessel 230990002.',
  retryable: false,
};

type WindKind = 'manifest' | 'status' | 'grid';
type WindCall = WindRequest & { readonly id?: string };

/** A tiny in-memory backend both fakes delegate to. */
class FakeBackend {
  readonly subscriptions = new Map<string, { query: FeedQuery; push: (f: ServerFrame) => void }>();
  readonly windRequests: unknown[] = [];
  readonly vesselTrackRequests: string[] = [];
  failCatalog = false;
  /** Wind answers start but break off: the HTTP body errors, the IPC call rejects. */
  breakWindTransfers = false;

  catalog() {
    if (this.failCatalog) throw { code: 'upstreamUnavailable', message: 'down', retryable: true };
    return { layers: [], sources: [], imagery: [], licenceMode: 'evaluation' };
  }

  subscribe(id: string, query: FeedQuery, push: (f: ServerFrame) => void) {
    this.subscriptions.set(id, { query, push });
    push({ type: 'snapshot', id, snapshot: snapshot(query.feed, 1) });
  }

  update(id: string, query: FeedQuery) {
    const s = this.subscriptions.get(id);
    if (!s) return;
    s.query = query;
    s.push({ type: 'snapshot', id, snapshot: snapshot(query.feed, 2) });
  }

  unsubscribe(id: string) {
    this.subscriptions.delete(id);
  }

  vesselTrack(mmsi: string): VesselTrackResponse {
    this.vesselTrackRequests.push(mmsi);
    if (mmsi !== TRACKED_MMSI) throw noTrack;
    return vesselTrack;
  }

  wind(request: WindCall, kind: WindKind) {
    this.windRequests.push({ kind, ...request });
    if (kind === 'grid') {
      if (request.id !== GRID_ID) throw unknownGrid;
      return windGridBytes();
    }
    return windManifest(request, kind === 'manifest');
  }
}

class FakeSocket implements SocketLike {
  readyState = 0;
  onopen: ((ev: unknown) => void) | null = null;
  onclose: ((ev: unknown) => void) | null = null;
  onerror: ((ev: unknown) => void) | null = null;
  onmessage: ((ev: { data: unknown }) => void) | null = null;
  readonly sent: ClientFrame[] = [];

  constructor(readonly backend: FakeBackend) {
    queueMicrotask(() => {
      this.readyState = 1;
      this.onopen?.({});
    });
  }

  send(data: string) {
    const frame = JSON.parse(data) as ClientFrame;
    this.sent.push(frame);
    const push = (f: ServerFrame) =>
      queueMicrotask(() => this.onmessage?.({ data: JSON.stringify(f) }));
    if (frame.op === 'subscribe') this.backend.subscribe(frame.id, frame.query, push);
    if (frame.op === 'update') this.backend.update(frame.id, frame.query);
    if (frame.op === 'unsubscribe') this.backend.unsubscribe(frame.id);
  }

  close() {
    this.readyState = 3;
  }
}

/** A 200 whose body fails after its first bytes, as a dropped or aborted download does. */
function brokenBody(reason: Error): Response {
  const body = new ReadableStream<Uint8Array>({
    pull(controller) {
      controller.enqueue(new Uint8Array([0x7b, 0x22]));
      controller.error(reason);
    },
  });
  return new Response(body, { status: 200 });
}

/** Parses `/api/v1/wind/{kind}[/{id}.bin]?model=…&overlay=…` like lavender-server. */
function webWind(backend: FakeBackend, url: string): Response {
  const parsed = new URL(url, 'http://localhost');
  const match = /^\/api\/v1\/wind\/(manifest|status|grid)(?:\/(.+)\.bin)?$/.exec(parsed.pathname);
  if (!match) return new Response('{}', { status: 404 });
  const kind = match[1] as WindKind;
  const id = match[2];
  const model = parsed.searchParams.get('model') as WindRequest['model'];
  const overlay = (parsed.searchParams.get('overlay') ?? 'none') as WeatherOverlay;
  try {
    const answer = backend.wind(
      { model, overlay, ...(id ? { id: decodeURIComponent(id) } : {}) },
      kind,
    );
    if (backend.breakWindTransfers) return brokenBody(new TypeError('network connection lost'));
    return answer instanceof ArrayBuffer
      ? new Response(answer, { headers: { 'Content-Type': 'application/octet-stream' } })
      : new Response(JSON.stringify(answer));
  } catch (error) {
    return new Response(JSON.stringify({ error: 'unknown_grid', ...(error as object) }), {
      status: 404,
    });
  }
}

function webAdapter(backend: FakeBackend): LavenderPlatform {
  const fetch = async (url: string, init?: RequestInit): Promise<Response> => {
    // Like the browser's fetch: an already-aborted signal rejects before any request.
    if (init?.signal?.aborted) throw new DOMException('The operation was aborted.', 'AbortError');
    if (url.includes('/api/v1/wind/')) return webWind(backend, url);
    if (url.includes('/api/v1/satellites/25544/passes'))
      return new Response(
        JSON.stringify({
          noradId: 25544,
          name: 'ISS',
          observer: { lat: 60, lon: 24 },
          from: '2026-10-13T12:00:00Z',
          until: '2026-10-14T12:00:00Z',
          minElevationDeg: 0,
          passes: [],
        }),
      );
    const vessel = /\/api\/v1\/vessels\/([^/?]+)\/track$/.exec(url);
    if (vessel?.[1]) {
      try {
        return new Response(JSON.stringify(backend.vesselTrack(decodeURIComponent(vessel[1]))));
      } catch (error) {
        return new Response(JSON.stringify(error), { status: 404 });
      }
    }
    if (url.includes('/api/v1/aircraft/'))
      return new Response(
        JSON.stringify({
          hex: '4ca1b2',
          points: [{ time: '2026-10-13T12:00:00Z', position: { lat: 60, lon: 24 } }],
          sources: ['adsb-lol'],
        }),
        { status: 200 },
      );
    if (url.endsWith('/api/v1/catalog')) {
      try {
        return new Response(JSON.stringify(backend.catalog()), { status: 200 });
      } catch (e) {
        return new Response(JSON.stringify(e), { status: 502 });
      }
    }
    if (url.includes('/api/v1/feeds/earthquakes'))
      return new Response(JSON.stringify(snapshot('earthquakes', 5)), { status: 200 });
    return new Response(JSON.stringify({ code: 'notFound', message: 'no', retryable: false }), {
      status: 404,
    });
  };
  return new WebPlatform({
    fetch,
    socket: () => new FakeSocket(backend),
    preferences: memoryPrefs(),
  });
}

function desktopAdapter(backend: FakeBackend): LavenderPlatform {
  const invoke = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    switch (command) {
      case 'catalog':
        return backend.catalog() as T;
      case 'feed_snapshot':
        return snapshot('earthquakes', 5) as T;
      case 'aircraft_track':
        return {
          hex: args?.hex,
          points: [{ time: '2026-10-13T12:00:00Z', position: { lat: 60, lon: 24 } }],
          sources: ['adsb-lol'],
        } as T;
      case 'vessel_track':
        return backend.vesselTrack(args?.mmsi as string) as T;
      case 'satellite_passes':
        return {
          noradId: 25544,
          name: 'ISS',
          observer: { lat: 60, lon: 24 },
          from: '2026-10-13T12:00:00Z',
          until: '2026-10-14T12:00:00Z',
          minElevationDeg: 0,
          passes: [],
        } as T;
      case 'feed_subscribe': {
        const channel = args?.channel as ChannelLike<ServerFrame>;
        backend.subscribe(args?.id as string, args?.query as FeedQuery, (f) =>
          queueMicrotask(() => channel.onmessage(f)),
        );
        return undefined as T;
      }
      case 'feed_update':
        backend.update(args?.id as string, args?.query as FeedQuery);
        return undefined as T;
      case 'feed_unsubscribe':
        backend.unsubscribe(args?.id as string);
        return undefined as T;
      case 'wind_manifest':
      case 'wind_status':
      case 'wind_grid':
        if (backend.breakWindTransfers) throw new TypeError('IPC connection lost');
        return backend.wind(
          args as unknown as WindCall,
          command.slice('wind_'.length) as WindKind,
        ) as T;
      default:
        throw { code: 'notFound', message: `no command ${command}`, retryable: false };
    }
  };
  return new TauriPlatform({
    invoke,
    channel: <T>() => ({ onmessage: (_: T) => {} }),
    preferences: memoryPrefs(),
  });
}

const nextEvent = (events: FeedEvent[], count: number) =>
  vi.waitFor(() => expect(events.length).toBeGreaterThanOrEqual(count));

describe.each([
  ['web', webAdapter],
  ['desktop', desktopAdapter],
] as const)('%s platform contract', (_name, make) => {
  it('loads the catalog and reports failures as PlatformError with the envelope', async () => {
    const backend = new FakeBackend();
    const platform = make(backend);
    await expect(platform.catalog()).resolves.toMatchObject({ licenceMode: 'evaluation' });
    backend.failCatalog = true;
    const err = await platform.catalog().catch((e: unknown) => e);
    expect(err).toBeInstanceOf(PlatformError);
    expect((err as PlatformError).apiError.code).toBe('upstreamUnavailable');
  });

  it('takes one-shot snapshots', async () => {
    const platform = make(new FakeBackend());
    const s = await platform.snapshot({ feed: 'earthquakes', filters: {} });
    expect(s.sourceCount).toBe(5);
  });

  it('reads the same bounded aircraft history through either adapter', async () => {
    const platform = make(new FakeBackend());
    await expect(platform.aircraftTrack('4ca1b2')).resolves.toMatchObject({
      hex: '4ca1b2',
      points: [{ time: '2026-10-13T12:00:00Z', position: { lat: 60, lon: 24 } }],
      sources: ['adsb-lol'],
    });
  });

  it('reads the same retained vessel track through either adapter', async () => {
    const backend = new FakeBackend();
    const platform = make(backend);
    await expect(platform.vesselTrack(TRACKED_MMSI)).resolves.toEqual(vesselTrack);
    expect(backend.vesselTrackRequests).toEqual([TRACKED_MMSI]);
  });

  it('reports a vessel without a retained track as notFound', async () => {
    const platform = make(new FakeBackend());
    const err = await platform.vesselTrack('230990002').catch((e: unknown) => e);
    expect(err).toBeInstanceOf(PlatformError);
    expect((err as PlatformError).apiError).toEqual(noTrack);
  });

  it('does not start a vessel track request that was already cancelled', async () => {
    const backend = new FakeBackend();
    const platform = make(backend);
    const request = new AbortController();
    request.abort();
    const err = await platform
      .vesselTrack(TRACKED_MMSI, { signal: request.signal })
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(PlatformError);
    expect((err as PlatformError).apiError.message).toBe('The request was cancelled.');
  });

  it('predicts the same geometric satellite passes through either adapter', async () => {
    const platform = make(new FakeBackend());
    await expect(
      platform.satellitePasses({ noradId: 25544, observer: { lat: 60, lon: 24 }, hours: 24 }),
    ).resolves.toMatchObject({
      noradId: 25544,
      observer: { lat: 60, lon: 24 },
      minElevationDeg: 0,
      passes: [],
    });
  });

  it('subscribes, updates and unsubscribes', async () => {
    const backend = new FakeBackend();
    const platform = make(backend);
    const events: FeedEvent[] = [];
    const sub = platform.subscribe({ feed: 'earthquakes', filters: {} }, (e) => events.push(e));
    await nextEvent(events, 1);
    expect(events[0]).toMatchObject({ kind: 'snapshot', snapshot: { sourceCount: 1 } });
    sub.update({ feed: 'earthquakes', filters: { minMagnitude: 4 } });
    await nextEvent(events, 2);
    expect(events[1]).toMatchObject({ kind: 'snapshot', snapshot: { sourceCount: 2 } });
    expect([...backend.subscriptions.values()][0]?.query.filters.minMagnitude).toBe(4);
    sub.close();
    await vi.waitFor(() => expect(backend.subscriptions.size).toBe(0));
  });

  it('reads the same wind manifest and status through either adapter', async () => {
    const backend = new FakeBackend();
    const platform = make(backend);
    const manifest = await platform.windManifest({ model: 'gfs' });
    expect(manifest).toMatchObject({ model: 'gfs', gridId: GRID_ID, stale: false });
    const status = await platform.windStatus({ model: 'ifs', overlay: 'pressure' });
    expect(status).toMatchObject({ model: 'ifs', unavailable: false });
    expect(status.gridId).toBeUndefined();
    expect(backend.windRequests).toEqual([
      { kind: 'manifest', model: 'gfs', overlay: 'none' },
      { kind: 'status', model: 'ifs', overlay: 'pressure' },
    ]);
  });

  it('downloads the same little-endian wind grid through either adapter', async () => {
    const backend = new FakeBackend();
    const platform = make(backend);
    const manifest = await platform.windManifest({ model: 'gfs' });
    const buffer = await platform.windGrid({ model: 'gfs', id: manifest.gridId ?? '' });
    // 1.5 as binary32, least-significant byte first.
    expect([...new Uint8Array(buffer, 0, 4)]).toEqual([0x00, 0x00, 0xc0, 0x3f]);
    const grid = decodeWindGrid(buffer, manifest);
    expect([...grid.u]).toEqual([1.5, -2, 0.25, 3]);
    expect([...grid.v]).toEqual([4, -5, 6.5, -7]);
    expect(grid.scalar).toBeUndefined();
  });

  it('reports an unknown wind grid as notFound', async () => {
    const platform = make(new FakeBackend());
    const err = await platform.windGrid({ model: 'gfs', id: 'gone' }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(PlatformError);
    expect((err as PlatformError).apiError.code).toBe('notFound');
  });

  it('reports a wind transfer that breaks off as a retryable PlatformError', async () => {
    const backend = new FakeBackend();
    const platform = make(backend);
    backend.breakWindTransfers = true;
    const calls = [
      () => platform.windGrid({ model: 'gfs', id: GRID_ID }),
      () => platform.windManifest({ model: 'gfs' }),
      () => platform.windStatus({ model: 'ifs' }),
    ];
    for (const call of calls) {
      const err = await call().catch((e: unknown) => e);
      expect(err).toBeInstanceOf(PlatformError);
      expect((err as PlatformError).apiError).toMatchObject({
        code: 'upstreamUnavailable',
        retryable: true,
      });
    }
  });

  it('reports the transport state immediately', () => {
    const platform = make(new FakeBackend());
    const states: string[] = [];
    platform.onTransportState((s) => states.push(s))();
    expect(states).toHaveLength(1);
  });
});

describe('web transport specifics', () => {
  afterEach(() => vi.useRealTimers());

  it('marks place submission explicitly and safely encodes aircraft ids', async () => {
    const urls: string[] = [];
    const platform = new WebPlatform({
      fetch: async (url) => {
        urls.push(url);
        return new Response('{}');
      },
      preferences: memoryPrefs(),
      socket: () => new FakeSocket(new FakeBackend()),
    });
    await platform.search({ q: 'Helsinki', places: false });
    await platform.search({ q: 'Helsinki', places: true });
    await platform.aircraftTrack('a/b');
    await platform.vesselTrack('23099/0001?x');
    expect(urls[0]).toContain('places=false');
    expect(urls[1]).toContain('places=true');
    expect(urls[2]).toContain('/aircraft/a%2Fb/track');
    expect(urls[3]).toBe('/api/v1/vessels/23099%2F0001%3Fx/track');
  });

  it('builds feed query strings like lavender-server expects', () => {
    expect(
      feedParams({
        feed: 'aircraft',
        bbox: { west: 170, south: -10, east: -170, north: 10 },
        filters: { militaryOnly: true, groups: ['a', 'b'] },
      }),
    ).toBe('?bbox=170%2C-10%2C-170%2C10&militaryOnly=true&groups=a%2Cb');
    expect(feedParams({ feed: 'earthquakes', filters: {} })).toBe('');
  });

  it('requests wind grids at the gridUrl lavender-server publishes', async () => {
    expect(windParams({ model: 'gfs' })).toBe('?model=gfs');
    expect(windParams({ model: 'gfs', overlay: 'none' })).toBe('?model=gfs');
    expect(windParams({ model: 'ifs', overlay: 'temperature' })).toBe(
      '?model=ifs&overlay=temperature',
    );
    const urls: string[] = [];
    const accepts: string[] = [];
    const platform = new WebPlatform({
      fetch: async (url, init) => {
        urls.push(url);
        const headers = (init?.headers ?? {}) as Record<string, string>;
        accepts.push(headers.Accept ?? '');
        return new Response(new ArrayBuffer(8));
      },
      preferences: memoryPrefs(),
      socket: () => new FakeSocket(new FakeBackend()),
    });
    await platform.windGrid({ model: 'gfs', overlay: 'pressure', id: 'gfs-a/b' });
    expect(urls).toEqual(['/api/v1/wind/grid/gfs-a%2Fb.bin?model=gfs&overlay=pressure']);
    expect(accepts).toEqual(['application/octet-stream']);
  });

  it('reports a wind grid cancelled while its body streams as a cancelled PlatformError', async () => {
    const platform = new WebPlatform({
      fetch: async () => brokenBody(new DOMException('The operation was aborted.', 'AbortError')),
      preferences: memoryPrefs(),
      socket: () => new FakeSocket(new FakeBackend()),
    });
    const err = await platform.windGrid({ model: 'gfs', id: GRID_ID }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(PlatformError);
    expect((err as PlatformError).apiError.message).toBe('The request was cancelled.');
  });

  it('rejects a successful answer whose JSON cannot be read instead of resolving undefined', async () => {
    const platform = new WebPlatform({
      fetch: async () => new Response('{"schemaVersion": 1, "mo', { status: 200 }),
      preferences: memoryPrefs(),
      socket: () => new FakeSocket(new FakeBackend()),
    });
    const err = await platform.windManifest({ model: 'gfs' }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(PlatformError);
    expect((err as PlatformError).apiError.retryable).toBe(true);
  });

  it('revalidates with ETags and reuses the cached body on 304', async () => {
    const calls: Array<Record<string, string>> = [];
    const fetch = async (_url: string, init?: RequestInit): Promise<Response> => {
      const headers = (init?.headers ?? {}) as Record<string, string>;
      calls.push(headers);
      if (headers['If-None-Match'] === 'W/"1"') return new Response(null, { status: 304 });
      return new Response(JSON.stringify({ version: '1' }), {
        status: 200,
        headers: { ETag: 'W/"1"' },
      });
    };
    const p = new WebPlatform({
      fetch,
      preferences: memoryPrefs(),
      socket: () => new FakeSocket(new FakeBackend()),
    });
    await expect(p.status()).resolves.toEqual({ version: '1' });
    await expect(p.status()).resolves.toEqual({ version: '1' });
    expect(calls[1]?.['If-None-Match']).toBe('W/"1"');
  });

  it('reconnects with backoff and resubscribes with the latest query', async () => {
    vi.useFakeTimers();
    const backend = new FakeBackend();
    const sockets: FakeSocket[] = [];
    const p = new WebPlatform({
      fetch: async () => new Response('{}', { status: 200 }),
      socket: () => {
        const s = new FakeSocket(backend);
        sockets.push(s);
        return s;
      },
      preferences: memoryPrefs(),
      backoffMs: { min: 100, max: 1_000 },
    });
    const states: string[] = [];
    p.onTransportState((s) => states.push(s));
    const events: FeedEvent[] = [];
    const sub = p.subscribe({ feed: 'earthquakes', filters: {} }, (e) => events.push(e));
    await vi.advanceTimersByTimeAsync(0);
    expect(states.at(-1)).toBe('live');
    expect(events).toHaveLength(1);

    // The server drops the connection; an update made while disconnected must not be lost.
    const first = sockets[0];
    if (!first) throw new Error('no socket');
    first.readyState = 3;
    first.onclose?.({});
    expect(states.at(-1)).toBe('connecting');
    sub.update({ feed: 'earthquakes', filters: { minMagnitude: 5 } });
    await vi.advanceTimersByTimeAsync(250);

    expect(sockets).toHaveLength(2);
    expect(states.at(-1)).toBe('live');
    const resent = sockets[1]?.sent.find((f) => f.op === 'subscribe');
    expect(resent).toMatchObject({
      op: 'subscribe',
      id: 's1',
      query: { filters: { minMagnitude: 5 } },
    });
    expect(events.length).toBeGreaterThanOrEqual(2);
  });

  it('falls back to polling after repeated stream failures and says so', async () => {
    vi.useFakeTimers();
    let polls = 0;
    const fetch = async (): Promise<Response> => {
      polls += 1;
      return new Response(JSON.stringify(snapshot('earthquakes', 7)), { status: 200 });
    };
    const dead = (): SocketLike => {
      const s: SocketLike = {
        readyState: 3,
        onopen: null,
        onclose: null,
        onerror: null,
        onmessage: null,
        send: () => {},
        close: () => {},
      };
      queueMicrotask(() => s.onclose?.({}));
      return s;
    };
    const p = new WebPlatform({
      fetch,
      socket: dead,
      preferences: memoryPrefs(),
      attemptsBeforePolling: 2,
      backoffMs: { min: 10, max: 20 },
    });
    const states: string[] = [];
    p.onTransportState((s) => states.push(s));
    const events: FeedEvent[] = [];
    p.subscribe({ feed: 'earthquakes', filters: {} }, (e) => events.push(e), { pollIntervalS: 1 });
    await vi.advanceTimersByTimeAsync(200);
    expect(states).toContain('polling');
    expect(events[0]).toMatchObject({ kind: 'snapshot', snapshot: { sourceCount: 7 } });
    await vi.advanceTimersByTimeAsync(2_100);
    expect(polls).toBeGreaterThanOrEqual(2);
  });
});
