/**
 * The persisted part of the workspace (docs/architecture/frontend.md §3). Preferences are
 * device-local but still untrusted input: a stale or hand-edited value must never put the
 * app into a state the catalog cannot back, so everything read is validated against it.
 */
import type { Catalog, FeedFilters, FeedId } from '../contracts';
import { type MapAnnotation, validateAnnotations } from '../features/annotations/model';
import type { LayerState } from '../state/stores';

export const WORKSPACE_PREF_KEY = 'workspace';
const VERSION = 1;

export interface PersistedWorkspace {
  readonly version: typeof VERSION;
  readonly layers: Partial<Record<FeedId, LayerState>>;
  readonly imageryId: string | null;
  readonly annotations: readonly MapAnnotation[];
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

function readFilters(value: unknown): FeedFilters {
  if (!isRecord(value)) return {};
  const filters: { -readonly [K in keyof FeedFilters]: FeedFilters[K] } = {};
  const { minMagnitude, militaryOnly, groups } = value;
  if (
    typeof minMagnitude === 'number' &&
    Number.isFinite(minMagnitude) &&
    minMagnitude >= -2 &&
    minMagnitude <= 10
  ) {
    filters.minMagnitude = minMagnitude;
  }
  if (typeof militaryOnly === 'boolean') filters.militaryOnly = militaryOnly;
  if (Array.isArray(groups)) {
    const valid = groups.filter(
      (g): g is string => typeof g === 'string' && /^[a-z0-9-]{1,32}$/.test(g),
    );
    if (valid.length > 0) filters.groups = valid.slice(0, 32);
  }
  return filters;
}

/** The layer choices a fresh workspace starts with. */
export function defaultWorkspace(catalog: Catalog): PersistedWorkspace {
  const layers: Partial<Record<FeedId, LayerState>> = {};
  for (const layer of catalog.layers) {
    if (layer.defaultOn) layers[layer.feed] = { enabled: true, filters: {} };
  }
  return { version: VERSION, layers, imageryId: null, annotations: [] };
}

/** Reads a persisted workspace, keeping only what the current catalog can back. */
export function readWorkspace(value: unknown, catalog: Catalog): PersistedWorkspace {
  if (!isRecord(value) || value.version !== VERSION || !isRecord(value.layers))
    return defaultWorkspace(catalog);
  const known = new Set<string>(catalog.layers.map((l) => l.feed));
  const layers: Partial<Record<FeedId, LayerState>> = {};
  for (const [feed, state] of Object.entries(value.layers)) {
    if (!known.has(feed) || !isRecord(state) || typeof state.enabled !== 'boolean') continue;
    layers[feed as FeedId] = { enabled: state.enabled, filters: readFilters(state.filters) };
  }
  const imageryId =
    typeof value.imageryId === 'string' && catalog.imagery.some((i) => i.id === value.imageryId)
      ? value.imageryId
      : null;
  let annotations: readonly MapAnnotation[] = [];
  try {
    annotations = validateAnnotations(value.annotations ?? []);
  } catch {
    /* Corrupt marks never reach the map. */
  }
  return { version: VERSION, layers, imageryId, annotations };
}

export function toPersisted(
  layers: Partial<Record<FeedId, LayerState>>,
  imageryId: string | null,
  annotations: readonly MapAnnotation[] = [],
): PersistedWorkspace {
  return { version: VERSION, layers, imageryId, annotations };
}
