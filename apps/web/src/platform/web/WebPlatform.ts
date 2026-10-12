/**
 * The web transport (ADR 0005): HTTP for request/response calls and one multiplexed
 * WebSocket (`/api/v1/stream`) for live feeds. If the stream cannot be (re)established, each
 * subscription falls back to polling snapshots and the transport state says `polling`.
 */
import type {
  Catalog,
  ClientFrame,
  FeedQuery,
  FeedSnapshot,
  SatelliteTrack,
  SatelliteTrackRequest,
  SearchQuery,
  SearchResults,
  ServerFrame,
  SystemStatus,
} from '../../contracts';
import { LocalPreferences } from '../preferences';
import {
  type CallOptions,
  type FeedEvent,
  type FeedSubscription,
  isApiError,
  type LavenderPlatform,
  PlatformError,
  type PreferenceStore,
  type SubscribeOptions,
  type TransportState,
  toApiError,
} from '../types';

type FetchLike = (input: string, init?: RequestInit) => Promise<Response>;

/** The subset of the WebSocket API the stream uses (injectable for tests). */
export interface SocketLike {
  readonly readyState: number;
  onopen: ((ev: unknown) => void) | null;
  onclose: ((ev: unknown) => void) | null;
  onerror: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  send(data: string): void;
  close(): void;
}

export type SocketFactory = (url: string) => SocketLike;

export interface WebPlatformOptions {
  /** API origin; empty means same origin (the normal deployment). */
  readonly baseUrl?: string;
  readonly fetch?: FetchLike;
  readonly socket?: SocketFactory;
  readonly preferences?: PreferenceStore;
  /** Failed connection attempts before subscriptions switch to polling. */
  readonly attemptsBeforePolling?: number;
  /** Reconnect backoff bounds in milliseconds. */
  readonly backoffMs?: { readonly min: number; readonly max: number };
}

const OPEN = 1;

interface Entry {
  query: FeedQuery;
  readonly onEvent: (event: FeedEvent) => void;
  readonly pollIntervalS: number;
  pollTimer: ReturnType<typeof setTimeout> | null;
}

export class WebPlatform implements LavenderPlatform {
  readonly kind = 'web' as const;
  readonly preferences: PreferenceStore;
  readonly #base: string;
  readonly #fetch: FetchLike;
  readonly #socketFactory: SocketFactory;
  readonly #attemptsBeforePolling: number;
  readonly #backoff: { readonly min: number; readonly max: number };
  readonly #etags = new Map<string, { etag: string; body: unknown }>();
  readonly #entries = new Map<string, Entry>();
  readonly #transportListeners = new Set<(state: TransportState) => void>();
  #socket: SocketLike | null = null;
  #state: TransportState = 'connecting';
  #failures = 0;
  #reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  #nextId = 0;

  constructor(options: WebPlatformOptions = {}) {
    this.#base = options.baseUrl ?? '';
    this.#fetch = options.fetch ?? ((input, init) => globalThis.fetch(input, init));
    this.#socketFactory = options.socket ?? ((url) => new WebSocket(url) as unknown as SocketLike);
    this.preferences = options.preferences ?? new LocalPreferences();
    this.#attemptsBeforePolling = options.attemptsBeforePolling ?? 3;
    this.#backoff = options.backoffMs ?? { min: 500, max: 30_000 };
  }

  // ── request / response ──────────────────────────────────────────────────────────────

  async #get<T>(path: string, options?: CallOptions): Promise<T> {
    const url = `${this.#base}/api/v1${path}`;
    const cached = this.#etags.get(url);
    const headers: Record<string, string> = { Accept: 'application/json' };
    if (cached) headers['If-None-Match'] = cached.etag;
    let response: Response;
    try {
      response = await this.#fetch(url, {
        headers,
        ...(options?.signal ? { signal: options.signal } : {}),
      });
    } catch (error) {
      throw new PlatformError(toApiError(error));
    }
    if (response.status === 304 && cached) return cached.body as T;
    const body: unknown = await response.json().catch(() => undefined);
    if (!response.ok) {
      throw new PlatformError(
        isApiError(body)
          ? body
          : {
              code: 'internal',
              message: `The server answered HTTP ${response.status}.`,
              retryable: response.status >= 500,
            },
      );
    }
    const etag = response.headers.get('ETag');
    if (etag) this.#etags.set(url, { etag, body });
    return body as T;
  }

  catalog(options?: CallOptions): Promise<Catalog> {
    return this.#get('/catalog', options);
  }

  status(options?: CallOptions): Promise<SystemStatus> {
    return this.#get('/status', options);
  }

  snapshot(query: FeedQuery, options?: CallOptions): Promise<FeedSnapshot> {
    return this.#get(`/feeds/${query.feed}${feedParams(query)}`, options);
  }

  search(query: SearchQuery, options?: CallOptions): Promise<SearchResults> {
    const params = new URLSearchParams({ q: query.q });
    if (query.limit !== undefined) params.set('limit', String(query.limit));
    if (query.near) params.set('near', `${query.near.lat},${query.near.lon}`);
    return this.#get(`/search?${params.toString()}`, options);
  }

  satelliteTrack(request: SatelliteTrackRequest, options?: CallOptions): Promise<SatelliteTrack> {
    const params = new URLSearchParams();
    if (request.minutes !== undefined) params.set('minutes', String(request.minutes));
    if (request.stepS !== undefined) params.set('stepS', String(request.stepS));
    if (request.from !== undefined) params.set('from', request.from);
    const qs = params.toString();
    return this.#get(`/satellites/${request.noradId}/track${qs ? `?${qs}` : ''}`, options);
  }

  // ── live stream ─────────────────────────────────────────────────────────────────────

  onTransportState(listener: (state: TransportState) => void): () => void {
    this.#transportListeners.add(listener);
    listener(this.#state);
    return () => this.#transportListeners.delete(listener);
  }

  #setState(state: TransportState): void {
    if (state === this.#state) return;
    this.#state = state;
    for (const l of this.#transportListeners) l(state);
  }

  subscribe(
    query: FeedQuery,
    onEvent: (event: FeedEvent) => void,
    options?: SubscribeOptions,
  ): FeedSubscription {
    this.#nextId += 1;
    const id = `s${this.#nextId}`;
    const entry: Entry = {
      query,
      onEvent,
      pollIntervalS: options?.pollIntervalS ?? 30,
      pollTimer: null,
    };
    this.#entries.set(id, entry);
    if (this.#state === 'polling') this.#startPolling(id, entry);
    this.#ensureSocket();
    this.#send({ op: 'subscribe', id, query });
    return {
      update: (next) => {
        entry.query = next;
        if (this.#state === 'polling') this.#poll(entry);
        this.#send({ op: 'update', id, query: next });
      },
      close: () => {
        this.#stopPolling(entry);
        this.#entries.delete(id);
        this.#send({ op: 'unsubscribe', id });
        if (this.#entries.size === 0) this.#closeSocket();
      },
    };
  }

  #streamUrl(): string {
    const origin = this.#base || globalThis.location?.origin || 'http://localhost';
    return `${origin.replace(/^http/, 'ws')}/api/v1/stream`;
  }

  #ensureSocket(): void {
    if (this.#socket || this.#reconnectTimer) return;
    if (this.#state !== 'polling') this.#setState('connecting');
    let socket: SocketLike;
    try {
      socket = this.#socketFactory(this.#streamUrl());
    } catch {
      this.#scheduleReconnect();
      return;
    }
    this.#socket = socket;
    socket.onopen = () => {
      this.#failures = 0;
      for (const [id, entry] of this.#entries) {
        this.#stopPolling(entry);
        socket.send(
          JSON.stringify({ op: 'subscribe', id, query: entry.query } satisfies ClientFrame),
        );
      }
      this.#setState('live');
    };
    socket.onmessage = (ev) => this.#receive(ev.data);
    socket.onerror = () => {};
    socket.onclose = () => {
      if (this.#socket !== socket) return;
      this.#socket = null;
      if (this.#entries.size === 0) return;
      this.#failures += 1;
      if (this.#failures >= this.#attemptsBeforePolling && this.#state !== 'polling') {
        this.#setState('polling');
        for (const [id, entry] of this.#entries) this.#startPolling(id, entry);
      } else if (this.#state === 'live') {
        this.#setState('connecting');
      }
      this.#scheduleReconnect();
    };
  }

  #scheduleReconnect(): void {
    if (this.#reconnectTimer) return;
    const exp = Math.min(this.#backoff.max, this.#backoff.min * 2 ** Math.min(this.#failures, 10));
    const delay = exp / 2 + Math.random() * (exp / 2);
    this.#reconnectTimer = setTimeout(() => {
      this.#reconnectTimer = null;
      if (this.#entries.size > 0) this.#ensureSocket();
    }, delay);
  }

  #closeSocket(): void {
    if (this.#reconnectTimer) clearTimeout(this.#reconnectTimer);
    this.#reconnectTimer = null;
    const socket = this.#socket;
    this.#socket = null;
    socket?.close();
  }

  #send(frame: ClientFrame): void {
    if (this.#socket?.readyState === OPEN) this.#socket.send(JSON.stringify(frame));
  }

  #receive(data: unknown): void {
    if (typeof data !== 'string') return;
    let frame: ServerFrame;
    try {
      frame = JSON.parse(data) as ServerFrame;
    } catch {
      return;
    }
    if (frame.type === 'snapshot') {
      this.#entries.get(frame.id)?.onEvent({ kind: 'snapshot', snapshot: frame.snapshot });
    } else if (frame.type === 'error' && frame.id !== undefined) {
      this.#entries.get(frame.id)?.onEvent({ kind: 'error', error: frame.error });
    }
  }

  // ── polling fallback ────────────────────────────────────────────────────────────────

  #startPolling(_id: string, entry: Entry): void {
    if (entry.pollTimer) return;
    const tick = (): void => {
      void this.#poll(entry).finally(() => {
        if (this.#state === 'polling' && entry.pollTimer !== null) {
          entry.pollTimer = setTimeout(tick, entry.pollIntervalS * 1000);
        }
      });
    };
    entry.pollTimer = setTimeout(tick, 0);
  }

  async #poll(entry: Entry): Promise<void> {
    try {
      entry.onEvent({ kind: 'snapshot', snapshot: await this.snapshot(entry.query) });
    } catch (error) {
      entry.onEvent({ kind: 'error', error: toApiError(error) });
    }
  }

  #stopPolling(entry: Entry): void {
    if (entry.pollTimer) clearTimeout(entry.pollTimer);
    entry.pollTimer = null;
  }
}

/** `?bbox=w,s,e,n&minMagnitude=…` for a feed query (matches lavender-server's routes). */
export function feedParams(query: FeedQuery): string {
  const params = new URLSearchParams();
  const b = query.bbox;
  if (b) params.set('bbox', `${b.west},${b.south},${b.east},${b.north}`);
  const f = query.filters;
  if (f.minMagnitude !== undefined) params.set('minMagnitude', String(f.minMagnitude));
  if (f.militaryOnly !== undefined) params.set('militaryOnly', String(f.militaryOnly));
  if (f.groups !== undefined) params.set('groups', f.groups.join(','));
  const qs = params.toString();
  return qs ? `?${qs}` : '';
}
