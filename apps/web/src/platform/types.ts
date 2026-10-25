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
  BBox,
  BoundaryClass,
  BoundaryResponse,
  CameraSource,
  Catalog,
  CctvDetectionsResponse,
  FeedQuery,
  FeedSnapshot,
  GeoPoint,
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
  WeatherOverlay,
  WindManifest,
  WindModel,
} from '../contracts';
import type { RecentImageryResponse, ThumbnailProbeResponse } from './recentImageryTypes';

/** Submarine cable item as returned by the cables API. */
export interface SubmarineCableApiItem {
  readonly id: string;
  readonly name: string;
  readonly color?: string | undefined;
  readonly coordinates: readonly (readonly (readonly [number, number])[])[];
  readonly bbox?:
    | {
        readonly west: number;
        readonly south: number;
        readonly east: number;
        readonly north: number;
      }
    | undefined;
}

/** Admin-1 division record item. */
export interface DivisionRecordItem {
  readonly id: string;
  readonly name: string;
  readonly nameEn?: string | null | undefined;
  readonly country: string;
  readonly iso?: string | null | undefined;
  readonly kind?: string | null | undefined;
  readonly polygons: readonly (readonly GeoPoint[])[];
  readonly bbox: BBox;
}

/** Military installation entity. */
export interface MilitaryInstallationItem {
  readonly id: string;
  readonly name?: string | undefined;
  readonly kind: 'airfield' | 'naval_base' | 'range' | 'barracks' | 'base' | 'general';
  readonly position: GeoPoint;
  readonly tags: Readonly<Record<string, string>>;
}

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

/** One wind forecast variant: a model and an optional same-cycle overlay (default none). */
export interface WindRequest {
  readonly model: WindModel;
  readonly overlay?: WeatherOverlay;
}

/** A retained wind grid of a variant; `id` is the manifest's `gridId`. */
export interface WindGridRequest extends WindRequest {
  readonly id: string;
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
  /**
   * The recent track this backend kept for a vessel from the live AIS stream (bounded, oldest
   * first). `notFound` when it has no retained positions; never fetched from upstream.
   */
  vesselTrack(mmsi: string, options?: CallOptions): Promise<VesselTrackResponse>;
  /**
   * The current wind forecast manifest. Upstream trouble is not an error: the manifest says
   * `stale` or `unavailable`. Errors mean the variant cannot be served (disabled, licence).
   */
  windManifest(request: WindRequest, options?: CallOptions): Promise<WindManifest>;
  /** The same manifest without `gridUrl` and `gridId`, for status displays. */
  windStatus(request: WindRequest, options?: CallOptions): Promise<WindManifest>;
  /**
   * The binary grid named by a manifest's `gridId`: little-endian f32 blocks U, V and the
   * optional scalar (layout on `WindGridDef`; read it with `decodeWindGrid`).
   */
  windGrid(request: WindGridRequest, options?: CallOptions): Promise<ArrayBuffer>;
  /** Searches NASA CMR for recent HLS imagery and VIIRS overviews over the area. */
  recentImagery(bbox: BBox, days?: number, options?: CallOptions): Promise<RecentImageryResponse>;
  /** Probes Worldview Snapshots to verify whether pixels exist for a day and area. */
  imageryThumbnailProbe(
    product: string,
    day: string,
    bbox: BBox,
    options?: CallOptions,
  ): Promise<ThumbnailProbeResponse>;
  /** Whether Mapillary credentials are configured on the backend. */
  mapillaryStatus(options?: CallOptions): Promise<{ configured: boolean }>;
  /** Queries administrative and territorial boundary features. */
  boundaries(
    bbox: BBox,
    options?: {
      readonly classes?: readonly BoundaryClass[];
      readonly zoom?: number;
      readonly limit?: number;
      readonly signal?: AbortSignal;
    },
  ): Promise<BoundaryResponse>;
  /** Computer vision detections from the Mojo MLIR perception sidecar for a CCTV camera. */
  cctvDetections(cameraId: string, options?: CallOptions): Promise<CctvDetectionsResponse>;
  /** CCTV cameras in viewport or worldwide. */
  cctvSources(bbox?: BBox, limit?: number, options?: CallOptions): Promise<CameraSource[]>;
  /** Historical transit trail for vehicle playback. */
  transitTrail(feedId: string, vehicleId: string, options?: CallOptions): Promise<TransitTrail>;
  /** Submarine telecommunication cables across ocean basins. */
  submarineCables(
    bbox?: BBox,
    zoom?: number,
    limit?: number,
    options?: CallOptions,
  ): Promise<SubmarineCableApiItem[]>;
  /** Travel directions between coordinates via OSRM with paced queuing and step instructions. */
  calculateRoute(
    profile: 'car' | 'bike' | 'foot',
    coords: string,
    withSteps?: boolean,
    options?: CallOptions,
  ): Promise<Route>;
  /** Admin-1 divisions containing (lon, lat), most specific first. */
  divisionsAt(
    lon: number,
    lat: number,
    options?: CallOptions,
  ): Promise<readonly DivisionRecordItem[]>;
  /** Case-insensitive Admin-1 division search by name, country or ISO code. */
  divisionsSearch(query: string, options?: CallOptions): Promise<readonly DivisionRecordItem[]>;
  /** Military defense, airfield and naval facilities in viewport. */
  militaryInstallations(
    bbox: BBox,
    limit?: number,
    options?: CallOptions,
  ): Promise<{ readonly elements: readonly MilitaryInstallationItem[] }>;
  /** Returns whether upstream credentials are configured without exposing values. */
  secretStatus?(options?: CallOptions): Promise<SecretStatusResponse>;
  /** Desktop only: stores an upstream provider credential into the secure credential store. */
  setSecret?(key: string, value: string, options?: CallOptions): Promise<void>;
  /** Desktop only: removes an upstream provider credential from the secure credential store. */
  clearSecret?(key: string, options?: CallOptions): Promise<void>;
  /** Reports transport changes; calls the listener immediately with the current state. */
  onTransportState(listener: (state: TransportState) => void): () => void;
  readonly preferences: PreferenceStore;
  /**
   * Desktop only: hands a self-check report to the app, which writes it when automated
   * verification asked for one (`LAVENDER_SMOKE_OUT`) and ignores it otherwise.
   */
  readonly diagnostics?: DiagnosticsSink;
}

export interface SecretStatusResponse {
  readonly managedBy: 'operator' | 'keychain' | 'memory';
  readonly status: Readonly<Record<string, boolean>>;
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
