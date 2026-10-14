/**
 * Result groups for search and the palette, as pure functions of what was found plus the
 * actions a result can take. The hook in ./useSearch wires them to the runtime.
 */
import type {
  BBox,
  Catalog,
  CoordinateHit,
  EntityHit,
  FeedId,
  GeoPoint,
  PlaceHit,
} from '../../contracts';
import {
  AirplaneIcon,
  CrosshairIcon,
  type Icon,
  MapTrifoldIcon,
  PlanetIcon,
  StackIcon,
  WaveformIcon,
} from '../../design';
import { formatDecimal, type ParsedCoordinate } from '../../lib/coordinates';
import { lookup } from '../../lib/lookup';
import { spaceMgrs, toMgrs } from '../../lib/mgrs';
import type { LayerState } from '../../state/stores';
import { COMMANDS, type Command, matchesQuery, shortcutLabel } from '../commands';

export interface ResultItem {
  /** Unique within the list; also cmdk's value. */
  readonly id: string;
  readonly title: string;
  readonly detail?: string;
  /** What Enter does: "Fly to", "Select", "Turn on". */
  readonly hint: string;
  readonly icon: Icon;
  readonly shortcut?: string;
  readonly disabled?: boolean;
  readonly run: () => void;
}

export interface ResultGroup {
  readonly id: 'coordinates' | 'places' | 'entities' | 'layers' | 'actions';
  readonly heading: string;
  readonly items: readonly ResultItem[];
  /** Attribution or context under the group (places name their geocoder). */
  readonly footer?: string;
}

/** What results do when chosen. Each call also finishes the search (closes, clears). */
export interface ResultActions {
  flyToPoint(position: GeoPoint): void;
  flyToBox(bbox: BBox): void;
  selectEntity(hit: EntityHit): void;
  setLayerEnabled(feed: FeedId, on: boolean): void;
  run(command: Command): void;
  isEnabled(command: Command): boolean;
}

const withDetail = (detail: string | null | undefined) => (detail ? { detail } : {});

function coordinateDetail(parsed: ParsedCoordinate): string | null {
  if (parsed.format === 'mgrs') return 'Read as MGRS';
  const mgrs = toMgrs(parsed.position.lat, parsed.position.lon, 5);
  return mgrs ? `MGRS ${spaceMgrs(mgrs)}` : null;
}

/** "Go to …": the local parse first; otherwise whatever forms the server recognised. */
export function coordinateGroup(
  parsed: ParsedCoordinate | null,
  serverHits: readonly CoordinateHit[],
  act: ResultActions,
): ResultGroup | null {
  const items: ResultItem[] = parsed
    ? [
        {
          id: `coordinates:${parsed.position.lat},${parsed.position.lon}`,
          title: `Go to ${formatDecimal(parsed.position)}`,
          ...withDetail(coordinateDetail(parsed)),
          hint: 'Fly to',
          icon: CrosshairIcon,
          run: () => act.flyToPoint(parsed.position),
        },
      ]
    : serverHits.map((hit, i) => ({
        id: `coordinates:server:${i}:${hit.label}`,
        title: `Go to ${hit.label}`,
        ...withDetail(hit.format === 'mgrs' ? 'Read as MGRS' : null),
        hint: 'Fly to',
        icon: CrosshairIcon,
        run: () => act.flyToPoint(hit.position),
      }));
  return items.length > 0 ? { id: 'coordinates', heading: 'Coordinates', items } : null;
}

/** Geocoded places, with the geocoder's attribution under the group. */
export function placeGroup(
  places: readonly PlaceHit[],
  catalog: Catalog | null,
  act: ResultActions,
): ResultGroup | null {
  if (places.length === 0) return null;
  const footer = [...new Set(places.map((p) => p.sourceId))]
    .map((id) => catalog?.sources.find((s) => s.id === id)?.attribution ?? id)
    .join(' · ');
  return {
    id: 'places',
    heading: 'Places',
    footer,
    items: places.map((place, i) => ({
      id: `place:${i}:${place.name}:${place.position.lat},${place.position.lon}`,
      title: place.name,
      ...withDetail([place.context, place.kind].filter(Boolean).join(' · ')),
      hint: 'Fly to',
      icon: MapTrifoldIcon,
      run: () => (place.bbox ? act.flyToBox(place.bbox) : act.flyToPoint(place.position)),
    })),
  };
}

const ENTITY_ICONS: Readonly<Record<FeedId, Icon>> = {
  aircraft: AirplaneIcon,
  earthquakes: WaveformIcon,
  satellites: PlanetIcon,
  cyclones: WaveformIcon,
};

/** Live entities: local matches first, then the server's, each entity once. */
export function entityGroup(
  local: readonly EntityHit[],
  remote: readonly EntityHit[],
  act: ResultActions,
): ResultGroup | null {
  const seen = new Set<string>();
  const hits = [...local, ...remote].filter((hit) => {
    const key = `${hit.feed}:${hit.id}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  if (hits.length === 0) return null;
  return {
    id: 'entities',
    heading: 'Live entities',
    items: hits.map((hit) => ({
      id: `entity:${hit.feed}:${hit.id}`,
      title: hit.label,
      ...withDetail(hit.detail),
      hint: hit.position ? 'Select and fly to' : 'Select',
      icon: lookup(ENTITY_ICONS, hit.feed, StackIcon),
      run: () => act.selectEntity(hit),
    })),
  };
}

/** Turn layers on or off by name; unavailable layers are listed but cannot be chosen. */
export function layerGroup(
  catalog: Catalog | null,
  states: Partial<Record<FeedId, LayerState>>,
  query: string,
  act: ResultActions,
): ResultGroup | null {
  const q = query.trim().toLowerCase();
  const layers = (catalog?.layers ?? []).filter(
    (l) => q === '' || l.name.toLowerCase().includes(q) || l.feed.includes(q),
  );
  if (layers.length === 0) return null;
  return {
    id: 'layers',
    heading: 'Layers',
    items: layers.map((layer) => {
      const on = states[layer.feed]?.enabled ?? false;
      const available = layer.availability.state === 'available';
      return {
        id: `layer:${layer.feed}`,
        title: `${on ? 'Turn off' : 'Turn on'} ${layer.name}`,
        ...withDetail(available ? null : 'Not available: see Sources'),
        hint: on ? 'Turn off' : 'Turn on',
        icon: StackIcon,
        disabled: !available,
        run: () => act.setLayerEnabled(layer.feed, !on),
      };
    }),
  };
}

/** The registry's palette actions that match the query, with their shortcuts. */
export function actionGroup(query: string, act: ResultActions): ResultGroup | null {
  const commands = COMMANDS.filter((c) => c.inPalette && matchesQuery(c, query));
  if (commands.length === 0) return null;
  return {
    id: 'actions',
    heading: 'Actions',
    items: commands.map((command) => ({
      id: `action:${command.id}`,
      title: command.title,
      hint: 'Run',
      icon: CrosshairIcon,
      ...(command.shortcut ? { shortcut: shortcutLabel(command.shortcut) } : {}),
      disabled: !act.isEnabled(command),
      run: () => act.run(command),
    })),
  };
}
