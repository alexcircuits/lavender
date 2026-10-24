/**
 * Application state (docs/architecture/frontend.md §3). Three small stores with narrow
 * selectors; entity *data* never lives here (it lives in the globe renderers), only summaries,
 * selection and what the user turned on.
 */
import { createStore } from 'zustand/vanilla';
import type {
  ApiError,
  Catalog,
  Coverage,
  FeedFilters,
  FeedId,
  Freshness,
  SystemStatus,
} from '../contracts';
import { type MapAnnotation, validateAnnotations } from '../features/annotations/model';
import {
  type BoundaryClassVisibility,
  DEFAULT_BOUNDARY_VISIBILITY,
  type EntityRef,
  type GlobeController,
} from '../globe';
import type { TransportState } from '../platform';

// ── workspace: what the analyst turned on and selected (persisted per device) ─────────────

export interface LayerState {
  readonly enabled: boolean;
  readonly filters: FeedFilters;
}

export interface WorkspaceState {
  readonly layers: Partial<Record<FeedId, LayerState>>;
  readonly selection: EntityRef | null;
  readonly imageryId: string | null;
  readonly annotations: readonly MapAnnotation[];
  readonly annotationRenderError: string | null;
  setAnnotationRenderError(error: string | null): void;
  readonly annotationStorageError: string | null;
  setAnnotationStorageError(error: string | null): void;
  setAnnotations(annotations: readonly MapAnnotation[]): void;
  claimAnnotations(): void;
  /** User intent counters; hydration/restoration never claims a lane. Even no-op actions do. */
  readonly revisions: {
    readonly layers: number;
    readonly imagery: number;
    readonly selection: number;
    readonly annotations: number;
  };
  setLayerEnabled(feed: FeedId, enabled: boolean): void;
  setFilters(feed: FeedId, filters: FeedFilters): void;
  select(selection: EntityRef | null): void;
  setImagery(id: string): void;
  readonly boundariesEnabled: boolean;
  readonly boundaryClasses: BoundaryClassVisibility;
  setBoundariesEnabled(enabled: boolean): void;
  setBoundaryClasses(classes: Partial<BoundaryClassVisibility>): void;
  /** Applies persisted or default layer choices without touching selection. */
  hydrate(layers: Partial<Record<FeedId, LayerState>>, imageryId: string | null): void;
  restore(
    layers: Partial<Record<FeedId, LayerState>>,
    imageryId: string | null,
    selection: EntityRef | null,
    annotations?: readonly MapAnnotation[],
  ): void;
}

export const createWorkspaceStore = () =>
  createStore<WorkspaceState>()((set) => ({
    layers: {},
    selection: null,
    imageryId: null,
    annotations: [],
    annotationRenderError: null,
    setAnnotationRenderError: (annotationRenderError) => set({ annotationRenderError }),
    annotationStorageError: null,
    setAnnotationStorageError: (annotationStorageError) => set({ annotationStorageError }),
    revisions: { layers: 0, imagery: 0, selection: 0, annotations: 0 },
    setAnnotations: (annotations) =>
      set((s) => ({
        annotations: validateAnnotations(annotations),
        revisions: { ...s.revisions, annotations: s.revisions.annotations + 1 },
      })),
    claimAnnotations: () =>
      set((s) => ({ revisions: { ...s.revisions, annotations: s.revisions.annotations + 1 } })),
    setLayerEnabled: (feed, enabled) =>
      set((s) => ({
        layers: { ...s.layers, [feed]: { filters: s.layers[feed]?.filters ?? {}, enabled } },
        selection: !enabled && s.selection?.feed === feed ? null : s.selection,
        revisions: {
          ...s.revisions,
          layers: s.revisions.layers + 1,
          selection: s.revisions.selection + (!enabled && s.selection?.feed === feed ? 1 : 0),
        },
      })),
    setFilters: (feed, filters) =>
      set((s) => ({
        layers: { ...s.layers, [feed]: { enabled: s.layers[feed]?.enabled ?? false, filters } },
        revisions: { ...s.revisions, layers: s.revisions.layers + 1 },
      })),
    select: (selection) =>
      set((s) => ({
        selection,
        revisions: { ...s.revisions, selection: s.revisions.selection + 1 },
      })),
    setImagery: (imageryId) =>
      set((s) => ({ imageryId, revisions: { ...s.revisions, imagery: s.revisions.imagery + 1 } })),
    boundariesEnabled: true,
    boundaryClasses: { ...DEFAULT_BOUNDARY_VISIBILITY },
    setBoundariesEnabled: (boundariesEnabled) =>
      set((s) => ({
        boundariesEnabled,
        selection: !boundariesEnabled && s.selection?.feed === 'boundaries' ? null : s.selection,
        revisions: {
          ...s.revisions,
          layers: s.revisions.layers + 1,
          selection:
            s.revisions.selection +
            (!boundariesEnabled && s.selection?.feed === 'boundaries' ? 1 : 0),
        },
      })),
    setBoundaryClasses: (visibility) =>
      set((s) => ({
        boundaryClasses: { ...s.boundaryClasses, ...visibility },
        revisions: { ...s.revisions, layers: s.revisions.layers + 1 },
      })),
    hydrate: (layers, imageryId) => set({ layers, imageryId }),
    restore: (layers, imageryId, selection, annotations) =>
      set((s) => ({ layers, imageryId, selection, annotations: annotations ?? s.annotations })),
  }));

// ── feeds: per-layer summaries the interface renders (count, freshness, errors) ───────────

export type FeedPhase = 'connecting' | 'ready' | 'error';

export interface FeedSummary {
  readonly phase: FeedPhase;
  /** Items shown after filtering. */
  readonly count: number;
  /** Items the source reported before filtering. */
  readonly sourceCount: number;
  readonly freshness: Freshness | null;
  readonly coverage: Coverage | null;
  readonly fetchedAt: string | null;
  readonly pendingRegions: number;
  readonly notices: readonly string[];
  readonly sources: readonly string[];
  readonly error: ApiError | null;
}

export const EMPTY_SUMMARY: FeedSummary = {
  phase: 'connecting',
  count: 0,
  sourceCount: 0,
  freshness: null,
  coverage: null,
  fetchedAt: null,
  pendingRegions: 0,
  notices: [],
  sources: [],
  error: null,
};

export interface FeedsState {
  readonly summaries: Partial<Record<FeedId, FeedSummary>>;
  readonly transport: TransportState;
  /** The selected entity's latest record (an Aircraft, Earthquake, …), or null when the
   *  selection has left the feed. The inspector renders this. */
  readonly selectedEntity: unknown;
  /** When the selected entity was last seen in its feed (ISO time), for "no longer in feed". */
  readonly selectedLastSeen: string | null;
  /** Whether the selected entity is in its feed's latest snapshot. */
  readonly selectedPresent: boolean;
  setSummary(feed: FeedId, summary: FeedSummary): void;
  clearSummary(feed: FeedId): void;
  setTransport(state: TransportState): void;
  setSelectedEntity(entity: unknown, lastSeen: string | null, present: boolean): void;
}

export const createFeedsStore = () =>
  createStore<FeedsState>()((set) => ({
    summaries: {},
    transport: 'connecting',
    selectedEntity: null,
    selectedLastSeen: null,
    selectedPresent: false,
    setSummary: (feed, summary) => set((s) => ({ summaries: { ...s.summaries, [feed]: summary } })),
    clearSummary: (feed) =>
      set((s) => {
        const { [feed]: _removed, ...rest } = s.summaries;
        return { summaries: rest };
      }),
    setTransport: (transport) => set({ transport }),
    setSelectedEntity: (selectedEntity, selectedLastSeen, selectedPresent) =>
      set({ selectedEntity, selectedLastSeen, selectedPresent }),
  }));

// ── system: catalog and backend status ───────────────────────────────────────────────────

export interface SystemState {
  readonly catalog: Catalog | null;
  readonly status: SystemStatus | null;
  readonly catalogError: ApiError | null;
  setCatalog(catalog: Catalog): void;
  setStatus(status: SystemStatus): void;
  setCatalogError(error: ApiError): void;
}

export const createSystemStore = () =>
  createStore<SystemState>()((set) => ({
    catalog: null,
    status: null,
    catalogError: null,
    setCatalog: (catalog) => set({ catalog, catalogError: null }),
    setStatus: (status) => set({ status }),
    setCatalogError: (catalogError) => set({ catalogError }),
  }));

// ── ui: panels, palette and the mounted globe (never persisted) ───────────────────────────

import {
  type CoordinateFormat,
  createPreferencesStore,
  DEFAULT_USER_PREFERENCES,
  type MotionPreference,
  type PreferencesState,
  type PreferencesStore,
  type UnitSystem,
  type UserPreferences,
} from './preferencesStore';

export type {
  CoordinateFormat,
  MotionPreference,
  PreferencesState,
  PreferencesStore,
  UnitSystem,
  UserPreferences,
};
export { DEFAULT_USER_PREFERENCES };

export type SidePanel = 'layers' | 'sources' | 'annotations' | 'launches' | 'settings';

export interface UiState {
  /** The mounted globe's command interface, or null while it loads or after it failed. */
  readonly globe: GlobeController | null;
  /** Which left island is open (the navigation rail switches it); null hides it. */
  readonly sidePanel: SidePanel | null;
  readonly paletteOpen: boolean;
  /** The keyboard shortcut sheet (`?`). */
  readonly shortcutsOpen: boolean;
  setGlobe(globe: GlobeController | null): void;
  setSidePanel(panel: SidePanel | null): void;
  setPaletteOpen(open: boolean): void;
  setShortcutsOpen(open: boolean): void;
}

export const createUiStore = () =>
  createStore<UiState>()((set) => ({
    globe: null,
    sidePanel: 'layers',
    paletteOpen: false,
    shortcutsOpen: false,
    setGlobe: (globe) => set({ globe }),
    setSidePanel: (sidePanel) => set({ sidePanel }),
    setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
    setShortcutsOpen: (shortcutsOpen) => set({ shortcutsOpen }),
  }));

export type WorkspaceStore = ReturnType<typeof createWorkspaceStore>;
export type FeedsStore = ReturnType<typeof createFeedsStore>;
export type SystemStore = ReturnType<typeof createSystemStore>;
export type UiStore = ReturnType<typeof createUiStore>;

export interface Stores {
  readonly workspace: WorkspaceStore;
  readonly feeds: FeedsStore;
  readonly system: SystemStore;
  readonly ui: UiStore;
  readonly preferences: PreferencesStore;
}

export function createStores(): Stores {
  return {
    workspace: createWorkspaceStore(),
    feeds: createFeedsStore(),
    system: createSystemStore(),
    ui: createUiStore(),
    preferences: createPreferencesStore(),
  };
}
