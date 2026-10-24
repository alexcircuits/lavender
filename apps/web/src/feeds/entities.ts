/**
 * Engine-free helpers over snapshot items, shared by the feed controller, the globe
 * renderers and the inspector.
 */
import type {
  Aircraft,
  BikeshareStation,
  Cyclone,
  Earthquake,
  FeedId,
  FeedItems,
  FireCell,
  FirePerimeter,
  Launch,
  SatelliteElements,
  TransitVehicle,
  Vessel,
} from '../contracts';
import { fireCellId, isValidFireCell } from './fireCells';

export type FeedItem =
  | Aircraft
  | Earthquake
  | SatelliteElements
  | Cyclone
  | FireCell
  | Launch
  | Vessel
  | FirePerimeter
  | BikeshareStation
  | TransitVehicle;

/**
 * The stable id of an item within its feed, or null when the item has none (a fire cell that
 * fails the fire validity rule: it is neither drawn nor selectable).
 */
export function itemId(items: FeedItems, item: FeedItem): string | null {
  switch (items.kind) {
    case 'aircraft':
      return (item as Aircraft).hex;
    case 'earthquakes':
      return (item as Earthquake).id;
    case 'satellites':
      return String((item as SatelliteElements).noradId);
    case 'cyclones':
      return (item as Cyclone).id;
    case 'fires':
      return isValidFireCell(item) ? fireCellId(item) : null;
    case 'launches':
      return (item as Launch).id;
    case 'vessels':
      return (item as Vessel).mmsi;
    case 'perimeters':
      return (item as FirePerimeter).id;
    case 'bikeshare':
      return (item as BikeshareStation).stationId;
    case 'transit':
      return (item as TransitVehicle).vehicleId;
    default:
      return null;
  }
}

export function itemsOf(items: FeedItems): readonly FeedItem[] {
  return items.items;
}

/** Finds an item by id in a snapshot's items. */
export function findItem(items: FeedItems, id: string): FeedItem | undefined {
  return itemsOf(items).find((it) => itemId(items, it) === id);
}

/**
 * Feeds whose server-side filtering uses the viewport. Satellites are global objects, and
 * launches are a short worldwide list: their list must not change as the camera moves.
 */
export function usesViewport(feed: FeedId): boolean {
  return (
    feed === 'aircraft' ||
    feed === 'earthquakes' ||
    feed === 'fires' ||
    feed === 'vessels' ||
    feed === 'perimeters'
  );
}
