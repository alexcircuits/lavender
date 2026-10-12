/**
 * The search model behind the top search field and the command palette: instant local results
 * (coordinates, live entities from the feeds already on, layers, commands) plus the server's
 * places, entities and coordinate forms, debounced and cancellable (design-system §8).
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import type { EntityHit, FeedId, GeoPoint, SearchResults } from '../../contracts';
import { parseCoordinates } from '../../lib/coordinates';
import type { Runtime } from '../../runtime';
import {
  useFeeds,
  useGlobe,
  usePlatform,
  useRuntime,
  useSystem,
  useWorkspace,
} from '../../runtime';
import { type CommandContext, runCommand } from '../commands';
import { searchLiveEntities } from './entitySearch';
import {
  actionGroup,
  coordinateGroup,
  entityGroup,
  layerGroup,
  placeGroup,
  type ResultActions,
  type ResultGroup,
} from './results';

export type { ResultGroup, ResultItem } from './results';

export const SEARCH_DEBOUNCE_MS = 200;
/** Shorter queries stay local: one letter matches half the world's place names. */
export const MIN_REMOTE_QUERY = 2;
const REMOTE_LIMIT = 8;

export type RemoteStatus = 'idle' | 'loading' | 'ready' | 'failed';

export interface SearchModel {
  readonly groups: readonly ResultGroup[];
  readonly status: RemoteStatus;
  readonly notices: readonly string[];
  /** The query produced nothing anywhere (and the server has answered). */
  readonly empty: boolean;
}

interface RemoteState {
  readonly query: string;
  readonly status: 'ready' | 'failed';
  readonly results: SearchResults | null;
}

/** Server search for `query`, debounced, with the previous request aborted on every change. */
function useRemoteSearch(query: string): { status: RemoteStatus; results: SearchResults | null } {
  const platform = usePlatform();
  const globe = useGlobe();
  const centre = useRef<GeoPoint | null>(null);
  const [state, setState] = useState<RemoteState | null>(null);
  const q = query.trim();

  // Bias places towards what the camera shows, without re-rendering on every move.
  useEffect(
    () =>
      globe?.onViewChange((view) => {
        centre.current = view.centre;
      }),
    [globe],
  );

  useEffect(() => {
    if (q.length < MIN_REMOTE_QUERY) return;
    const request = new AbortController();
    const timer = setTimeout(() => {
      const near = centre.current;
      platform
        .search({ q, limit: REMOTE_LIMIT, ...(near ? { near } : {}) }, { signal: request.signal })
        .then((results) => {
          if (!request.signal.aborted) setState({ query: q, status: 'ready', results });
        })
        .catch(() => {
          if (!request.signal.aborted) setState({ query: q, status: 'failed', results: null });
        });
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      clearTimeout(timer);
      request.abort();
    };
  }, [platform, q]);

  if (q.length < MIN_REMOTE_QUERY) return { status: 'idle', results: null };
  if (state?.query !== q) {
    // Keep showing the previous answer while the next one is on its way.
    return { status: 'loading', results: state?.status === 'ready' ? state.results : null };
  }
  return { status: state.status, results: state.results };
}

/** Selects an entity, turning its layer on first if needed, and flies to it when it has a place. */
function selectEntity(runtime: Runtime, hit: EntityHit): void {
  const workspace = runtime.stores.workspace.getState();
  const layer = runtime.stores.system.getState().catalog?.layers.find((l) => l.feed === hit.feed);
  if (!workspace.layers[hit.feed]?.enabled && layer?.availability.state === 'available') {
    workspace.setLayerEnabled(hit.feed, true);
  }
  workspace.select({ feed: hit.feed, id: hit.id });
  if (hit.position) {
    runtime.stores.ui.getState().globe?.flyTo({ kind: 'point', position: hit.position });
  }
}

function resultActions(
  runtime: Runtime,
  commandContext: CommandContext,
  onDone: () => void,
): ResultActions {
  const globe = () => runtime.stores.ui.getState().globe;
  // Finish first (close the palette, clear the field), then act, so focus settles before
  // anything the action opens (the inspector, the shortcut sheet) records where it came from.
  const finish =
    <A extends unknown[]>(action: (...args: A) => void) =>
    (...args: A) => {
      onDone();
      action(...args);
    };
  return {
    flyToPoint: finish((position) => globe()?.flyTo({ kind: 'point', position })),
    flyToBox: finish((bbox) => globe()?.flyTo({ kind: 'bbox', bbox })),
    selectEntity: finish((hit) => selectEntity(runtime, hit)),
    setLayerEnabled: finish((feed, on) =>
      runtime.stores.workspace.getState().setLayerEnabled(feed, on),
    ),
    run: finish((command) => void runCommand(command, commandContext)),
    isEnabled: (command) => !command.isEnabled || command.isEnabled(commandContext),
  };
}

export interface SearchModelOptions {
  /** Include layers and actions even for an empty query (the palette). */
  readonly commands: boolean;
  /** Called after an item runs: the palette closes, the field clears. */
  readonly onDone: () => void;
  readonly commandContext: CommandContext;
}

export function useSearchModel(query: string, options: SearchModelOptions): SearchModel {
  const { commands, onDone, commandContext } = options;
  const runtime = useRuntime();
  const catalog = useSystem((s) => s.catalog);
  const layerStates = useWorkspace((s) => s.layers);
  // Feed summaries change with every snapshot: matches follow the data as it arrives.
  const summaries = useFeeds((s) => s.summaries);
  const hasSelection = useWorkspace((s) => s.selection !== null);
  const hasGlobe = useGlobe() !== null;
  const remote = useRemoteSearch(query);
  const q = query.trim();

  const parsed = useMemo(() => parseCoordinates(q), [q]);
  const liveEntities = useMemo(
    () => searchLiveEntities(runtime.feeds, q, 8, Object.keys(summaries) as FeedId[]),
    [runtime, q, summaries],
  );
  const act = useMemo(
    () => resultActions(runtime, commandContext, onDone),
    [runtime, commandContext, onDone],
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: commands read the selection and the globe from the stores when asked if they are enabled; listing them keeps those states current.
  return useMemo(() => {
    const results = remote.results;
    const listCommands = commands || q !== '';
    const groups = [
      coordinateGroup(parsed, results?.coordinates ?? [], act),
      placeGroup(results?.places ?? [], catalog, act),
      entityGroup(liveEntities, results?.entities ?? [], act),
      listCommands ? layerGroup(catalog, layerStates, q, act) : null,
      listCommands ? actionGroup(q, act) : null,
    ].filter((g): g is ResultGroup => g !== null);
    const total = groups.reduce((sum, g) => sum + g.items.length, 0);
    return {
      groups,
      status: remote.status,
      notices: results?.notices ?? [],
      empty: q !== '' && total === 0 && remote.status !== 'loading',
    };
  }, [
    act,
    catalog,
    commands,
    hasGlobe,
    hasSelection,
    layerStates,
    liveEntities,
    parsed,
    q,
    remote.results,
    remote.status,
  ]);
}
