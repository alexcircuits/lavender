/**
 * The desktop transport (ADR 0005, docs/architecture/tauri.md §4): Tauri commands call the
 * `LavenderCore` embedded in the app; live feeds arrive over one `Channel` per subscription
 * carrying the same `ServerFrame` values the web stream sends. Only `src/platform` may
 * import `@tauri-apps/*` (src/architecture.test.ts).
 */
import type {
  AircraftTrack,
  BBox,
  BoundaryClass,
  BoundaryResponse,
  CameraSource,
  Catalog,
  CctvDetectionsResponse,
  FeedQuery,
  FeedSnapshot,
  Route,
  SatellitePasses,
  SatellitePassRequest,
  SatelliteTrack,
  SatelliteTrackRequest,
  SearchQuery,
  SearchResults,
  ServerFrame,
  SystemStatus,
  TransitTrail,
  VesselTrackResponse,
  WindManifest,
} from '../../contracts';
import type { SubmarineCableItem } from '../../globe';
import { LocalPreferences } from '../preferences';
import type { RecentImageryResponse, ThumbnailProbeResponse } from '../recentImageryTypes';
import {
  type CallOptions,
  type DiagnosticsSink,
  type DivisionRecordItem,
  type FeedEvent,
  type FeedSubscription,
  type LavenderPlatform,
  type MilitaryInstallationItem,
  PlatformError,
  type PreferenceStore,
  type SecretStatusResponse,
  type TransportState,
  toApiError,
  type WindGridRequest,
  type WindRequest,
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

  aircraftTrack(hex: string, options?: CallOptions): Promise<AircraftTrack> {
    return this.#call('aircraft_track', { hex }, options);
  }

  /** `commands::vessel_track` in apps/desktop/src-tauri. */
  vesselTrack(mmsi: string, options?: CallOptions): Promise<VesselTrackResponse> {
    return this.#call('vessel_track', { mmsi }, options);
  }

  satellitePasses(request: SatellitePassRequest, options?: CallOptions): Promise<SatellitePasses> {
    return this.#call('satellite_passes', { request }, options);
  }

  satelliteTrack(request: SatelliteTrackRequest, options?: CallOptions): Promise<SatelliteTrack> {
    return this.#call('satellite_track', { request }, options);
  }

  windManifest(request: WindRequest, options?: CallOptions): Promise<WindManifest> {
    return this.#call('wind_manifest', windArgs(request), options);
  }

  windStatus(request: WindRequest, options?: CallOptions): Promise<WindManifest> {
    return this.#call('wind_status', windArgs(request), options);
  }

  /** The command answers raw bytes (`tauri::ipc::Response`), which arrive as an ArrayBuffer. */
  windGrid(request: WindGridRequest, options?: CallOptions): Promise<ArrayBuffer> {
    return this.#call('wind_grid', { ...windArgs(request), id: request.id }, options);
  }

  recentImagery(bbox: BBox, days?: number, options?: CallOptions): Promise<RecentImageryResponse> {
    return this.#call('recent_imagery', { bbox, days }, options);
  }

  imageryThumbnailProbe(
    product: string,
    day: string,
    bbox: BBox,
    options?: CallOptions,
  ): Promise<ThumbnailProbeResponse> {
    return this.#call('imagery_thumbnail_probe', { product, day, bbox }, options);
  }

  mapillaryStatus(options?: CallOptions): Promise<{ configured: boolean }> {
    return this.#call('mapillary_status', {}, options);
  }

  boundaries(
    bbox: BBox,
    options?: {
      readonly classes?: readonly BoundaryClass[];
      readonly zoom?: number;
      readonly limit?: number;
      readonly signal?: AbortSignal;
    },
  ): Promise<BoundaryResponse> {
    return this.#call('boundaries', { bbox, ...options }, options);
  }

  cctvDetections(cameraId: string, options?: CallOptions): Promise<CctvDetectionsResponse> {
    return this.#call('cctv_detections', { cameraId }, options);
  }

  cctvSources(bbox?: BBox, limit?: number, options?: CallOptions): Promise<CameraSource[]> {
    return this.#call('cctv_sources', { bbox, limit }, options);
  }

  transitTrail(feedId: string, vehicleId: string, options?: CallOptions): Promise<TransitTrail> {
    return this.#call('transit_trail', { feedId, vehicleId }, options);
  }

  submarineCables(
    bbox?: BBox,
    zoom?: number,
    limit?: number,
    options?: CallOptions,
  ): Promise<SubmarineCableItem[]> {
    return this.#call('cables', { bbox, zoom, limit }, options);
  }

  calculateRoute(
    profile: 'car' | 'bike' | 'foot',
    coords: string,
    withSteps = true,
    options?: CallOptions,
  ): Promise<Route> {
    return this.#call('calculate_route', { profile, coords, steps: withSteps }, options);
  }

  divisionsAt(
    lon: number,
    lat: number,
    options?: CallOptions,
  ): Promise<readonly DivisionRecordItem[]> {
    return this.#call('divisions_at', { lon, lat }, options);
  }

  divisionsSearch(query: string, options?: CallOptions): Promise<readonly DivisionRecordItem[]> {
    return this.#call('divisions_search', { query }, options);
  }

  militaryInstallations(
    bbox: BBox,
    limit?: number,
    options?: CallOptions,
  ): Promise<{ readonly elements: readonly MilitaryInstallationItem[] }> {
    return this.#call('military_installations', { bbox, limit }, options);
  }

  async secretStatus(options?: CallOptions): Promise<SecretStatusResponse> {
    const status = await this.#call<Record<string, boolean>>('secret_status', {}, options);
    return { managedBy: 'keychain', status };
  }

  setSecret(key: string, value: string, options?: CallOptions): Promise<void> {
    return this.#call('secret_set', { key, value }, options);
  }

  clearSecret(key: string, options?: CallOptions): Promise<void> {
    return this.#call('secret_clear', { key }, options);
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

/** Arguments of the `wind_*` commands; the overlay is always explicit. */
function windArgs(request: WindRequest): Record<string, unknown> {
  return { model: request.model, overlay: request.overlay ?? 'none' };
}
