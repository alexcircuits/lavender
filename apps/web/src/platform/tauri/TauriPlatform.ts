/**
 * The desktop transport (ADR 0005, docs/architecture/tauri.md §4): Tauri commands call the
 * `LavenderCore` embedded in the app; live feeds arrive over one `Channel` per subscription
 * carrying the same `ServerFrame` values the web stream sends. Only `src/platform` may
 * import `@tauri-apps/*` (src/architecture.test.ts).
 */
import type {
  Catalog,
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
  type DiagnosticsSink,
  type FeedEvent,
  type FeedSubscription,
  type LavenderPlatform,
  PlatformError,
  type PreferenceStore,
  type TransportState,
  toApiError,
} from '../types';

export type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

/** A Tauri `Channel`, reduced to what subscriptions need (injectable for tests). */
export interface ChannelLike<T> {
  onmessage: (message: T) => void;
}

export type ChannelFactory = <T>() => ChannelLike<T>;

export interface TauriPlatformOptions {
  readonly invoke: Invoke;
  readonly channel: ChannelFactory;
  readonly preferences?: PreferenceStore;
}

export class TauriPlatform implements LavenderPlatform {
  readonly kind = 'desktop' as const;
  readonly preferences: PreferenceStore;
  readonly diagnostics: DiagnosticsSink;
  readonly #invoke: Invoke;
  readonly #channel: ChannelFactory;
  #nextId = 0;

  constructor(options: TauriPlatformOptions) {
    this.#invoke = options.invoke;
    this.#channel = options.channel;
    this.preferences = options.preferences ?? new LocalPreferences();
    this.diagnostics = {
      report: (report) => this.#call<void>('smoke_report', { report }),
    };
  }

  async #call<T>(
    command: string,
    args?: Record<string, unknown>,
    options?: CallOptions,
  ): Promise<T> {
    if (options?.signal?.aborted)
      throw new PlatformError(toApiError(new DOMException('aborted', 'AbortError')));
    try {
      return await this.#invoke<T>(command, args);
    } catch (error) {
      throw new PlatformError(toApiError(error));
    }
  }

  catalog(options?: CallOptions): Promise<Catalog> {
    return this.#call('catalog', undefined, options);
  }

  status(options?: CallOptions): Promise<SystemStatus> {
    return this.#call('status', undefined, options);
  }

  snapshot(query: FeedQuery, options?: CallOptions): Promise<FeedSnapshot> {
    return this.#call('feed_snapshot', { query }, options);
  }

  search(query: SearchQuery, options?: CallOptions): Promise<SearchResults> {
    return this.#call('search', { query }, options);
  }

  satelliteTrack(request: SatelliteTrackRequest, options?: CallOptions): Promise<SatelliteTrack> {
    return this.#call('satellite_track', { request }, options);
  }

  /** IPC to an in-process core has no connection to lose. */
  onTransportState(listener: (state: TransportState) => void): () => void {
    listener('live');
    return () => {};
  }

  subscribe(query: FeedQuery, onEvent: (event: FeedEvent) => void): FeedSubscription {
    this.#nextId += 1;
    const id = `d${this.#nextId}`;
    let closed = false;
    const channel = this.#channel<ServerFrame>();
    channel.onmessage = (frame) => {
      if (closed) return;
      if (frame.type === 'snapshot') onEvent({ kind: 'snapshot', snapshot: frame.snapshot });
      else onEvent({ kind: 'error', error: frame.error });
    };
    const report = (error: unknown): void => {
      if (!closed) onEvent({ kind: 'error', error: toApiError(error) });
    };
    this.#invoke('feed_subscribe', { id, query, channel }).catch(report);
    return {
      update: (next) => {
        this.#invoke('feed_update', { id, query: next }).catch(report);
      },
      close: () => {
        closed = true;
        this.#invoke('feed_unsubscribe', { id }).catch(() => {});
      },
    };
  }
}
