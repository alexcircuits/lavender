/**
 * One behavioural contract, two transports (ADR 0005): every `LavenderPlatform` adapter
 * must pass the same suite. The web adapter runs against a fake HTTP server and fake
 * WebSocket; the desktop adapter against a fake Tauri `invoke` and channels.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ClientFrame, FeedQuery, FeedSnapshot, ServerFrame } from '../contracts';
import { type ChannelLike, TauriPlatform } from './tauri/TauriPlatform';
import {
  type FeedEvent,
  type LavenderPlatform,
  PlatformError,
  type PreferenceStore,
} from './types';
import { feedParams, type SocketLike, WebPlatform } from './web/WebPlatform';

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

/** A tiny in-memory backend both fakes delegate to. */
class FakeBackend {
  readonly subscriptions = new Map<string, { query: FeedQuery; push: (f: ServerFrame) => void }>();
  failCatalog = false;

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

function webAdapter(backend: FakeBackend): LavenderPlatform {
  const fetch = async (url: string): Promise<Response> => {
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

  it('reports the transport state immediately', () => {
    const platform = make(new FakeBackend());
    const states: string[] = [];
    platform.onTransportState((s) => states.push(s))();
    expect(states).toHaveLength(1);
  });
});

describe('web transport specifics', () => {
  afterEach(() => vi.useRealTimers());

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
