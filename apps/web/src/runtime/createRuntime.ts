/**
 * The composition root's moving parts, outside React: the platform, the stores, the feed
 * controller, catalog/status loading, workspace persistence and the mounted globe. React
 * reaches all of it through `RuntimeProvider` (./context.tsx); tests build one with a fake
 * platform.
 */
import type { ImageryDescriptor } from '../contracts';
import { FeedController } from '../feeds/FeedController';
import type { GlobeController } from '../globe';
import { type LavenderPlatform, toApiError } from '../platform';
import { createStores, type Stores } from '../state/stores';
import {
  readWorkspace,
  readWorkspaceAnnotations,
  toPersisted,
  toPersistedBeforeCatalog,
  WORKSPACE_PREF_KEY,
} from './workspacePrefs';
import { browserWorkspaceLocation, type WorkspaceLocation, WorkspaceShare } from './workspaceShare';

export interface RuntimeOptions {
  /** How often backend status is refreshed for the status bar and Sources panel. */
  readonly statusIntervalMs?: number;
  /** Catalog retry backoff after a failure. */
  readonly catalogRetryMs?: { readonly min: number; readonly max: number };
  /** Delay before a workspace change is written to preferences. */
  readonly persistDelayMs?: number;
  /** Replace browser navigation in tests, or disable it for an embedded host. */
  readonly workspaceLocation?: WorkspaceLocation | null;
}

export interface Runtime {
  readonly platform: LavenderPlatform;
  readonly stores: Stores;
  readonly feeds: FeedController;
  /** Starts loading and streaming; returns the matching stop. Idempotent while running. */
  start(): () => void;
  /** Connects a mounted globe: feeds, selection, imagery and the `ui.globe` handle. */
  attachGlobe(globe: GlobeController): void;
  detachGlobe(): void;
  /** Retries a failed catalog load now (the error state's action). */
  reloadCatalog(): void;
  /** The imagery the globe should show: the workspace choice, else the catalog default. */
  activeImagery(): ImageryDescriptor | null;
  copyViewLink(): Promise<void>;
  restoreViewUrl(url: string): boolean;
}

export function createRuntime(platform: LavenderPlatform, options: RuntimeOptions = {}): Runtime {
  const statusIntervalMs = options.statusIntervalMs ?? 30_000;
  const retry = options.catalogRetryMs ?? { min: 2_000, max: 30_000 };
  const persistDelayMs = options.persistDelayMs ?? 500;
  const stores = createStores();
  const feeds = new FeedController(platform, stores);
  const { workspace, system, ui } = stores;
  const share = new WorkspaceShare(
    stores,
    options.workspaceLocation === undefined
      ? browserWorkspaceLocation()
      : options.workspaceLocation,
  );

  const initialPreferences = platform.preferences.get<unknown>(WORKSPACE_PREF_KEY);
  const initialState = workspace.getState();
  initialState.restore(
    initialState.layers,
    initialState.imageryId,
    initialState.selection,
    readWorkspaceAnnotations(initialPreferences),
  );

  let preferencesHydrated = false;
  let running: (() => void) | null = null;
  let catalogTimer: ReturnType<typeof setTimeout> | null = null;
  let catalogAttempt = 0;
  let catalogRequest: AbortController | null = null;
  let shownImagery: string | null = null;
  let offCameraIntent: (() => void) | null = null;

  const activeImagery = (): ImageryDescriptor | null => {
    const catalog = system.getState().catalog;
    if (!catalog) return null;
    const chosen = workspace.getState().imageryId;
    return (
      catalog.imagery.find((i) => i.id === chosen) ??
      catalog.imagery.find((i) => i.isDefault) ??
      catalog.imagery[0] ??
      null
    );
  };

  const syncImagery = (): void => {
    const globe = ui.getState().globe;
    const imagery = activeImagery();
    if (!globe || !imagery || imagery.id === shownImagery) return;
    shownImagery = imagery.id;
    globe.setImagery(imagery);
  };

  const loadCatalog = (): void => {
    if (catalogTimer) clearTimeout(catalogTimer);
    catalogTimer = null;
    catalogRequest?.abort();
    const request = new AbortController();
    catalogRequest = request;
    platform
      .catalog({ signal: request.signal })
      .then((catalog) => {
        if (request.signal.aborted) return;
        catalogAttempt = 0;
        if (system.getState().catalog === null) {
          // First load: hydrate before publishing the catalog so the controller opens the
          // right feeds once. Later reloads keep what the analyst turned on since.
          const persisted = readWorkspace(initialPreferences, catalog);
          preferencesHydrated = true;
          const state = workspace.getState();
          workspace
            .getState()
            .hydrate(
              state.revisions.layers === 0 ? persisted.layers : state.layers,
              state.revisions.imagery === 0 ? persisted.imageryId : state.imageryId,
            );
        }
        share.apply(catalog);
        system.getState().setCatalog(catalog);
      })
      .catch((error: unknown) => {
        if (request.signal.aborted) return;
        system.getState().setCatalogError(toApiError(error));
        const delay = Math.min(retry.max, retry.min * 2 ** catalogAttempt);
        catalogAttempt += 1;
        catalogTimer = setTimeout(loadCatalog, delay);
      });
  };

  const refreshStatus = (): void => {
    platform
      .status()
      .then((status) => system.getState().setStatus(status))
      .catch(() => {
        // Keep the last status; the transport state already tells the user the backend is away.
      });
  };

  const persistWorkspace = (state: ReturnType<typeof workspace.getState>): void => {
    const error =
      'Device storage is unavailable. These annotations are saved only for this session; copy a view link to keep them.';
    try {
      const persisted =
        preferencesHydrated || system.getState().catalog
          ? toPersisted(state.layers, state.imageryId, state.annotations)
          : toPersistedBeforeCatalog(
              platform.preferences.get<unknown>(WORKSPACE_PREF_KEY),
              state.layers,
              state.imageryId,
              state.revisions.imagery > 0,
              state.annotations,
            );
      platform.preferences.set(WORKSPACE_PREF_KEY, persisted);
      const saved = platform.preferences.get<{ readonly annotations?: unknown }>(
        WORKSPACE_PREF_KEY,
      );
      workspace
        .getState()
        .setAnnotationStorageError(
          JSON.stringify(saved?.annotations) === JSON.stringify(state.annotations) ? null : error,
        );
    } catch {
      workspace.getState().setAnnotationStorageError(error);
    }
  };

  const syncAnnotations = (): void => {
    const state = workspace.getState();
    try {
      ui.getState().globe?.setAnnotations(state.annotations, null);
      state.setAnnotationRenderError(null);
    } catch (error) {
      console.error('Globe could not draw annotations', error);
      state.setAnnotationRenderError(
        'Some annotations could not be drawn. Their saved geometry remains available in this list.',
      );
    }
  };

  const start = (): (() => void) => {
    if (running) return running;
    const cleanup: Array<() => void> = [];
    cleanup.push(share.start());
    feeds.start();
    cleanup.push(() => feeds.stop());
    loadCatalog();
    cleanup.push(() => {
      catalogRequest?.abort();
      if (catalogTimer) clearTimeout(catalogTimer);
    });
    refreshStatus();
    const statusTimer = setInterval(refreshStatus, statusIntervalMs);
    cleanup.push(() => clearInterval(statusTimer));

    let persistTimer: ReturnType<typeof setTimeout> | null = null;
    cleanup.push(
      workspace.subscribe((s, prev) => {
        if (
          s.layers === prev.layers &&
          s.imageryId === prev.imageryId &&
          s.annotations === prev.annotations
        )
          return;
        if (s.annotations !== prev.annotations) {
          // Marks are durable before a reload can cancel the debounce timer.
          if (s.annotations.length > 0 || prev.annotations.length > 0) persistWorkspace(s);
          syncAnnotations();
        }
        if (s.imageryId !== prev.imageryId) syncImagery();
        if (persistTimer) clearTimeout(persistTimer);
        persistTimer = setTimeout(() => {
          persistTimer = null;
          persistWorkspace(s);
        }, persistDelayMs);
      }),
    );
    cleanup.push(() => {
      if (persistTimer) clearTimeout(persistTimer);
    });
    cleanup.push(system.subscribe((s, prev) => s.catalog !== prev.catalog && syncImagery()));

    running = () => {
      for (const c of cleanup.splice(0).reverse()) c();
      running = null;
    };
    return running;
  };

  return {
    platform,
    stores,
    feeds,
    start,
    attachGlobe(globe) {
      offCameraIntent?.();
      offCameraIntent = globe.onCameraIntent(() => share.claimCamera());
      shownImagery = null;
      ui.getState().setGlobe(globe);
      feeds.attachGlobe(globe);
      syncImagery();
      share.apply(system.getState().catalog);
      syncAnnotations();
    },
    detachGlobe() {
      offCameraIntent?.();
      offCameraIntent = null;
      feeds.detachGlobe();
      ui.getState().setGlobe(null);
      shownImagery = null;
    },
    reloadCatalog: loadCatalog,
    activeImagery,
    copyViewLink: () => share.copyLink(),
    restoreViewUrl: (url) => share.restoreUrl(url),
  };
}
