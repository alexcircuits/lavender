/**
 * The platform boundary (ADR 0005). Everything Lavender's interface needs from "outside"
 * goes through `LavenderPlatform`: the web build talks HTTP + WebSocket to lavender-server,
 * the desktop build talks Tauri IPC to the core embedded in the app. Components never
 * import fetch, WebSocket or @tauri-apps — only this interface (enforced by
 * src/architecture.test.ts).
 */
import type {
  AircraftTrack,
  ApiError,
  Catalog,
  FeedQuery,
  FeedSnapshot,
  SatellitePasses,
  SatellitePassRequest,
  SatelliteTrack,
  SatelliteTrackRequest,
  SearchQuery,
  SearchResults,
  SystemStatus,
} from '../contracts';

/** What a live subscription delivers. */
export type FeedEvent =
  | { readonly kind: 'snapshot'; readonly snapshot: FeedSnapshot }
  | { readonly kind: 'error'; readonly error: ApiError };

/**
 * How live data currently reaches the interface. `polling` means the stream is down and
 * snapshots are being fetched on an interval instead; the interface says so.
 */
export type TransportState = 'connecting' | 'live' | 'polling' | 'offline';

export interface FeedSubscription {
  /** Replaces the query (for example a new viewport). */
  update(query: FeedQuery): void;
  close(): void;
}

export interface SubscribeOptions {
  /** Fallback polling interval in seconds when the live stream is unavailable. */
  readonly pollIntervalS?: number;
}

/** Per-device preferences (panel widths, last camera, enabled layers). Never secrets. */
export interface PreferenceStore {
  get<T>(key: string): T | undefined;
  set<T>(key: string, value: T): void;
  remove(key: string): void;
}

export interface CallOptions {
  readonly signal?: AbortSignal;
}

export interface LavenderPlatform {
  readonly kind: 'web' | 'desktop';
  catalog(options?: CallOptions): Promise<Catalog>;
  status(options?: CallOptions): Promise<SystemStatus>;
  snapshot(query: FeedQuery, options?: CallOptions): Promise<FeedSnapshot>;
  subscribe(
    query: FeedQuery,
    onEvent: (event: FeedEvent) => void,
    options?: SubscribeOptions,
  ): FeedSubscription;
  search(query: SearchQuery, options?: CallOptions): Promise<SearchResults>;
  aircraftTrack(hex: string, options?: CallOptions): Promise<AircraftTrack>;
  satelliteTrack(request: SatelliteTrackRequest, options?: CallOptions): Promise<SatelliteTrack>;
  satellitePasses(request: SatellitePassRequest, options?: CallOptions): Promise<SatellitePasses>;
  /** Reports transport changes; calls the listener immediately with the current state. */
  onTransportState(listener: (state: TransportState) => void): () => void;
  readonly preferences: PreferenceStore;
  /**
   * Desktop only: hands a self-check report to the app, which writes it when automated
   * verification asked for one (`LAVENDER_SMOKE_OUT`) and ignores it otherwise.
   */
  readonly diagnostics?: DiagnosticsSink;
}

export interface DiagnosticsSink {
  report(report: Readonly<Record<string, unknown>>): Promise<void>;
}

/** A failure the platform could explain with Lavender's error envelope. */
export class PlatformError extends Error {
  readonly apiError: ApiError;

  constructor(apiError: ApiError) {
    super(apiError.message);
    this.name = 'PlatformError';
    this.apiError = apiError;
  }
}

/** Normalises anything thrown by a transport into Lavender's error envelope. */
export function toApiError(error: unknown): ApiError {
  if (error instanceof PlatformError) return error.apiError;
  if (isApiError(error)) return error;
  if (error instanceof DOMException && error.name === 'AbortError') {
    return { code: 'internal', message: 'The request was cancelled.', retryable: true };
  }
  return {
    code: 'upstreamUnavailable',
    message: 'Lavender could not reach its backend. Check the connection and try again.',
    retryable: true,
  };
}

export function isApiError(value: unknown): value is ApiError {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.code === 'string' && typeof v.message === 'string' && typeof v.retryable === 'boolean'
  );
}
