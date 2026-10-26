/**
 * Bridges what the analyst turned on (workspace store) to live subscriptions (platform) and
 * to what the globe draws (GlobeController), and summarises each feed for the interface
 * (feeds store). Entity data flows platform → globe without passing through React
 * (docs/architecture/frontend.md §5).
 */
import type { FeedFilters, FeedId, FeedQuery, FeedSnapshot, LayerDescriptor } from '../contracts';
import type { EntityRef, GlobeController, ViewState } from '../globe';
import type { FeedEvent, FeedSubscription, LavenderPlatform } from '../platform';
import { EMPTY_SUMMARY, type FeedSummary, type Stores } from '../state/stores';
import { findItem, usesViewport } from './entities';

const VIEW_DEBOUNCE_MS = 300;

interface Active {
  readonly subscription: FeedSubscription;
  filtersKey: string;
}

export function summarise(snapshot: FeedSnapshot): FeedSummary {
  return {
    phase: 'ready',
    count: snapshot.items.items.length,
    sourceCount: snapshot.sourceCount,
    freshness: snapshot.freshness,
    coverage: snapshot.coverage,
    fetchedAt: snapshot.fetchedAt ?? null,
    pendingRegions: snapshot.pendingRegions,
    notices: snapshot.notices,
    sources: snapshot.sources,
    error: null,
  };
}

export class FeedController {
  readonly #platform: LavenderPlatform;
  readonly #stores: Stores;
  readonly #active = new Map<FeedId, Active>();
  readonly #latest = new Map<FeedId, FeedSnapshot>();
  readonly #cleanup: Array<() => void> = [];
  readonly #globeCleanup: Array<() => void> = [];
  #globe: GlobeController | null = null;
  #view: ViewState | null = null;
  #viewTimer: ReturnType<typeof setTimeout> | null = null;
  #trackRequest: AbortController | null = null;
  /** `feed:id` of the entity whose record the feeds store currently holds. */
  #heldKey: string | null = null;

  constructor(platform: LavenderPlatform, stores: Stores) {
    this.#platform = platform;
    this.#stores = stores;
  }

  start(): void {
    const { workspace, system, feeds } = this.#stores;
    this.#cleanup.push(this.#platform.onTransportState((s) => feeds.getState().setTransport(s)));
    this.#cleanup.push(
      workspace.subscribe((s, prev) => {
        if (s.layers !== prev.layers) this.#reconcile();
        if (s.selection !== prev.selection) this.#onSelection(s.selection);
      }),
    );
    this.#cleanup.push(
      system.subscribe((s, prev) => s.catalog !== prev.catalog && this.#reconcile()),
    );
    this.#reconcile();
  }

  stop(): void {
    this.detachGlobe();
    for (const c of this.#cleanup.splice(0)) c();
    for (const feed of [...this.#active.keys()]) this.#close(feed);
    if (this.#viewTimer) clearTimeout(this.#viewTimer);
    this.#trackRequest?.abort();
  }

  /** Connects the globe once it has mounted, replaying what is already known. */
  attachGlobe(globe: GlobeController): void {
    this.detachGlobe();
    this.#globe = globe;
    for (const snapshot of this.#latest.values()) globe.applySnapshot(snapshot);
    globe.setSelection(this.#stores.workspace.getState().selection);
    this.#globeCleanup.push(
      globe.onPick((entity) => this.#stores.workspace.getState().select(entity)),
    );
    this.#globeCleanup.push(globe.onViewChange((view) => this.#onView(view)));
  }

  detachGlobe(): void {
    for (const c of this.#globeCleanup.splice(0)) c();
    this.#globe = null;
  }

  /** The latest snapshot for a feed (for lists and search-to-select). */
  latest(feed: FeedId): FeedSnapshot | undefined {
    return this.#latest.get(feed);
  }

  #layers(): readonly LayerDescriptor[] {
    return this.#stores.system.getState().catalog?.layers ?? [];
  }

  #query(feed: FeedId, filters: FeedFilters): FeedQuery {
    const bbox = usesViewport(feed) ? this.#view?.bbox : null;
    return bbox ? { feed, bbox, filters } : { feed, filters };
  }

  #reconcile(): void {
    const layers = this.#stores.workspace.getState().layers;
    for (const descriptor of this.#layers()) {
      const feed = descriptor.feed;
      const state = layers[feed];
      const wanted = Boolean(state?.enabled) && descriptor.availability.state === 'available';
      const active = this.#active.get(feed);
      const filters = state?.filters ?? {};
      if (wanted && !active) {
        this.#open(feed, filters, descriptor.cadenceS);
      } else if (wanted && active && active.filtersKey !== JSON.stringify(filters)) {
        active.filtersKey = JSON.stringify(filters);
        active.subscription.update(this.#query(feed, filters));
      } else if (!wanted && active) {
        this.#close(feed);
      }
    }
  }

  #open(feed: FeedId, filters: FeedFilters, cadenceS: number): void {
    this.#stores.feeds.getState().setSummary(feed, EMPTY_SUMMARY);
    const subscription = this.#platform.subscribe(
      this.#query(feed, filters),
      (event) => this.#onEvent(feed, event),
      {
        pollIntervalS: Math.max(cadenceS, 10),
      },
    );
    this.#active.set(feed, { subscription, filtersKey: JSON.stringify(filters) });
  }

  #close(feed: FeedId): void {
    this.#active.get(feed)?.subscription.close();
    this.#active.delete(feed);
    this.#latest.delete(feed);
    this.#globe?.clearFeed(feed);
    this.#stores.feeds.getState().clearSummary(feed);
  }

  #onEvent(feed: FeedId, event: FeedEvent): void {
    if (!this.#active.has(feed)) return;
    const feeds = this.#stores.feeds.getState();
    if (event.kind === 'error') {
      const previous = feeds.summaries[feed] ?? EMPTY_SUMMARY;
      feeds.setSummary(feed, { ...previous, phase: 'error', error: event.error });
      return;
    }
    this.#latest.set(feed, event.snapshot);
    this.#globe?.applySnapshot(event.snapshot);
    feeds.setSummary(feed, summarise(event.snapshot));
    if (this.#stores.workspace.getState().selection?.feed === feed) this.#refreshSelected();
  }

  #onView(view: ViewState): void {
    this.#view = view;
    if (this.#viewTimer) clearTimeout(this.#viewTimer);
    this.#viewTimer = setTimeout(() => {
      this.#viewTimer = null;
      const layers = this.#stores.workspace.getState().layers;
      for (const [feed, active] of this.#active) {
        if (usesViewport(feed))
          active.subscription.update(this.#query(feed, layers[feed]?.filters ?? {}));
      }
    }, VIEW_DEBOUNCE_MS);
  }

  #onSelection(selection: EntityRef | null): void {
    this.#globe?.setSelection(selection);
    this.#refreshSelected();
    this.#trackRequest?.abort();
    this.#trackRequest = null;
    const noradId = selection?.feed === 'satellites' ? Number(selection.id) : Number.NaN;
    if (!Number.isInteger(noradId) || noradId <= 0) {
      this.#globe?.showTrack(null);
      return;
    }
    const request = new AbortController();
    this.#trackRequest = request;
    this.#platform
      .satelliteTrack({ noradId, stepS: 60 }, { signal: request.signal })
      .then((track) => {
        if (!request.signal.aborted) this.#globe?.showTrack(track);
      })
      .catch(() => {
        // The orbit is a convenience; the inspector still shows the satellite.
      });
  }

  #refreshSelected(): void {
    const feeds = this.#stores.feeds.getState();
    const selection = this.#stores.workspace.getState().selection;
    if (
      !selection ||
      selection.feed === 'boundaries' ||
      selection.feed === 'cables' ||
      selection.feed === 'cctv' ||
      selection.feed === 'installations' ||
      selection.feed === 'directions-step' ||
      selection.feed === 'directions-waypoint' ||
      selection.feed === 'air-quality' ||
      selection.feed === 'gnss'
    ) {
      this.#heldKey = null;
      feeds.setSelectedEntity(null, null, false);
      return;
    }
    const key = `${selection.feed}:${selection.id}`;
    const snapshot = this.#latest.get(selection.feed);
    const item = snapshot ? findItem(snapshot.items, selection.id) : undefined;
    if (item) {
      this.#heldKey = key;
      feeds.setSelectedEntity(item, snapshot?.generatedAt ?? null, true);
    } else if (this.#heldKey === key) {
      // Keep the last known record so the inspector can say when it was last seen.
      feeds.setSelectedEntity(feeds.selectedEntity, feeds.selectedLastSeen, false);
    } else {
      this.#heldKey = null;
      feeds.setSelectedEntity(null, null, false);
    }
  }
}
